//! **El** conjunto de reglas del motor, en orden de intento.
//!
//! Uno solo, y en un solo sitio, porque el orden es semántico: repartirlo en
//! varios `rules!` haría invisible cómo se intercalan los conjuntos, que es
//! justo la propiedad que este módulo existe para hacer legible de un vistazo.
//! Los algoritmos viven en los módulos temáticos —`fraction`, `polynomial`—; lo
//! que hay aquí es la lista ordenada y el cuerpo de cada regla.
//!
//! **El orden de declaración es el orden de intento**, y no es cosmético:
//!
//! - El recíproco antes que el producto: mientras haya un `\left(\frac{c}{d}
//!   \right)^{-1}` en medio, el producto no reconoce una fracción y no puede
//!   juntar nada.
//! - El producto antes que la simplificación: al revés, `\frac{1}{2} \cdot
//!   \frac{2}{3}` saltaría directo a `\frac{1}{3}` y el estudiante no vería
//!   multiplicar.
//! - Todo antes que los plegados de enteros, que son silenciosos. Una regla
//!   silenciosa declarada demasiado pronto hace el trabajo de otra y se lleva su
//!   paso por delante: ése fue exactamente el defecto que E0 dejó anotado.
//! - El denominador común antes que la suma, y la suma antes que la reducción.
//!
//! Los títulos están en español porque son **lo que lee el estudiante**. Cuando
//! el motor se internacionalice, lo que se traduce es esto, no los ids.

use ms_assume::Assumptions;
use ms_core::{Arena, Builtin, ExprId, Head, Node};
use ms_num::{
    Integer, Rational, checked_pow, gcd, lcm, round_half_away, round_half_even,
};
use ms_rules_macro::rules;
use num_traits::{One, ToPrimitive, Zero};

use crate::fraction::{as_fraction, make_fraction, rational_value};
use crate::polynomial;
use crate::rule::{RuleError, RuleResult};

/// ¿Es `id` el literal `-1`? Es el exponente que marca un denominador.
fn is_minus_one(arena: &Arena, id: ExprId) -> bool {
    matches!(arena.get(id), Some(ms_core::Node::Int(value)) if *value == -Integer::one())
}

/// ¿Es `id` el literal cero?
fn is_zero(arena: &Arena, id: ExprId) -> bool {
    matches!(arena.get(id), Some(ms_core::Node::Int(value)) if value.is_zero())
}

/// Los sumandos de una suma, separados en los que son fracciones y los que no.
///
/// **Separar en vez de exigir que todos lo sean** es lo que hace que
/// `\frac{1}{2} + x + \frac{1}{3}` se convierta en `\frac{5}{6} + x`. Antes, un
/// solo sumando simbólico bloqueaba toda la suma y el estudiante no veía combinar
/// lo que sí se podía combinar.
struct Summands {
    fractions: Vec<(Integer, Integer)>,
    /// Los sumandos que no son fracciones, tal cual. Vuelven al resultado sin
    /// tocar: combinar lo que se puede no autoriza a inventarse el resto.
    rest: Vec<ExprId>,
}

fn split_summands(arena: &mut Arena, id: ExprId) -> Result<Option<Summands>, RuleError> {
    if arena.head_of(id) != Some(Head::Plus) {
        return Ok(None);
    }
    let Some(args) = arena.app_args(id).map(<[ExprId]>::to_vec) else {
        return Ok(None);
    };

    let mut fractions = Vec::with_capacity(args.len());
    let mut rest = Vec::new();
    for arg in args {
        // Un producto que todavía se puede juntar en una sola fracción **no
        // cuenta como sumando racional**, aunque su valor lo sea. Si contara,
        // `\frac{1}{2} + \frac{1}{3}\cdot\frac{3}{4}` pondría denominador común
        // sobre un `\frac{3}{12}` que nadie ha visto aparecer: el orden de las
        // operaciones exige que la multiplicación se haga y **se vea** primero.
        let ready = collapse_product(arena, arg)?.is_none();
        match as_fraction(arena, arg).filter(|_| ready) {
            Some(fraction) => fractions.push(fraction),
            None => rest.push(arg),
        }
    }

    // Con un solo racional no hay nada que combinar, y reconstruir la suma con
    // los mismos sumandos haría que la regla se disparase sobre su propio
    // resultado para siempre.
    if fractions.len() < 2 {
        return Ok(None);
    }
    Ok(Some(Summands { fractions, rest }))
}

/// Cuántos factores de un producto van al denominador.
///
/// Un factor con cabeza `Power` dentro de algo que `as_fraction` reconoce es
/// siempre un `b^{-1}`: ésa es la única potencia que sabe reconocer.
fn denominator_factors(arena: &Arena, id: ExprId) -> usize {
    arena.app_args(id).map_or(0, |args| {
        args.iter()
            .filter(|arg| arena.head_of(**arg) == Some(Head::Power))
            .count()
    })
}

