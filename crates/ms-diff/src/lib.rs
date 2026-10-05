//! `ms-diff` — qué se le hace a una derivada.
//!
//! Una función por regla de derivación, cada una **un paso que el estudiante
//! reconoce**: la constante, la variable, la potencia, la cadena, la constante
//! que sale fuera, la suma y el producto. Ninguna decide cuándo aplicarse —eso es
//! del reescritor— y ninguna escribe LaTeX: aquí sólo se transforma el árbol.
//!
//! # Por qué esto no pasa por `ms-alg`
//!
//! Derivar es **estructural**: se mira la cabeza y se recurre. Pasar por el
//! polinomio obligaría a que todo lo derivable fuera un polinomio, y
//! `\frac{1}{x}` no lo es. Con `Poly` sólo se podría derivar lo que ya está
//! expandido, que es justo lo contrario de lo que hace falta: la gracia de la
//! regla de la cadena es no expandir `\left(x^{2}+1\right)^{3}`.
//!
//! # Terminación
//!
//! Toda regla consume la cabeza `derivar` que recibe. Las que reintroducen una
//! —la cadena, la suma, el producto, la constante fuera— lo hacen **sobre
//! operandos estrictamente más pequeños** que el que consumieron, y una expresión
//! es un árbol finito. Así que la recursión acaba, y no depende del presupuesto
//! de iteraciones del reescritor: ése es la red, no el suelo.
//!
//! # Alcance de E3, y lo que queda fuera con su motivo
//!
//! **Exponentes enteros.** `x^{\frac{1}{2}}` es `\sqrt{x}`, y escribir su
//! derivada exige la torre algebraica que el roadmap sitúa en E5. Se derivan
//! sumas, productos, potencias de exponente entero y sus composiciones — que es
//! todo lo que este motor sabe escribir hoy.
//!
//! **Los demás símbolos son constantes.** En `\frac{d}{dx}\left(y\right)` la `y`
//! no depende de `x`, así que la derivada es cero. La otra lectura —`y` es una
//! función de `x` y aparece un `\frac{dy}{dx}`— es la diferenciación implícita, y
//! exige poder escribir `y(x)`, que es una decisión de kernel pendiente.

use ms_core::{Arena, Builtin, CoreError, ExprId, Head, Node};
use ms_num::Integer;
use num_traits::{One, Zero};

/// El resultado de una regla: la expresión nueva, o `None` si esta regla no es
/// la que toca. Es la misma forma que `ms-solve`, y por la misma razón: decidir
/// es del reescritor.
type Rewrite = Result<Option<ExprId>, CoreError>;

/// `\frac{d}{dx}\left(c\right) = 0` cuando `c` no contiene la variable.
///
/// **La primera**, porque es la que corta la recursión: todo lo que la cadena, la
/// suma y el producto van dejando por debajo acaba siendo o la variable o algo que
/// no la contiene.
pub fn constant(arena: &mut Arena, id: ExprId) -> Rewrite {
    let Some((operand, variable)) = request(arena, id) else {
        return Ok(None);
    };
    if contains(arena, operand, variable) {
        return Ok(None);
    }
    arena.int(0).map(Some)
}

/// `\frac{d}{dx}\left(x\right) = 1`.
pub fn variable(arena: &mut Arena, id: ExprId) -> Rewrite {
    let Some((operand, variable)) = request(arena, id) else {
        return Ok(None);
    };
    if operand != variable {
        return Ok(None);
    }
    arena.int(1).map(Some)
}

/// `\frac{d}{dx}\left(x^{n}\right) = nx^{n-1}`.
///
/// **Es un caso particular de [`chain`] y aun así tiene regla propia**, y ésa es
/// la decisión: con la cadena, `x^{2}` daría
/// `2x\frac{d}{dx}\left(x\right)` y un paso después el factor `1`. El resultado
/// es el mismo y el paso que se lee es peor — el estudiante espera «derivamos una
/// potencia de x», no «regla de la cadena» sobre la identidad.
///
/// Por eso va **antes** que la cadena en el orden de intento.
pub fn power_of_variable(arena: &mut Arena, id: ExprId) -> Rewrite {
    let Some((operand, variable)) = request(arena, id) else {
        return Ok(None);
    };
    let Some((base, exponent)) = integer_power(arena, operand) else {
        return Ok(None);
    };
    if base != variable {
        return Ok(None);
    }
    lower_the_exponent(arena, base, &exponent).map(Some)
}

