//! Expr → LaTeX.
//!
//! Esto es **serialización, no representación** (02-architecture.md §1): produce
//! la cadena LaTeX que denota la expresión, y no decide tipografía, tamaño,
//! color ni layout. Quién la renderiza, y con qué, es asunto de la capa de TS.
//!
//! El otro requisito es el round-trip: `parse(latex(e)) == e` es la propiedad
//! más importante del proyecto (05-testing.md §3). Si LaTeX es la única entrada
//! y el round-trip falla, todo lo demás es humo.

use ms_core::{Arena, Builtin, ExprId, Head, Node};
use num_bigint::BigInt;
use num_traits::Signed;

use crate::error::SerializeError;

/// Precedencia para decidir paréntesis. Mayor liga más fuerte.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    /// El `=` de una ecuación, que liga menos que todo lo demás: en
    /// `x^{2}-5x+6 = 0` la suma entera está a un lado del igual, no al revés.
    Relation,
    Sum,
    Product,
    Power,
    Atom,
}

pub fn to_latex(arena: &Arena, id: ExprId) -> Result<String, SerializeError> {
    write_latex(arena, id, Precedence::Relation)
}

fn write_latex(
    arena: &Arena,
    id: ExprId,
    context: Precedence,
) -> Result<String, SerializeError> {
    let node = arena.get(id).ok_or(SerializeError::UnknownExpr(id))?;

    match node {
        Node::Int(value) => Ok(parenthesize_if_negative(value.to_string(), context)),
        Node::Rat(value) => {
            // El signo va fuera de la fracción: -\frac{1}{2}, nunca \frac{-1}{2}.
            let magnitude = format!(
                "\\frac{{{}}}{{{}}}",
                value.numer().magnitude(),
                value.denom().magnitude()
            );
            let signed = if value.numer().is_negative() {
                format!("-{magnitude}")
            } else {
                magnitude
            };
            Ok(group_fraction(signed, context))
        }
        Node::Sym(sym) => arena
            .symbol_name(*sym)
            .map(str::to_owned)
            .ok_or(SerializeError::UnknownSymbol(*sym)),
        Node::App { head, args } => {
            let args = arena
                .args_of(*args)
                .ok_or(SerializeError::UnknownExpr(id))?
                .to_vec();
            match head {
                Head::Plus => write_sum(arena, &args, context),
                Head::Times => write_product(arena, &args, context),
                Head::Power => write_power(arena, &args, context),
                Head::Fn(builtin) => write_function(arena, *builtin, &args, context),
                Head::Equal => write_equation(arena, &args, context),
                Head::Solutions => write_solutions(arena, &args),
            }
        }
    }
}

/// Un literal negativo necesita paréntesis dentro de un producto o una potencia:
/// `x^{-2}` está bien, pero `2 \cdot -3` no se lee.
fn parenthesize_if_negative(rendered: String, context: Precedence) -> String {
    if context >= Precedence::Product && rendered.starts_with('-') {
        return format!("\\left({rendered}\\right)");
    }
    rendered
}

/// Una fracción ya es un grupo visual: dentro de una suma o de un producto no
/// necesita paréntesis, y ponérselos sólo añade ruido.
///
/// **Como base de una potencia sí los necesita.** `\frac{14}{9}^{-1}` es LaTeX
/// válido que se renderiza sin protestar y que no dice qué se está elevando; el
/// estudiante lee una cosa y el motor calculó otra. Salió a la luz en el primer
/// paso del criterio de salida de E1, donde se invierte `\frac{14}{9}`.
fn group_fraction(rendered: String, context: Precedence) -> String {
    let negative_inside_a_product =
        context >= Precedence::Product && rendered.starts_with('-');
    if context >= Precedence::Atom || negative_inside_a_product {
        return format!("\\left({rendered}\\right)");
    }
    rendered
}