/// Un producto de fracciones juntado en una sola, o `None` si ya lo está.
///
/// Es a la vez el cuerpo de `rational-multiply` y la pregunta que las reglas de
/// la suma le hacen a cada sumando. Una sola definición: dos copias de esta
/// condición se desincronizarían y el orden de las operaciones dejaría de
/// respetarse en algún caso que nadie probó.
fn collapse_product(arena: &mut Arena, id: ExprId) -> Result<Option<ExprId>, RuleError> {
    if arena.head_of(id) != Some(Head::Times) {
        return Ok(None);
    }
    // Hacen falta **dos** denominadores para que esto sea multiplicar
    // fracciones. Con uno solo lo que hay son enteros que se pliegan en el
    // numerador: `\frac{3}{4} - \frac{5}{6}` deja el término negativo como
    // `(-1) \cdot 5 \cdot 6^{-1}`, y anunciar eso como «multiplicamos las
    // fracciones» sería ponerle título a un paso que nadie dio.
    if denominator_factors(arena, id) < 2 {
        return Ok(None);
    }
    let Some((numerator, denominator)) = as_fraction(arena, id) else {
        return Ok(None);
    };
    // Sin denominador esto es un producto de enteros, y de eso se encarga la
    // regla de plegado, que además es silenciosa: `2 \cdot 3` no merece un paso
    // titulado «multiplicamos las fracciones».
    if denominator.is_one() {
        return Ok(None);
    }

    let collapsed = make_fraction(arena, numerator, denominator)?;
    // La arena comparte estructura, así que un id igual significa que la
    // expresión ya estaba en esta forma y no hay nada que hacer. Devolver lo
    // mismo detendría la reescritura en seco.
    if collapsed == id {
        return Ok(None);
    }
    Ok(Some(collapsed))
}

/// ¿Puede lo que queda tras quitar los neutros escribirse solo?
///
/// En `Plus` siempre: la canonicalización desenvuelve `Plus(x)` a `x` y el
/// escritor sabe escribir cualquier sumando por su cuenta.
///
/// En `Times` casi siempre, y la excepción es de escritura, no de matemáticas:
/// `Times(1, b^{-1})` se escribe `\frac{1}{b}`, pero un `b^{-1}` **solo** se
/// escribe `b^{-1}`, porque es el producto —y no la potencia— quien sabe bajar
/// los exponentes negativos al denominador. Quitar ese `1` cambiaría lo que lee
/// el estudiante a peor, así que ahí se conserva.
///
/// **Antes esto era un mínimo de dos argumentos, y prohibía de más.** `1x` se
/// quedaba escrito así —lo destapó la regla del producto de E3, que produce
/// `\frac{d}{dx}\left(x\right)y` y un `1` en cuanto la derivada se resuelve— y
/// «uno por equis» es exactamente la clase de cosa que un estudiante no escribe.
/// La condición que hacía falta no era cuántos quedan, sino **qué** queda.
fn may_stand_alone(arena: &Arena, head: Head, kept: &[ExprId]) -> bool {
    match head {
        Head::Plus => true,
        Head::Times => match kept {
            [only] => arena.head_of(*only) != Some(Head::Power),
            _ => true,
        },
        // De aquí no se quita nada: ni una potencia ni una llamada tienen
        // argumentos de sobra, los tienen posicionales. Una ecuación tiene sus
        // dos lados y un conjunto sus soluciones; ninguno tiene neutro que
        // quitar, y `identity_element` ya devuelve `None`.
        Head::Power | Head::Fn(_) | Head::Equal | Head::Solutions => false,
    }
}