/// `\frac{d}{dx}\left(u^{n}\right) = nu^{n-1}\frac{d}{dx}\left(u\right)`.
///
/// La cadena, con la base ya derivada dentro. Sólo se dispara cuando `u`
/// **contiene** la variable: si no la contiene es una constante, y de eso se
/// encarga [`constant`] sin repartir nada.
pub fn chain(arena: &mut Arena, id: ExprId) -> Rewrite {
    let Some((operand, variable)) = request(arena, id) else {
        return Ok(None);
    };
    let Some((base, exponent)) = integer_power(arena, operand) else {
        return Ok(None);
    };
    // La potencia de la variable pelada ya la atendió `power_of_variable`, y una
    // base sin la variable dentro es constante entera.
    if base == variable || !contains(arena, base, variable) {
        return Ok(None);
    }

    let lowered = lower_the_exponent(arena, base, &exponent)?;
    let inner = derivative_of(arena, base, variable)?;
    arena.times(&[lowered, inner]).map(Some)
}

/// `\frac{d}{dx}\left(cf\right) = c\frac{d}{dx}\left(f\right)`.
///
/// La constante multiplicativa sale fuera. **No es un adorno del producto**: sin
/// ella, `\frac{d}{dx}\left(3x^{2}\right)` se anuncia como «derivada de un
/// producto» y produce un `\frac{d}{dx}\left(3\right)x^{2}` que después vale
/// cero. Es correcto y no es lo que se enseña.
///
/// Va **antes** que [`product`], y las dos son disjuntas por construcción: ésta
/// exige al menos un factor sin la variable, aquélla exige que no haya ninguno.
pub fn constant_multiple(arena: &mut Arena, id: ExprId) -> Rewrite {
    let Some((operand, variable)) = request(arena, id) else {
        return Ok(None);
    };
    let Some(factors) = factors_of(arena, operand) else {
        return Ok(None);
    };

    let (constants, rest): (Vec<ExprId>, Vec<ExprId>) = factors
        .into_iter()
        .partition(|factor| !contains(arena, *factor, variable));
    if constants.is_empty() || rest.is_empty() {
        return Ok(None);
    }

    let inner = arena.times(&rest)?;
    let derivative = derivative_of(arena, inner, variable)?;

    let mut product = constants;
    product.push(derivative);
    arena.times(&product).map(Some)
}

/// `\frac{d}{dx}\left(f + g\right) = \frac{d}{dx}\left(f\right) +
/// \frac{d}{dx}\left(g\right)`.
pub fn sum(arena: &mut Arena, id: ExprId) -> Rewrite {
    let Some((operand, variable)) = request(arena, id) else {
        return Ok(None);
    };
    if arena.head_of(operand) != Some(Head::Plus) {
        return Ok(None);
    }
    let Some(summands) = arena.app_args(operand).map(<[ExprId]>::to_vec) else {
        return Ok(None);
    };

    let mut derivatives = Vec::with_capacity(summands.len());
    for summand in summands {
        derivatives.push(derivative_of(arena, summand, variable)?);
    }
    arena.plus(&derivatives).map(Some)
}

/// `\frac{d}{dx}\left(fg\right) = \frac{d}{dx}\left(f\right)g +
/// f\frac{d}{dx}\left(g\right)`, y con más factores, un sumando por factor.
///
/// Sólo cuando **todos** los factores contienen la variable: si alguno no la
/// contiene es una constante y sale fuera con [`constant_multiple`], que es el
/// paso que se enseña.
pub fn product(arena: &mut Arena, id: ExprId) -> Rewrite {
    let Some((operand, variable)) = request(arena, id) else {
        return Ok(None);
    };
    let Some(factors) = factors_of(arena, operand) else {
        return Ok(None);
    };
    if factors.iter().any(|f| !contains(arena, *f, variable)) {
        return Ok(None);
    }

    // Un sumando por factor: el factor derivado y los demás tal cual.
    let mut summands = Vec::with_capacity(factors.len());
    for (position, factor) in factors.iter().enumerate() {
        let derivative = derivative_of(arena, *factor, variable)?;
        let mut term = factors.clone();
        let Some(slot) = term.get_mut(position) else {
            return Ok(None);
        };
        *slot = derivative;
        summands.push(arena.times(&term)?);
    }
    arena.plus(&summands).map(Some)
}

/// ¿Hay aquí una derivada que este crate no sabe hacer?
///
/// Se pregunta **después** de que todas las reglas se hayan rendido, y es la
/// diferencia entre un motor que no llega y uno que se calla: sin esto,
/// `\frac{d}{dx}\left(x^{\frac{1}{2}}\right)` devolvería al estudiante su propia
/// pregunta, sin error, sin paso y marcada «sin verificar». Es el mismo trato que
/// `polynomial-irreducible` le da a lo que no se puede factorizar.
///
/// **Una derivada cuyo operando contiene otra derivada todavía no está
/// atascada**, y ésa es la corrección que costó un test: el reescritor prueba las
/// reglas **de fuera adentro**, así que en
/// `\frac{d}{dx}\left(\frac{d}{dx}\left(x^{3}\right)\right)` la de fuera se mira
/// primero y ninguna regla la reconoce —su operando es una derivada, no una suma
/// ni una potencia—. Rendirse ahí sería declarar imposible una segunda derivada
/// que sale sola en cuanto la de dentro se resuelve.
pub fn is_unsupported(arena: &Arena, id: ExprId) -> bool {
    let Some((operand, _)) = request(arena, id) else {
        return false;
    };
    !contains_derivative(arena, operand)
}