/// Una suma, con sus términos **de mayor a menor grado**.
///
/// El orden canónico no vale para leer, y no porque esté mal: ordena por rango
/// estructural, así que `x^{2}-5x+6` sale `6 - 5x + x^{2}` —el `6` es un `Int`,
/// que es el rango 0— y una fracción sin reducir se cuela delante o detrás según
/// esté escrita como nodo o como producto. El orden de lectura acaba siendo un
/// accidente de cómo se guardó el número.
///
/// Aquí se ordena por grado porque **es como se escribe un polinomio**, y esto
/// es el escritor: decidir cómo se denota es su trabajo, y el orden canónico
/// —que sí es del kernel— no se toca. El round-trip aguanta porque `Plus` es
/// orderless: la canonicalización vuelve a ponerlo como estaba.
///
/// La ordenación es estable, así que dos términos del mismo grado conservan
/// entre sí el orden canónico y la salida sigue siendo determinista.
fn write_sum(
    arena: &Arena,
    args: &[ExprId],
    context: Precedence,
) -> Result<String, SerializeError> {
    let mut ordered = args.to_vec();
    ordered.sort_by_key(|arg| core::cmp::Reverse(reading_degree(arena, *arg)));

    let mut rendered = String::new();
    for (position, arg) in ordered.iter().enumerate() {
        let term = write_latex(arena, *arg, Precedence::Sum)?;
        // Un término negativo se escribe restando, no sumando un negativo.
        if position == 0 {
            rendered.push_str(&term);
        } else if let Some(magnitude) = term.strip_prefix('-') {
            rendered.push_str(" - ");
            rendered.push_str(magnitude);
        } else {
            rendered.push_str(" + ");
            rendered.push_str(&term);
        }
    }
    Ok(wrap(rendered, Precedence::Sum, context))
}

/// El grado de un término, **sólo para decidir en qué orden se lee**.
///
/// No es el grado algebraico y no pretende serlo: `ms-alg` es quien sabe de
/// polinomios, y `ms-serialize` no puede depender de él sin abrir una arista
/// lateral que `deny.toml` prohíbe por buenas razones. Lo que hace falta aquí es
/// mucho menos —un criterio total, barato y estable para ordenar términos— y
/// equivocarse sólo cambia el orden de lectura, nunca el valor.
///
/// Satura en vez de desbordar: un exponente absurdo ordena como muy grande, que
/// es lo correcto, en lugar de dar la vuelta y colarse al final.
fn reading_degree(arena: &Arena, id: ExprId) -> u32 {
    match arena.get(id) {
        Some(Node::Sym(_)) => 1,
        Some(Node::App { head, args }) => {
            let Some(args) = arena.args_of(*args) else {
                return 0;
            };
            match head {
                Head::Plus => args
                    .iter()
                    .map(|arg| reading_degree(arena, *arg))
                    .max()
                    .unwrap_or(0),
                Head::Times => args
                    .iter()
                    .fold(0, |total, arg| total.saturating_add(reading_degree(arena, *arg))),
                Head::Power => power_reading_degree(arena, args),
                // Una llamada, una ecuación y un conjunto no tienen grado. Un
                // término que no lo tiene se lee al final, con las constantes.
                Head::Fn(_) | Head::Equal | Head::Solutions => 0,
            }
        }
        // Un número es de grado 0, y lo que no está en la arena tampoco tiene
        // grado: ordenarlo al final es la respuesta inofensiva.
        Some(Node::Int(_) | Node::Rat(_)) | None => 0,
    }
}

fn power_reading_degree(arena: &Arena, args: &[ExprId]) -> u32 {
    let [base, exponent] = args else {
        return 0;
    };
    // Un exponente negativo baja al denominador, y un denominador no sube el
    // grado del término: `\frac{x}{y}` se lee donde se lee `x`.
    let Some(Node::Int(exponent)) = arena.get(*exponent) else {
        return 0;
    };
    let Ok(exponent) = u32::try_from(exponent.clone()) else {
        return 0;
    };
    reading_degree(arena, *base).saturating_mul(exponent)
}