rules! {
    /// Quita los elementos neutros de una suma o un producto: el `+ 0` y el
    /// `\cdot 1` que sobran.
    ///
    /// Silenciosa, porque no hay nada que explicar. Y **la primera de todas**,
    /// porque un neutro colado dentro de un producto hace que otra regla crea que
    /// hay algo que multiplicar: `\frac{3}{4} - \frac{1}{4}` entra con el término
    /// negativo como `(-1) \cdot 1 \cdot 4^{-1}` —el `1` lo aporta el numerador
    /// de la fracción y el `-1`, la resta— y sin limpiarlo primero la traza gana
    /// un «Multiplicamos las fracciones» donde no se multiplicó nada.
    #[id = "identity-element"]
    #[title = "Quitamos los elementos neutros"]
    #[step = r"a \cdot 1 = a,\quad a + 0 = a"]
    #[silent]
    fn drop_identity_elements(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        let Some(head) = arena.head_of(id) else {
            return Ok(None);
        };
        let Some(identity) = head.identity_element() else {
            return Ok(None);
        };
        let Some(args) = arena.app_args(id).map(<[ExprId]>::to_vec) else {
            return Ok(None);
        };

        let identity = arena.int(i64::from(identity))?;
        let kept: Vec<ExprId> = args.iter().copied().filter(|arg| *arg != identity).collect();
        if kept.len() == args.len() || kept.is_empty() {
            return Ok(None);
        }
        if !may_stand_alone(arena, head, &kept) {
            return Ok(None);
        }
        Ok(Some(arena.app(head, &kept)?))
    }

    /// Una fracción de denominador 1 **es** su numerador.
    ///
    /// Mientras siga escrita como fracción, ninguna regla la trata como un
    /// número, y cada una tiene su buena razón para no hacerlo: `integer-power`
    /// se aparta del exponente `-1` porque así escribe el IR un denominador,
    /// `rational-reduce` se retira porque con denominador 1 no hay nada que
    /// simplificar, y `rational-add-numerators` delega en el plegado de enteros
    /// — que no la ve, porque un `Times(4, 1^{-1})` no es un `Int`.
    ///
    /// Entre las tres dejaban `\frac{1}{1} + \frac{-1}{1}` sin resolver: sin
    /// error y sin valor, que de las dos formas de fallar es la mala. Lo destapó
    /// la propiedad de conmutatividad de `laws.rs` con una semilla nueva, no un
    /// caso escrito a mano.
    ///
    /// Silenciosa: dividir entre uno no es un paso que se explique.
    #[id = "rational-unit-denominator"]
    #[title = "Dividir entre uno no cambia nada"]
    #[step = r"\frac{a}{1} = a"]
    #[silent]
    fn drop_unit_denominator(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        // Sólo sobre aplicaciones. Un `Int` ya es su propio numerador, y
        // reescribirlo a sí mismo es exactamente lo que el reescritor detecta
        // como ciclo.
        if arena.head_of(id).is_none() {
            return Ok(None);
        }
        let Some((numerator, denominator)) = as_fraction(arena, id) else {
            return Ok(None);
        };
        if !denominator.is_one() {
            return Ok(None);
        }
        Ok(Some(arena.int(numerator)?))
    }

    /// El recíproco de una fracción es la fracción dada la vuelta.
    ///
    /// **No hay ningún `\div` que reescribir**: el parser ya desazucara `a ÷ b` a
    /// `Times(a, Power(b, -1))`, así que lo que queda en el árbol es el recíproco
    /// de un cociente, `\left(\frac{c}{d}\right)^{-1}`. Ese es el nodo que esta
    /// regla desmonta, y el paso que produce es el que se enseña en clase.
    ///
    /// Es la primera regla que usa el contexto de suposiciones para algo real:
    /// invertir `\frac{c}{d}` exige `c \neq 0`, y ante la duda no se invierte.
    #[id = "rational-reciprocal"]
    #[soundness = lean_local("MathSlice.rational_reciprocal_sound")]
    #[title = "Dividir es multiplicar por el recíproco"]
    #[step = r"\frac{a}{b} \div \frac{c}{d} = \frac{a}{b} \cdot \frac{d}{c}"]
    fn reciprocal_of_a_fraction(
        arena: &mut Arena,
        id: ExprId,
        assumptions: &Assumptions,
    ) -> RuleResult {
        if arena.head_of(id) != Some(Head::Power) {
            return Ok(None);
        }
        let args = arena.app_args(id).map(<[ExprId]>::to_vec);
        let Some([base, exponent]) = args.as_deref() else {
            return Ok(None);
        };
        let (base, exponent) = (*base, *exponent);
        if !is_minus_one(arena, exponent) {
            return Ok(None);
        }

        // Un **entero** elevado a -1 ya es la forma normal de un denominador:
        // darle la vuelta lo devolvería a su sitio y el reescritor ciclaría.
        //
        // La condición es sobre la forma del nodo y no sobre su valor. Mirar si
        // el denominador vale 1 parece equivalente y no lo es:
        // `\left(\frac{-3}{1}\right)^{-1}` tiene denominador 1 y **no** es un
        // entero elevado a -1, así que quedaba sin invertir y la división se
        // atascaba sin llegar a ningún número. Lo encontró la propiedad de
        // `tests/laws.rs`, no un caso escrito a mano.
        if matches!(arena.get(base), Some(ms_core::Node::Int(_))) {
            return Ok(None);
        }
        let Some((numerator, denominator)) = as_fraction(arena, base) else {
            return Ok(None);
        };

        // Sin la certeza de que la base no es cero no se invierte nada. Es la
        // política restrictiva: no invertir deja la expresión como estaba;
        // invertir un cero devuelve una respuesta que no existe.
        //
        // Hoy el reescritor atrapa `\left(\frac{0}{9}\right)^{-1}` antes de
        // llegar aquí. La comprobación se queda igualmente: una regla que sólo
        // es correcta porque otro comprueba primero deja de serlo en cuanto
        // cambia el orden de las reglas, y eso no lo avisa ningún test.
        if !assumptions.is_known_nonzero(arena, base) {
            return Ok(None);
        }

        Ok(Some(make_fraction(arena, denominator, numerator)?))
    }

    /// Un producto de fracciones se junta en una sola: numeradores arriba,
    /// denominadores abajo.
    ///
    /// **No cancela en cruz a propósito.** `\frac{1}{2} \cdot \frac{2}{3}` pasa
    /// por `\frac{2}{6}` y de ahí a `\frac{1}{3}` con «Simplificamos la
    /// fracción», que es un paso que el estudiante ve. Cancelar dentro de esta
    /// regla daría el mismo número escondiendo la mitad del razonamiento, que es
    /// justo el defecto que esta regla viene a cerrar.
    #[id = "rational-multiply"]
    #[soundness = lean_local("MathSlice.rational_multiply_sound")]
    #[title = "Multiplicamos las fracciones"]
    #[step = r"\frac{a}{b} \cdot \frac{c}{d} = \frac{a \cdot c}{b \cdot d}"]
    fn multiply_fractions(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        collapse_product(arena, id)
    }

    /// Una potencia de exponente entero se calcula, sobre bignum y exacta.
    ///
    /// **El exponente `-1` no entra aquí**: es como el IR escribe un
    /// denominador, y calcularlo devolvería `\frac{1}{b}` sobre sí mismo para
    /// siempre. De ese caso se encargan la regla del recíproco y la fracción.
    ///
    /// Cuando el resultado excede `ms_num::MAX_EXACT_BITS` la regla **falla en
    /// vez de calcular**. No es una precaución de más: es el único freno que hay
    /// contra `2^{10^{9}}`, que no cicla y por tanto no lo detiene el
    /// presupuesto de iteraciones.
    #[id = "integer-power"]
    #[title = "Elevamos a la potencia"]
    #[step = r"\left(\frac{a}{b}\right)^{n} = \frac{a^{n}}{b^{n}}"]
    fn integer_power(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        if arena.head_of(id) != Some(Head::Power) {
            return Ok(None);
        }
        let args = arena.app_args(id).map(<[ExprId]>::to_vec);
        let Some([base, exponent]) = args.as_deref() else {
            return Ok(None);
        };
        let (base, exponent) = (*base, *exponent);
        if is_minus_one(arena, exponent) {
            return Ok(None);
        }

        let Some(ms_core::Node::Int(exponent)) = arena.get(exponent) else {
            return Ok(None);
        };
        let exponent = exponent.clone();
        let Some(value) = as_fraction(arena, base).map(|(n, d)| Rational::new(n, d)) else {
            return Ok(None);
        };

        // `None` aquí es «no se va a calcular esto»: o la magnitud se dispara, o
        // la base es cero con un exponente que la deja indefinida. La segunda ya
        // la reporta el reescritor como división entre cero, así que lo que llega
        // hasta aquí es la magnitud.
        let Some(power) = checked_pow(&value, &exponent) else {
            return Err(RuleError::MagnitudeLimit);
        };
        Ok(Some(make_fraction(
            arena,
            power.numer().clone(),
            power.denom().clone(),
        )?))
    }

    /// Un producto con un factor nulo vale cero, sin mirar los demás.
    ///
    /// Es un paso con título propio y no un plegado silencioso: «el cero absorbe»
    /// es una propiedad que se enseña, no aritmética que el lector ya hizo.
    #[id = "product-with-zero"]
    #[soundness = lean_local("MathSlice.product_with_zero_sound")]
    #[title = "El producto por cero es cero"]
    #[step = r"a \cdot 0 \cdot b = 0"]
    fn product_with_zero(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        if arena.head_of(id) != Some(Head::Times) {
            return Ok(None);
        }
        let Some(args) = arena.app_args(id).map(<[ExprId]>::to_vec) else {
            return Ok(None);
        };
        if !args.iter().any(|arg| is_zero(arena, *arg)) {
            return Ok(None);
        }
        Ok(Some(arena.int(Integer::zero())?))
    }

    /// `\frac{a}{b} + \frac{c}{d}` con `b ≠ d`: se reescriben ambos sobre
    /// `mcm(b, d)`. Usa el mínimo común múltiplo y no el producto porque
    /// `\frac{1}{6}+\frac{2}{3}` se resuelve sobre 6, no sobre 18: el paso que
    /// se enseña en clase.
    #[id = "rational-common-denominator"]
    #[title = "Buscamos denominador común"]
    #[step = r"\frac{a}{b} + \frac{c}{d} = \frac{a\,m/b}{m} + \frac{c\,m/d}{m},\quad m = \operatorname{mcm}(b,d)"]
    fn common_denominator(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        let Some(summands) = split_summands(arena, id)? else {
            return Ok(None);
        };

        let mut common = Integer::one();
        for (_, denominator) in &summands.fractions {
            common = lcm(&common, denominator);
        }

        // Si ya comparten denominador no hay nada que hacer, y decirlo con un
        // paso sería mentir sobre el trabajo hecho.
        if summands
            .fractions
            .iter()
            .all(|(_, denominator)| *denominator == common)
        {
            return Ok(None);
        }

        let mut terms = Vec::with_capacity(summands.fractions.len() + summands.rest.len());
        for (numerator, denominator) in summands.fractions {
            let scale = &common / &denominator;
            terms.push(make_fraction(arena, numerator * scale, common.clone())?);
        }
        terms.extend(summands.rest);
        Ok(Some(arena.plus(&terms)?))
    }

    /// Sumandos con el mismo denominador: se suman los numeradores.
    #[id = "rational-add-numerators"]
    #[soundness = lean_local("MathSlice.rational_add_sound")]
    #[title = "Sumamos los numeradores"]
    #[step = r"\frac{a}{m} + \frac{c}{m} = \frac{a + c}{m}"]
    fn add_numerators(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        let Some(summands) = split_summands(arena, id)? else {
            return Ok(None);
        };

        let Some((_, common)) = summands.fractions.first().cloned() else {
            return Ok(None);
        };
        // Con denominador 1 esto es una suma de enteros, y de eso se encarga la
        // regla de plegado, que además es silenciosa.
        if common.is_one() {
            return Ok(None);
        }
        if summands
            .fractions
            .iter()
            .any(|(_, denominator)| *denominator != common)
        {
            return Ok(None);
        }

        let total = summands
            .fractions
            .into_iter()
            .fold(Integer::zero(), |sum, (numerator, _)| sum + numerator);
        let combined = make_fraction(arena, total, common)?;

        // Los sumandos que no son racionales vuelven intactos: se ha combinado lo
        // que se podía combinar, y nada más.
        if summands.rest.is_empty() {
            return Ok(Some(combined));
        }
        let mut terms = summands.rest;
        terms.push(combined);
        Ok(Some(arena.plus(&terms)?))
    }

    /// `\frac{a}{b}` con `mcd(a, b) > 1`: se divide arriba y abajo.
    #[id = "rational-reduce"]
    #[soundness = lean_local("MathSlice.rational_reduce_sound")]
    #[title = "Simplificamos la fracción"]
    #[step = r"\frac{a}{b} = \frac{a/g}{b/g},\quad g = \operatorname{mcd}(a,b)"]
    fn reduce_fraction(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        let Some((numerator, denominator)) = as_fraction(arena, id) else {
            return Ok(None);
        };
        if denominator.is_one() {
            return Ok(None);
        }

        let divisor = gcd(&numerator, &denominator);
        if divisor.is_one() {
            return Ok(None);
        }
        Ok(Some(make_fraction(
            arena,
            numerator / &divisor,
            denominator / &divisor,
        )?))
    }

    /// `\operatorname{redondear}(x, n)` con `x` racional y `n` entero.
    ///
    /// Es la primera regla del motor que **pierde información a propósito**, y
    /// por eso tiene paso propio con título: redondear no es aritmética que el
    /// lector ya hizo, es una decisión sobre qué se tira.
    ///
    /// El cálculo lo hace `ms_num`, que es también quien redondea para el
    /// verificador. No es un atajo: dos copias del criterio de desempate
    /// discreparían en el `.5`, que es el único caso interesante, y entonces el
    /// motor calcularía una cosa y la comprobación afirmaría otra.
    ///
    /// La regla **no se dispara** si el segundo argumento no es un entero, ni si
    /// la precisión pedida excede `ms_num::MAX_ROUND_DIGITS`. En los dos casos la
    /// expresión se queda como está y sale sin verificar, que es la respuesta
    /// honesta: nadie sabe qué número es.
    #[id = "round-to-digits"]
    #[title = "Redondeamos"]
    #[step = r"\operatorname{redondear}(x, n) = x \text{ con } n \text{ cifras decimales}"]
    fn round_to_digits(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        let Some(Head::Fn(builtin)) = arena.head_of(id) else {
            return Ok(None);
        };
        // `enBase` no se reescribe **nunca**: no cambia el número, cambia cómo se
        // escribe, y una regla que lo intentara devolvería el desarrollo
        // posicional al número perdiendo la petición. Se comprueba antes de mirar
        // los argumentos porque los suyos van al revés —la base primero— y
        // leerlos como los del redondeo redondearía a un número de cifras que
        // nadie pidió.
        //
        // La guarda se escribe por lo que **sí** entra y no por lo que no: como
        // lista de exclusiones crecía con cada función nueva del motor, y
        // olvidarse de una la habría redondeado.
        if !matches!(builtin, Builtin::Round | Builtin::RoundHalfEven) {
            return Ok(None);
        }
        let args = arena.app_args(id).map(<[ExprId]>::to_vec);
        let Some([value, digits]) = args.as_deref() else {
            return Ok(None);
        };

        let Some(value) = rational_value(arena, *value) else {
            return Ok(None);
        };
        let Some(digits) = decimal_places(arena, *digits) else {
            return Ok(None);
        };

        let rounded = match builtin {
            Builtin::Round => round_half_away(&value, digits),
            Builtin::RoundHalfEven => round_half_even(&value, digits),
            // Descartadas arriba. Este brazo existe para que añadir una función
            // nueva vuelva a romper aquí en lugar de colarse en el redondeo.
            Builtin::InBase
            | Builtin::Expand
            | Builtin::Factor
            | Builtin::Gcd
            | Builtin::Derivative => {
                return Ok(None);
            }
        };
        let Some(rounded) = rounded else {
            return Err(RuleError::MagnitudeLimit);
        };

        Ok(Some(make_fraction(
            arena,
            rounded.numer().clone(),
            rounded.denom().clone(),
        )?))
    }

    /// `p = q` → `p - q = 0`, para poder factorizar un solo lado.
    ///
    /// No se dispara sobre `x = 2`, que ya está resuelta: la devolvería a
    /// `x - 2 = 0`, «despejamos» la traería de vuelta, y el reescritor daría
    /// vueltas hasta agotar el presupuesto. Ése es el ciclo que hay que probar
    /// si alguien toca esta regla o la de despejar.
    #[id = "equation-move-to-one-side"]
    #[title = "Pasamos todo al mismo lado"]
    #[step = r"p = q \iff p - q = 0"]
    fn move_equation_to_one_side(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        Ok(ms_solve::move_to_one_side(arena, id)?)
    }

    /// `p = 0` → `factores = 0`.
    ///
    /// **Aquí factorizar sí es automático**, al contrario que
    /// `\operatorname{factorizar}(p)` suelto. La diferencia es que dentro de una
    /// ecuación no hay nada que lo deshaga: expandir es una petición con nombre
    /// y ninguna regla la hace por su cuenta, así que no hay con quién ciclar.
    /// Y es como se resuelve: factorizar es el paso, no un adorno.
    #[id = "equation-factor"]
    #[title = "Factorizamos el polinomio"]
    #[step = r"x^{2} - \left(r + s\right)x + rs = 0 \iff \left(x - r\right)\left(x - s\right) = 0"]
    fn factor_equation(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_solve::factor_left_side(arena, id)?)
    }

    /// `f \cdot g = 0` → `\left\{f = 0, g = 0\right\}`.
    #[id = "equation-zero-product"]
    #[title = "Un producto es cero si alguno de sus factores lo es"]
    #[step = r"fg = 0 \iff f = 0 \text{ o } g = 0"]
    fn split_zero_product(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_solve::zero_product(arena, id)?)
    }

    /// `ax + b = 0` → `x = -\frac{b}{a}`.
    ///
    /// Se dispara también **dentro** del conjunto de soluciones: el reescritor
    /// recorre los hijos, así que cada ecuación del conjunto se despeja por su
    /// cuenta y cada una deja su paso.
    #[id = "equation-isolate-linear"]
    #[title = "Despejamos la incógnita"]
    #[step = r"ax + b = 0 \iff x = -\frac{b}{a}"]
    fn isolate_the_unknown(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_solve::isolate_linear(arena, id)?)
    }

    /// Una ecuación cuyas soluciones no son racionales **lo dice**.
    ///
    /// No dice «no tiene solución», que sería falso: `x^{2}-2=0` tiene dos, y
    /// son `\pm\sqrt{2}`. Dice que este motor todavía no las sabe escribir, que
    /// es un límite de la etapa y no un resultado. Escribirlas exige `\sqrt` y
    /// con él la torre algebraica, que el roadmap sitúa en E5.
    ///
    /// Sin ella, `x^{2}-2=0` devolvía al estudiante su propia ecuación, sin
    /// error, sin paso y marcada «sin verificar»: la forma de fallar en la que
    /// nadie se entera de que falló. Es el mismo trato que
    /// `\operatorname{enBase}` da a una fracción.
    ///
    /// **La última de las de ecuaciones**: sólo se afirma que no hay raíces
    /// racionales cuando las cuatro anteriores ya se han rendido.
    #[id = "equation-no-rational-solutions"]
    #[title = "Sus soluciones no son racionales"]
    #[step = r"p(x) = 0 \text{ sin raíces en } \mathbb{Q}"]
    fn refuse_irrational_solutions(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        if !ms_solve::has_no_rational_solutions(arena, id) {
            return Ok(None);
        }
        Err(RuleError::IrrationalSolutions)
    }

    /// `\operatorname{expandir}(p)` quita los paréntesis de un producto de
    /// polinomios.
    ///
    /// **Es una petición, no una simplificación automática**, y por eso la
    /// cabeza es `Fn(Expand)` y no un patrón sobre `Times`. Una regla que
    /// expandiera todo producto de sumas y otra que factorizara toda suma se
    /// devolverían el trabajo hasta agotar el presupuesto; con la petición en la
    /// cabeza, la regla la consume y lo que queda ya no pide nada.
    #[id = "polynomial-expand"]
    #[title = "Expandimos el producto"]
    #[step = r"\left(a + b\right)\left(c + d\right) = ac + ad + bc + bd"]
    fn expand_polynomial(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        if arena.head_of(id) != Some(Head::Fn(Builtin::Expand)) {
            return Ok(None);
        }
        let Some(argument) = polynomial::single_argument(arena, id) else {
            return Ok(None);
        };
        polynomial::expand(arena, argument)
    }

    /// `\operatorname{factorizar}(p)` escribe un polinomio como producto.
    ///
    /// La otra mitad de la decisión de arriba. Sólo factores lineales con raíz
    /// racional: lo que no factoriza sale a la vista dentro del producto, como
    /// hace `ms-num` con una factorización entera incompleta, y si no encuentra
    /// ninguna raíz la regla **no se dispara** — devolver el mismo polinomio
    /// haría que la llamada se reescribiera a sí misma para siempre.
    #[id = "polynomial-factor"]
    #[title = "Factorizamos el polinomio"]
    #[step = r"x^{2} - \left(r + s\right)x + rs = \left(x - r\right)\left(x - s\right)"]
    fn factor_polynomial(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        if arena.head_of(id) != Some(Head::Fn(Builtin::Factor)) {
            return Ok(None);
        }
        let Some(argument) = polynomial::single_argument(arena, id) else {
            return Ok(None);
        };
        polynomial::factorize(arena, argument)
    }

    /// Un polinomio sin raíces racionales **ya está factorizado**, y se dice.
    ///
    /// Sin esta regla, `\operatorname{factorizar}(x^{2}+1)` devolvía al
    /// estudiante su propia pregunta: la llamada se quedaba sin resolver en la
    /// salida, sin error y sin explicación. Es la peor de las dos formas de
    /// fallar — la otra, contestar algo incorrecto, al menos se ve.
    ///
    /// Va **después** de `polynomial-factor`: primero se intenta factorizar de
    /// verdad, y sólo si no sale se afirma que no se puede. No cicla porque
    /// consume la petición, igual que su hermana.
    #[id = "polynomial-irreducible"]
    #[title = "No se puede factorizar más sobre los racionales"]
    #[step = r"p(x) \text{ no tiene raíces racionales}"]
    fn already_irreducible(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        if arena.head_of(id) != Some(Head::Fn(Builtin::Factor)) {
            return Ok(None);
        }
        let Some(argument) = polynomial::single_argument(arena, id) else {
            return Ok(None);
        };
        // Sólo si de verdad es un polinomio. `factorizar` de algo que este motor
        // no sabe leer no es irreducible: es una pregunta sin responder, y
        // afirmar lo primero sería inventarse un teorema.
        if !polynomial::is_polynomial(arena, argument) {
            return Ok(None);
        }
        Ok(Some(argument))
    }

    /// `\operatorname{mcd}(a, b)`, lo que E1 dejó a E2.
    ///
    /// Ya no era una decisión de kernel: con `Head::Fn(Builtin)` es una variante
    /// y esta regla. Lo que faltaba era el azúcar, y aquí está.
    #[id = "integer-gcd"]
    #[title = "Buscamos el máximo común divisor"]
    #[step = r"\operatorname{mcd}(a, b)"]
    fn compute_gcd(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        if arena.head_of(id) != Some(Head::Fn(Builtin::Gcd)) {
            return Ok(None);
        }
        let Some([left, right]) = arena.app_args(id) else {
            return Ok(None);
        };
        let (left, right) = (*left, *right);

        // Sólo enteros literales. El MCD de dos polinomios existe y es otra
        // regla; el de dos expresiones sin resolver, ninguna.
        let (Some(Node::Int(left)), Some(Node::Int(right))) =
            (arena.get(left).cloned(), arena.get(right).cloned())
        else {
            return Ok(None);
        };

        Ok(Some(arena.int(ms_alg::integer_gcd(&left, &right))?))
    }

    /// `\frac{d}{dx}\left(c\right) = 0`.
    ///
    /// **La primera de las derivadas**, porque es la que corta la recursión: lo
    /// que la cadena, la suma y el producto van dejando por debajo acaba siendo o
    /// la variable o algo que no la contiene.
    #[id = "derivative-constant"]
    #[soundness = lean_local("MathSlice.derivative_constant_sound")]
    #[title = "La derivada de una constante es cero"]
    #[step = r"\frac{d}{dx}\left(c\right) = 0"]
    fn derive_constant(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_diff::constant(arena, id)?)
    }

    /// `\frac{d}{dx}\left(x\right) = 1`.
    #[id = "derivative-variable"]
    #[soundness = lean_local("MathSlice.derivative_variable_sound")]
    #[title = "La derivada de la variable es uno"]
    #[step = r"\frac{d}{dx}\left(x\right) = 1"]
    fn derive_variable(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_diff::variable(arena, id)?)
    }

    /// `\frac{d}{dx}\left(x^{n}\right) = nx^{n-1}`.
    ///
    /// **Antes que la cadena, y no al revés.** Una potencia de la variable pelada
    /// es un caso particular de la cadena, y por la cadena saldría
    /// `2x\frac{d}{dx}\left(x\right)` con un factor `1` un paso después: el
    /// resultado es el mismo y el paso que se lee es peor. El estudiante espera
    /// «derivamos una potencia de x», no la cadena sobre la identidad.
    ///
    /// **Demostrada a medias, y por eso no se declara demostrada.**
    /// `MathSlice.derivative_power_sound` existe y cubre el exponente natural,
    /// que es donde vive `Polynomial ℚ`. Esta regla acepta además exponentes
    /// **negativos** —`x^{-2}` sale `-2x^{-3}`, y hay caso de corpus— y ésos se
    /// salen del anillo de polinomios: son fracciones. Declarar el teorema aquí
    /// contaría como respaldada una regla que hace más de lo que se demostró, que
    /// es el error simétrico al de `rational-reciprocal` en E2.
    #[id = "derivative-power"]
    #[title = "Derivamos una potencia de x"]
    #[step = r"\frac{d}{dx}\left(x^{n}\right) = nx^{n-1}"]
    fn derive_power(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_diff::power_of_variable(arena, id)?)
    }

    /// `\frac{d}{dx}\left(u^{n}\right) = nu^{n-1}\frac{d}{dx}\left(u\right)`.
    ///
    /// La regla de la cadena, que es lo que hace que `\left(x^{2}+1\right)^{3}`
    /// se derive **sin expandirlo**. La derivada de dentro queda pedida en el
    /// resultado y la resuelven las reglas de abajo en las iteraciones
    /// siguientes.
    ///
    /// Sin declarar demostrada por lo mismo que `derivative-power`:
    /// `MathSlice.derivative_chain_sound` cubre el exponente natural y esta regla
    /// acepta también el negativo.
    #[id = "derivative-chain"]
    #[title = "Derivamos una potencia de una función (regla de la cadena)"]
    #[step = r"\frac{d}{dx}\left(u^{n}\right) = nu^{n-1}\frac{d}{dx}\left(u\right)"]
    fn derive_chain(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_diff::chain(arena, id)?)
    }

    /// `\frac{d}{dx}\left(cf\right) = c\frac{d}{dx}\left(f\right)`.
    ///
    /// Antes que el producto, y disjunta de él por construcción: ésta exige al
    /// menos un factor sin la variable y aquélla que no haya ninguno. Sin esta
    /// regla, `\frac{d}{dx}\left(3x^{2}\right)` se anunciaría como «derivada de
    /// un producto», que es correcto y no es lo que se enseña.
    #[id = "derivative-constant-multiple"]
    #[soundness = lean_local("MathSlice.derivative_constant_multiple_sound")]
    #[title = "La constante sale fuera de la derivada"]
    #[step = r"\frac{d}{dx}\left(cf\right) = c\frac{d}{dx}\left(f\right)"]
    fn derive_constant_multiple(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        Ok(ms_diff::constant_multiple(arena, id)?)
    }

    /// `\frac{d}{dx}\left(f + g\right)` reparte sobre los sumandos.
    #[id = "derivative-sum"]
    #[soundness = lean_local("MathSlice.derivative_sum_sound")]
    #[title = "La derivada de una suma es la suma de las derivadas"]
    #[step = r"\frac{d}{dx}\left(f + g\right) = \frac{d}{dx}\left(f\right) + \frac{d}{dx}\left(g\right)"]
    fn derive_sum(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_diff::sum(arena, id)?)
    }

    /// `\frac{d}{dx}\left(fg\right) = \frac{d}{dx}\left(f\right)g + f\frac{d}{dx}\left(g\right)`.
    #[id = "derivative-product"]
    #[soundness = lean_local("MathSlice.derivative_product_sound")]
    #[title = "Derivamos un producto"]
    #[step = r"\frac{d}{dx}\left(fg\right) = \frac{d}{dx}\left(f\right)g + f\frac{d}{dx}\left(g\right)"]
    fn derive_product(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        Ok(ms_diff::product(arena, id)?)
    }

    /// Una derivada que ninguna regla supo hacer **lo dice**.
    ///
    /// **La última de las de derivadas**: sólo se afirma que no se sabe derivar
    /// esto cuando las siete anteriores ya se han rendido. Sin ella,
    /// `\frac{d}{dx}\left(x^{\frac{1}{2}}\right)` devolvería al estudiante su
    /// propia pregunta —sin error, sin paso y marcada «sin verificar»—, que es la
    /// forma de fallar en la que nadie se entera de que falló.
    ///
    /// Es el mismo trato que `polynomial-irreducible` y
    /// `equation-no-rational-solutions`: un error que explica el límite de la
    /// etapa en vez de una respuesta a otra pregunta.
    #[id = "derivative-unsupported"]
    #[title = "Esta derivada todavía no se sabe hacer"]
    #[step = r"\frac{d}{dx}\left(f\right) \text{ fuera del alcance de esta etapa}"]
    fn refuse_unknown_derivative(
        arena: &mut Arena,
        id: ExprId,
        _assumptions: &Assumptions,
    ) -> RuleResult {
        if !ms_diff::is_unsupported(arena, id) {
            return Ok(None);
        }
        Err(RuleError::UnsupportedDerivative)
    }

    /// `x + x + 1` → `2x + 1`.
    ///
    /// **No es limpieza, es un paso**, y por eso no es silenciosa: «agrupamos los
    /// términos semejantes» es lo que se dice en clase.
    ///
    /// Hasta E2 no hizo falta porque las sumas simbólicas salían de `ms-alg`, que
    /// devuelve un término por grado. La regla del producto de una derivada no
    /// pasa por ahí: `\frac{d}{dx}\left(x\left(x+1\right)\right)` da
    /// `x + \left(x + 1\right)`, que es correcto y se lee `x + x + 1`.
    ///
    /// **Va después de las de fracciones** y sólo agrupa lo que tiene parte
    /// simbólica: `\frac{1}{2} + \frac{1}{3}` es cosa del denominador común, y
    /// `2 + 3`, del plegado silencioso.
    #[id = "like-terms"]
    #[title = "Agrupamos los términos semejantes"]
    #[step = r"ax + bx = \left(a + b\right)x"]
    fn collect_like_terms(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        if arena.head_of(id) != Some(Head::Plus) {
            return Ok(None);
        }
        polynomial::collect_like_terms(arena, id)
    }

    /// Plegar `2 + 3` en `5` no es un paso pedagógico, es aritmética que el
    /// lector ya sabe hacer. Por eso es silenciosa: aparecería en la traza como
    /// ruido entre los pasos que sí explican algo.
    #[id = "integer-fold-sum"]
    #[title = "Sumamos los enteros"]
    #[step = r"a + b = c"]
    #[silent]
    fn fold_integer_sum(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        fold_integers(arena, id, Head::Plus)
    }

    #[id = "integer-fold-product"]
    #[title = "Multiplicamos los enteros"]
    #[step = r"a \cdot b = c"]
    #[silent]
    fn fold_integer_product(arena: &mut Arena, id: ExprId, _assumptions: &Assumptions) -> RuleResult {
        fold_integers(arena, id, Head::Times)
    }
}

