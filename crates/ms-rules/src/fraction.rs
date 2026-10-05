//! Reconocer y construir fracciones sobre el IR.
//!
//! El IR no tiene cabeza `Divide`: una fracción es `Times(n, Power(d, -1))`, y
//! un entero es una fracción de denominador 1. Estas dos funciones son el único
//! sitio donde eso se sabe, para que las reglas hablen de numeradores y
//! denominadores en vez de desmontar árboles.

use ms_core::{Arena, ExprId, Head, Node};
use ms_num::{Integer, Rational};
use num_traits::{One, Signed, Zero};

use crate::rule::RuleError;

/// El numerador y el denominador de `id`, con el **denominador siempre
/// positivo** y no nulo.
///
/// Devuelve `None` si la expresión no es un racional literal — incluido el caso
/// de denominador cero, que no es una fracción sino un error que le toca
/// reportar a la capa de arriba.
pub fn as_fraction(arena: &Arena, id: ExprId) -> Option<(Integer, Integer)> {
    let (numerator, denominator) = match arena.get(id)? {
        Node::Int(value) => (value.clone(), Integer::one()),
        Node::Rat(value) => (value.numer().clone(), value.denom().clone()),
        Node::Sym(_) => return None,
        Node::App { head, .. } => match head {
            Head::Power => (Integer::one(), reciprocal_denominator(arena, id)?),
            Head::Times => times_as_fraction(arena, id)?,
            // Una suma sin plegar y una llamada sin resolver no son racionales
            // literales todavía. Que lo serán después no las convierte en uno
            // ahora: leerles un numerador exigiría evaluarlas.
            //
            // Una ecuación y un conjunto de soluciones no lo serán nunca: no son
            // números de ninguna forma, y la regla que los tratara como una
            // fracción operaría sobre algo que no lo es.
            Head::Plus | Head::Fn(_) | Head::Equal | Head::Solutions => return None,
        },
    };

    if denominator.is_zero() {
        return None;
    }
    // El signo vive en el numerador: así dos fracciones iguales se comparan como
    // iguales, y `-\frac{1}{2}` no puede escribirse también `\frac{1}{-2}`.
    if denominator.is_negative() {
        return Some((-numerator, -denominator));
    }
    Some((numerator, denominator))
}

/// Si `id` es `Power(d, -1)` con `d` entero, devuelve `d`.
fn reciprocal_denominator(arena: &Arena, id: ExprId) -> Option<Integer> {
    let [base, exponent] = arena.app_args(id)? else {
        return None;
    };
    let Node::Int(exponent) = arena.get(*exponent)? else {
        return None;
    };
    if *exponent != -Integer::one() {
        return None;
    }
    let Node::Int(base) = arena.get(*base)? else {
        return None;
    };
    Some(base.clone())
}

/// Un `Times` es una fracción si sus factores son enteros y recíprocos de enteros.
fn times_as_fraction(arena: &Arena, id: ExprId) -> Option<(Integer, Integer)> {
    let mut numerator = Integer::one();
    let mut denominator = Integer::one();

    for factor in arena.app_args(id)? {
        match arena.get(*factor)? {
            Node::Int(value) => numerator *= value,
            Node::App { head: Head::Power, .. } => {
                denominator *= reciprocal_denominator(arena, *factor)?;
            }
            Node::Rat(_) | Node::Sym(_) | Node::App { .. } => return None,
        }
    }

    Some((numerator, denominator))
}

/// El valor exacto de `id`, si es un racional literal.
pub fn rational_value(arena: &Arena, id: ExprId) -> Option<Rational> {
    let (numerator, denominator) = as_fraction(arena, id)?;
    Some(Rational::new(numerator, denominator))
}

/// Construye `n/d` en el IR, degradando a entero cuando el denominador es 1.
pub fn make_fraction(
    arena: &mut Arena,
    numerator: Integer,
    denominator: Integer,
) -> Result<ExprId, RuleError> {
    if denominator.is_one() {
        return Ok(arena.int(numerator)?);
    }
    let numerator = arena.int(numerator)?;
    let denominator = arena.int(denominator)?;
    let minus_one = arena.int(-1)?;
    let reciprocal = arena.power(denominator, minus_one)?;
    Ok(arena.times(&[numerator, reciprocal])?)
}

/// ¿Es `id` una potencia de base cero y exponente negativo, es decir un `1/0`?
fn is_zero_to_a_negative_power(arena: &Arena, id: ExprId) -> bool {
    if arena.head_of(id) != Some(Head::Power) {
        return false;
    }
    let Some([base, exponent]) = arena.app_args(id) else {
        return false;
    };
    // La base puede ser cero **estructuralmente** y no como literal: en
    // `\left(\frac{0}{9}\right)^{-1}` la base es un `Times`, y mirar sólo
    // `Node::Int` dejaba pasar ese recíproco como si estuviera definido.
    let Some((numerator, _)) = as_fraction(arena, *base) else {
        return false;
    };
    if !numerator.is_zero() {
        return false;
    }
    // El exponente puede ser entero o racional: `0^{-1}` y `0^{-1/2}` están
    // igual de indefinidos. Con exponente simbólico no se sabe, y no saberlo no
    // autoriza a afirmar nada.
    match arena.get(*exponent) {
        Some(Node::Int(value)) => value.is_negative(),
        Some(Node::Rat(value)) => value.is_negative(),
        Some(Node::Sym(_) | Node::App { .. }) | None => false,
    }
}

/// ¿Hay una división por cero en algún punto de la expresión?
///
/// Es una comprobación aparte y no una regla: no hay nada que reescribir, hay
/// algo que reportar. La respuesta correcta a `\frac{2}{0}` es un error
/// explicado, no un resultado (§6).
///
/// **No basta con mirar la entrada.** El denominador puede volverse cero al
/// reescribir: `\frac{2}{1-1}` entra con un `Plus` en el denominador y sólo
/// después del plegado es un `0`. Por eso quien llama a esto es el reescritor, en
/// cada paso, y no la frontera de WASM una sola vez.
///
/// Se recorre el árbol entero cada vez. Memoizar por `ExprId` —que sería correcto,
/// porque la arena es append-only y «este subárbol está limpio» es un hecho
/// permanente del id— **se probó y salió peor**: montar el conjunto cuesta más que
/// recorrer seis nodos. Cuando las expresiones sean grandes volverá a merecer la
/// pena, y entonces el bench lo dirá.
pub(crate) fn has_division_by_zero(arena: &Arena, id: ExprId) -> bool {
    if is_zero_to_a_negative_power(arena, id) {
        return true;
    }
    let Some(node) = arena.get(id) else {
        return false;
    };

    match node {
        Node::Int(_) | Node::Rat(_) | Node::Sym(_) => false,
        Node::App { args, .. } => {
            let Some(args) = arena.args_of(*args) else {
                return false;
            };
            args.iter().any(|arg| has_division_by_zero(arena, *arg))
        }
    }
}