/// Los factores con exponente negativo bajan al denominador, que es lo que
/// convierte `Times(n, Power(d, -1))` en `\frac{n}{d}`. Esa es la razón de que
/// el IR no necesite una cabeza `Divide` propia.
fn write_product(
    arena: &Arena,
    args: &[ExprId],
    context: Precedence,
) -> Result<String, SerializeError> {
    let mut numerator: Vec<ExprId> = Vec::new();
    let mut denominator: Vec<ExprId> = Vec::new();

    for arg in args {
        match reciprocal_base(arena, *arg)? {
            Some(base) => denominator.push(base),
            None => numerator.push(*arg),
        }
    }

    // Con dos o más factores abajo, `\frac` rompería el round-trip: `\frac{1}{3x}`
    // vuelve a entrar como `Power(Times(3, x), -1)`, que no es el
    // `Times(Power(3,-1), Power(x,-1))` del que salió. Mientras el motor no tenga
    // una forma de salida propia (E2, ms-simplify), la fidelidad manda sobre la
    // estética y esos factores se escriben con su exponente negativo.
    if denominator.is_empty() || denominator.len() > 1 {
        let all_factors = args
            .iter()
            .enumerate()
            .map(|(position, arg)| write_factor(arena, *arg, position == 0))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(wrap(
            join_factors(&all_factors),
            Precedence::Product,
            context,
        ));
    }

    Ok(group_fraction(
        format!(
            "\\frac{{{}}}{{{}}}",
            write_fraction_side(arena, &numerator)?,
            write_fraction_side(arena, &denominator)?
        ),
        context,
    ))
}

/// Un factor de un producto, sabiendo si es el primero.
///
/// **Un literal negativo al frente no necesita paréntesis.** `-5x` es como se
/// escribe un coeficiente negativo, y `\left(-5\right)x` es lo mismo escrito
/// como no lo escribe nadie: el estudiante lo lee como una multiplicación por un
/// número entre paréntesis en vez de como un término con signo.
///
/// Después del primero sí hacen falta, y ésa es la razón de que existan:
/// `2 \cdot -3` no se lee. Por eso la excepción mira la posición y no el signo.
///
/// El orden canónico pone los números delante —`Int` es el rango 0— así que el
/// coeficiente de un término siempre cae en la posición donde esto se aplica.
fn write_factor(
    arena: &Arena,
    id: ExprId,
    first: bool,
) -> Result<String, SerializeError> {
    let numeric = matches!(arena.get(id), Some(Node::Int(_) | Node::Rat(_)));
    let context = if first && numeric {
        Precedence::Sum
    } else {
        Precedence::Product
    };
    write_latex(arena, id, context)
}

/// Un lado de la fracción, dentro de las llaves del `\frac`.
///
/// Con un solo factor las llaves **ya son el grupo**, así que ese factor no
/// necesita paréntesis: ni por ser una suma ni por ser un literal negativo. Sin
/// esto, `\frac{3}{4}-\frac{5}{6}` se escribía `\frac{\left(-1\right)}{12}`, que
/// es LaTeX válido y aritmética que el estudiante lee mal.
///
/// El signo se queda **dentro** del numerador y no se iza fuera de la fracción,
/// al contrario que en la rama `Node::Rat`. No es un descuido: `-\frac{1}{12}`
/// vuelve a entrar como `Times(-1, 1, Power(12,-1))` —el `1` sobrante lo aporta
/// `\frac{1}{12}`— y eso no es el `Times(-1, Power(12,-1))` del que salió. Izar
/// el signo exige antes que la canonicalización elimine los elementos neutros,
/// que es para lo que existe `Head::identity_element` y todavía nadie usa.
fn write_fraction_side(arena: &Arena, factors: &[ExprId]) -> Result<String, SerializeError> {
    if factors.is_empty() {
        return Ok("1".to_owned());
    }
    if let [only] = factors {
        return write_latex(arena, *only, Precedence::Sum);
    }

    let rendered = factors
        .iter()
        .map(|factor| write_latex(arena, *factor, Precedence::Product))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(join_factors(&rendered))
}

/// `\cdot` sólo donde la yuxtaposición se leería como otro número: `2 \cdot 3`
/// es necesario, `2x` no.
///
/// Un factor que termina en `}` también cierra un número: `12^{-1}` seguido de
/// `14^{-1}` se escribía `12^{-1}14^{-1}`, donde el `-1` y el `14` se leen
/// pegados y el estudiante ve un número que no existe. Cuenta el último carácter
/// *visible* del factor previo, no el de la cadena.
fn join_factors(factors: &[String]) -> String {
    let mut rendered = String::new();
    let mut previous_ends_a_number = false;

    for factor in factors {
        if previous_ends_a_number && starts_a_number(factor) {
            rendered.push_str(" \\cdot ");
        }
        rendered.push_str(factor);
        previous_ends_a_number = ends_a_number(factor);
    }
    rendered
}