/// El argumento de precisión de un redondeo: un número **entero** de cifras.
///
/// `2.5` cifras no significa nada y «unas cuantas» no es una operación, así que
/// ante cualquier otra cosa la regla no se dispara. Es la misma lectura que hace
/// `ms-verify` por su lado, y tiene que serlo: si una acepta lo que la otra
/// rechaza, el resultado se calcula y no se puede comprobar.
fn decimal_places(arena: &Arena, id: ExprId) -> Option<i32> {
    let value = rational_value(arena, id)?;
    if !value.is_integer() {
        return None;
    }
    value.to_integer().to_i32()
}

/// Pliega los argumentos enteros de una suma o un producto en uno solo.
///
/// Los demás argumentos se conservan: `2 + x + 3` se convierte en `5 + x`, no en
/// un fallo por no ser todo numérico.
fn fold_integers(arena: &mut Arena, id: ExprId, head: Head) -> RuleResult {
    if arena.head_of(id) != Some(head) {
        return Ok(None);
    }
    let args = arena.app_args(id).map(<[ExprId]>::to_vec);
    let Some(args) = args else {
        return Ok(None);
    };

    let mut folded = match head {
        Head::Plus => Integer::zero(),
        Head::Times => Integer::one(),
        // Plegar no es una operación de estas cabezas: los argumentos de una
        // potencia, de una llamada, de una ecuación y de un conjunto de
        // soluciones no se acumulan, cada uno significa lo suyo.
        Head::Power | Head::Fn(_) | Head::Equal | Head::Solutions => return Ok(None),
    };
    let mut integers = 0_usize;
    let mut rest = Vec::with_capacity(args.len());

    for arg in args {
        match arena.get(arg) {
            Some(ms_core::Node::Int(value)) => {
                let value = value.clone();
                integers += 1;
                match head {
                    Head::Plus => folded += value,
                    Head::Times => folded *= value,
                    Head::Power | Head::Fn(_) | Head::Equal | Head::Solutions => {
                        return Ok(None);
                    }
                }
            }
            Some(ms_core::Node::Rat(_) | ms_core::Node::Sym(_) | ms_core::Node::App { .. })
            | None => rest.push(arg),
        }
    }

    // Con un solo entero no hay nada que plegar, y plegarlo igualmente haría que
    // la regla se dispare para siempre sobre su propio resultado.
    if integers < 2 {
        return Ok(None);
    }

    let folded = arena.int(folded)?;
    rest.push(folded);
    Ok(Some(arena.app(head, &rest)?))
}