/// ¿Queda alguna derivada sin resolver dentro de `id`?
fn contains_derivative(arena: &Arena, id: ExprId) -> bool {
    if arena.head_of(id) == Some(Head::Fn(Builtin::Derivative)) {
        return true;
    }
    match arena.get(id) {
        Some(Node::App { args, .. }) => arena.args_of(*args).is_some_and(|args| {
            args.to_vec()
                .into_iter()
                .any(|arg| contains_derivative(arena, arg))
        }),
        Some(Node::Int(_) | Node::Rat(_) | Node::Sym(_)) | None => false,
    }
}

/// La petición: `derivar(f, x)` con `x` un símbolo.
///
/// La variable se devuelve como `ExprId` y no como `SymId` porque la arena
/// comparte estructura: dos símbolos iguales son el mismo id, así que comparar
/// ids **es** comparar símbolos, y no hace falta desenvolver nada.
fn request(arena: &Arena, id: ExprId) -> Option<(ExprId, ExprId)> {
    if arena.head_of(id) != Some(Head::Fn(Builtin::Derivative)) {
        return None;
    }
    let [operand, variable] = arena.app_args(id)? else {
        return None;
    };
    // La variable tiene que ser un símbolo. `derivar(f, 2)` no significa nada, y
    // el parser no lo produce; llegar aquí desde otro sitio y derivar «respecto
    // de 2» sería inventarse una respuesta.
    matches!(arena.get(*variable)?, Node::Sym(_)).then_some((*operand, *variable))
}

/// `derivar(f, x)` recién construida. Es lo que reintroducen las reglas que
/// reparten, y siempre sobre un operando más pequeño que el que recibieron.
fn derivative_of(
    arena: &mut Arena,
    operand: ExprId,
    variable: ExprId,
) -> Result<ExprId, CoreError> {
    arena.app(Head::Fn(Builtin::Derivative), &[operand, variable])
}

/// Los factores de un producto, o `None` si esto no es un producto.
fn factors_of(arena: &Arena, id: ExprId) -> Option<Vec<ExprId>> {
    if arena.head_of(id) != Some(Head::Times) {
        return None;
    }
    arena.app_args(id).map(<[ExprId]>::to_vec)
}

/// ¿Aparece `symbol` dentro de `id`?
///
/// Comparación por id, que es exacta: la arena comparte estructura y no hay dos
/// nodos iguales con ids distintos.
fn contains(arena: &Arena, id: ExprId, symbol: ExprId) -> bool {
    if id == symbol {
        return true;
    }
    match arena.get(id) {
        Some(Node::App { args, .. }) => arena.args_of(*args).is_some_and(|args| {
            args.to_vec()
                .into_iter()
                .any(|arg| contains(arena, arg, symbol))
        }),
        Some(Node::Int(_) | Node::Rat(_) | Node::Sym(_)) | None => false,
    }
}

/// `u^{n}` con `n` entero, que es lo único que este crate sabe derivar.
///
/// Un exponente racional es una raíz —`x^{\frac{1}{2}}` es `\sqrt{x}`— y uno
/// simbólico exige el logaritmo. Los dos son E5, así que aquí devuelven `None` y
/// la respuesta acaba siendo un error que lo dice.
fn integer_power(arena: &Arena, id: ExprId) -> Option<(ExprId, Integer)> {
    if arena.head_of(id) != Some(Head::Power) {
        return None;
    }
    let [base, exponent] = arena.app_args(id)? else {
        return None;
    };
    let Node::Int(exponent) = arena.get(*exponent)? else {
        return None;
    };
    Some((*base, exponent.clone()))
}

/// `nu^{n-1}`, el cuerpo compartido de la regla de la potencia y de la cadena.
///
/// Una sola definición: dos copias de esta aritmética se desincronizarían en el
/// exponente, y un `n-1` mal escrito da una derivada perfectamente plausible.
fn lower_the_exponent(
    arena: &mut Arena,
    base: ExprId,
    exponent: &Integer,
) -> Result<ExprId, CoreError> {
    let coefficient = arena.int(exponent.clone())?;
    let lowered = exponent - Integer::one();

    // `u^{1}` se escribe `u`, y dejar la potencia obligaría al reescritor a
    // quitarla después con un paso que no explica nada.
    if lowered.is_one() {
        return arena.times(&[coefficient, base]);
    }
    // `u^{0}` es 1, así que lo que queda es el coeficiente solo.
    if lowered.is_zero() {
        return Ok(coefficient);
    }

    let lowered = arena.int(lowered)?;
    let power = arena.power(base, lowered)?;
    arena.times(&[coefficient, power])
}