/// Si lo primero que se lee del factor es un número: un dígito, **o una
/// fracción**.
///
/// `\frac` no empieza por dígito, así que `Times(2, \frac{1}{3}, x)` se escribía
/// `2\frac{1}{3}x` — que es un producto y se lee como un número mixto. Este
/// motor no acepta mixtas de entrada, y precisamente por eso no puede escribir
/// algo que las imite: el estudiante leería `2\frac{1}{3}` como `\frac{7}{3}` y
/// el valor es `\frac{2}{3}`. Es el tercer defecto de esta familia —el valor
/// correcto y la lectura equivocada—, después de `12^{-1}14^{-1}` y de
/// `\frac{14}{9}^{-1}` sin agrupar.
fn starts_a_number(factor: &str) -> bool {
    factor.starts_with(|character: char| character.is_ascii_digit())
        || factor.starts_with("\\frac")
}

/// Si lo último que se lee del factor es un número: o un dígito suelto, o un
/// grupo cerrado (`^{-1}`, `\frac{..}{..}`) cuyo contenido acaba en dígito.
fn ends_a_number(factor: &str) -> bool {
    match factor.chars().last() {
        Some(last) if last.is_ascii_digit() => true,
        Some('}') => factor
            .chars()
            .rev()
            .skip(1)
            .find(|character| *character != '}')
            .is_some_and(|character| character.is_ascii_digit()),
        Some(_) | None => false,
    }
}

/// Si `id` es `Power(base, -1)`, devuelve la base: ese factor va al denominador.
fn reciprocal_base(arena: &Arena, id: ExprId) -> Result<Option<ExprId>, SerializeError> {
    let Some(Head::Power) = arena.head_of(id) else {
        return Ok(None);
    };
    let args = arena.app_args(id).ok_or(SerializeError::UnknownExpr(id))?;
    let [base, exponent] = args else {
        return Ok(None);
    };
    let (base, exponent) = (*base, *exponent);

    let Some(Node::Int(value)) = arena.get(exponent) else {
        return Ok(None);
    };
    if *value == BigInt::from(-1) {
        return Ok(Some(base));
    }
    Ok(None)
}

/// `\operatorname{redondear}\left(3.456, 2\right)`.
///
/// `\operatorname` y no una macro inventada: es el comando con el que LaTeX
/// escribe una función con nombre, y lo tipografía derecho para distinguirlo del
/// producto de las variables `r`, `e`, `d`… Los paréntesis van siempre, aunque
/// haya un solo argumento: una llamada sin ellos se lee como una yuxtaposición.
///
/// El resultado es un átomo, así que no necesita `wrap`: nada de lo que lo
/// contenga puede partirlo.
fn write_function(
    arena: &Arena,
    builtin: Builtin,
    args: &[ExprId],
    context: Precedence,
) -> Result<String, SerializeError> {
    if args.len() != builtin.arity() {
        return Err(SerializeError::MalformedCall {
            function: builtin.name(),
            arity: args.len(),
        });
    }

    // La única que no es un átomo: `\frac{d}{dx}` es un operador prefijo y su
    // contexto decide si necesita paréntesis.
    if builtin == Builtin::Derivative {
        return write_derivative(arena, args, context);
    }

    let arguments = args
        .iter()
        // Cada argumento se escribe como si estuviera solo: las comas y los
        // paréntesis de la llamada ya lo separan de sus vecinos.
        .map(|arg| write_latex(arena, *arg, Precedence::Sum))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(format!(
        "\\operatorname{{{}}}\\left({}\\right)",
        builtin.name(),
        arguments.join(", ")
    ))
}

/// `\frac{d}{dx}\left(x^{2}\right)`.
///
/// **La única función que no se escribe con `\operatorname`**, y la excepción se
/// gana: `\frac{d}{dx}` es como se escribe una derivada en cualquier cuaderno, y
/// `\operatorname{derivar}\left(x^{2}, x\right)` no lo es en ninguno. La forma
/// canónica del IR sigue siendo la llamada —el parser acepta las dos— pero lo que
/// sale a la pantalla es esto.
///
/// **Los paréntesis del operando van siempre.** Sin ellos, `\frac{d}{dx}x + 1` se
/// lee como la derivada de `x`, más uno; y con el operando dentro no hay duda de
/// dónde acaba. Es la misma razón por la que una llamada con nombre lleva los
/// suyos aunque tenga un solo argumento.
///
/// **Y el operador entero no es un átomo, aunque su operando vaya entre
/// paréntesis.** Liga como el menos unario: se lleva la potencia que tenga
/// delante, así que bajo un exponente hay que envolverlo. Sin eso,
/// `Power(derivada, 2)` se escribe `\frac{d}{dx}\left(f\right)^{2}` y vuelve a
/// entrar como la derivada de `f^{2}`, que es otra expresión. Lo encontró el
/// round-trip, que es exactamente para lo que está.
fn write_derivative(
    arena: &Arena,
    args: &[ExprId],
    context: Precedence,
) -> Result<String, SerializeError> {
    let [operand, variable] = args else {
        return Err(SerializeError::MalformedCall {
            function: Builtin::Derivative.name(),
            arity: args.len(),
        });
    };
    // La variable se escribe como el símbolo que es: pegada a la `d` del
    // denominador, `dx`, que es lo que el parser vuelve a reconocer.
    let variable = write_latex(arena, *variable, Precedence::Atom)?;
    let operand = write_latex(arena, *operand, Precedence::Relation)?;

    // `Product` y no `Atom`: dentro de un producto se lee sin paréntesis
    // —`y\frac{d}{dx}\left(x\right)` es el producto que parece—, y sólo la
    // potencia obliga a envolverlo.
    Ok(wrap(
        format!("\\frac{{d}}{{d{variable}}}\\left({operand}\\right)"),
        Precedence::Product,
        context,
    ))
}

/// `x^{2}-5x+6 = 0`.
///
/// Los dos lados se escriben en contexto de suma, no de relación: dentro de un
/// lado no puede haber otro `=` —el parser sólo lo acepta donde una ecuación
/// puede vivir— y pedir `Relation` invitaría a anidarlos.
fn write_equation(
    arena: &Arena,
    args: &[ExprId],
    context: Precedence,
) -> Result<String, SerializeError> {
    let [left, right] = args else {
        return Err(SerializeError::MalformedEquation { arity: args.len() });
    };
    let left = write_latex(arena, *left, Precedence::Sum)?;
    let right = write_latex(arena, *right, Precedence::Sum)?;
    Ok(wrap(
        format!("{left} = {right}"),
        Precedence::Relation,
        context,
    ))
}

/// `\left\{x = 2, x = 3\right\}`.
///
/// Las llaves van porque es un conjunto y así se escribe en clase, y son lo que
/// el parser reconoce para volver a leerlo: el escritor no puede emitir nada que
/// la entrada no sepa recuperar (05-testing.md §3).
///
/// El resultado es un átomo —las llaves lo delimitan— así que no necesita `wrap`.
fn write_solutions(arena: &Arena, args: &[ExprId]) -> Result<String, SerializeError> {
    let solutions = args
        .iter()
        .map(|arg| write_latex(arena, *arg, Precedence::Relation))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(format!("\\left\\{{{}\\right\\}}", solutions.join(", ")))
}

fn write_power(
    arena: &Arena,
    args: &[ExprId],
    context: Precedence,
) -> Result<String, SerializeError> {
    let [base, exponent] = args else {
        return Err(SerializeError::MalformedPower { arity: args.len() });
    };
    let base = write_latex(arena, *base, Precedence::Atom)?;
    // Las llaves hacen innecesarios los paréntesis en el exponente.
    let exponent = write_latex(arena, *exponent, Precedence::Sum)?;
    Ok(wrap(
        format!("{base}^{{{exponent}}}"),
        Precedence::Power,
        context,
    ))
}

fn wrap(rendered: String, own: Precedence, context: Precedence) -> String {
    if own < context {
        return format!("\\left({rendered}\\right)");
    }
    rendered
}
