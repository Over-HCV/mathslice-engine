//! `ms-solve` — qué se le hace a una ecuación.
//!
//! Cuatro operaciones, cada una un paso que el estudiante reconoce: pasar todo
//! al mismo lado, factorizar, anular el producto y despejar. Ninguna decide
//! cuándo aplicarse —eso es del reescritor— y ninguna escribe LaTeX: aquí sólo
//! se transforma el árbol.
//!
//! # Alcance de E2, y lo que queda fuera con su motivo
//!
//! Se resuelve **factorizando sobre ℚ**. Una ecuación cuyas raíces son
//! irracionales —`x^{2}-2=0`— sale sin resolver y **lo dice**, en vez de
//! inventarse una respuesta o dar un decimal como si fuera exacto. No es una
//! limitación de este crate: la fórmula cuadrática necesita `\sqrt`, y con él la
//! torre algebraica que [`04-roadmap.md`](../../../architecture/04-roadmap.md)
//! sitúa en E5. Contestar `1.4142` a `x^{2}=2` sería exactamente lo que este
//! motor existe para no hacer.
//!
//! El resto de *Equation Solving* del árbol de capacidades —sistemas,
//! congruencias, Newton, secante, bisección— es de este crate y no está hecho.

use ms_alg::{Poly, as_polynomial, as_univariate, factor, factored_to_exprs, to_expr};
use ms_core::{Arena, CoreError, ExprId, Head, Node};
use ms_num::Rational;
use num_traits::Zero;

/// Los dos lados de una ecuación, si `id` lo es.
fn sides(arena: &Arena, id: ExprId) -> Option<(ExprId, ExprId)> {
    if arena.head_of(id) != Some(Head::Equal) {
        return None;
    }
    let [left, right] = arena.app_args(id)? else {
        return None;
    };
    Some((*left, *right))
}

/// ¿Vale cero este literal?
fn is_zero(arena: &Arena, id: ExprId) -> bool {
    match arena.get(id) {
        Some(Node::Int(value)) => value.is_zero(),
        Some(Node::Rat(value)) => value.is_zero(),
        Some(Node::Sym(_) | Node::App { .. }) | None => false,
    }
}

/// ¿Es `x = r`, con la incógnita ya sola? Entonces no hay nada que hacer.
///
/// Sin esta pregunta, «pasar todo a un lado» convertiría `x = 2` en `x - 2 = 0`,
/// «despejar» lo devolvería a `x = 2`, y el reescritor daría vueltas hasta
/// agotar el presupuesto. Es el ciclo que hay que probar si alguien cambia
/// alguna de las dos reglas.
pub fn is_solved(arena: &Arena, id: ExprId) -> bool {
    let Some((left, right)) = sides(arena, id) else {
        return false;
    };
    matches!(arena.get(left), Some(Node::Sym(_)))
        && matches!(arena.get(right), Some(Node::Int(_) | Node::Rat(_)))
}

/// `p = q` → `p - q = 0`.
///
/// Y `0 = p` → `p = 0`, que es un intercambio y no una resta: la incógnita se
/// escribe a la izquierda.
pub fn move_to_one_side(arena: &mut Arena, id: ExprId) -> Result<Option<ExprId>, CoreError> {
    let Some((left, right)) = sides(arena, id) else {
        return Ok(None);
    };
    if is_solved(arena, id) {
        return Ok(None);
    }
    if is_zero(arena, right) {
        return Ok(None);
    }

    let zero = arena.int(0)?;

    if is_zero(arena, left) {
        return Ok(Some(arena.app(Head::Equal, &[right, zero])?));
    }

    let negated = negate(arena, right)?;
    let difference = arena.plus(&[left, negated])?;
    Ok(Some(arena.app(Head::Equal, &[difference, zero])?))
}

/// `-x`, con el signo **dentro** del número si lo hay.
///
/// Un `Times(-1, 4)` en medio de un paso hace que se lea `x^{2} - 1 \cdot 4 = 0`
/// en vez de `x^{2} - 4 = 0`. El plegado silencioso lo arreglaría una iteración
/// después, pero el paso ya se habría escrito así, y el paso es lo que el
/// estudiante lee.
fn negate(arena: &mut Arena, id: ExprId) -> Result<ExprId, CoreError> {
    match arena.get(id) {
        Some(Node::Int(value)) => {
            let negated = -value.clone();
            arena.int(negated)
        }
        Some(Node::Rat(value)) => {
            let negated = -value.clone();
            arena.rat(negated)
        }
        Some(Node::Sym(_) | Node::App { .. }) | None => {
            let minus_one = arena.int(-1)?;
            arena.times(&[minus_one, id])
        }
    }
}

/// `p = 0` → `factores = 0`, si `p` factoriza sobre ℚ.
pub fn factor_left_side(arena: &mut Arena, id: ExprId) -> Result<Option<ExprId>, CoreError> {
    let Some((left, right)) = sides(arena, id) else {
        return Ok(None);
    };
    if !is_zero(arena, right) {
        return Ok(None);
    }
    let Some(univariate) = as_univariate(arena, left) else {
        return Ok(None);
    };
    let Some(factored) = factor(&univariate.poly) else {
        return Ok(None);
    };
    if factored.linear.is_empty() {
        return Ok(None);
    }

    let factors = factored_to_exprs(arena, univariate.variable, &factored)?;
    let product = match factors.as_slice() {
        [single] => *single,
        many => arena.times(many)?,
    };

    // La arena comparte estructura, así que un id igual significa que ya estaba
    // factorizada. **La comparación va sobre la forma canónica de los dos**: el
    // producto recién construido lleva los factores en el orden en que salieron
    // de la factorización y el lado izquierdo los tiene ya ordenados, así que
    // comparar los ids en crudo dice «cambió» cuando no ha cambiado nada.
    //
    // Cuando eso pasaba, la regla se disparaba una segunda vez, el reescritor
    // canonicalizaba el resultado, veía que era idéntico al estado anterior y
    // **paraba ahí** — dejando la ecuación factorizada y sin resolver, sin error
    // y sin paso que lo explicara.
    let canonical = arena.canonicalize(product)?.id();
    if canonical == left {
        return Ok(None);
    }

    let zero = arena.int(0)?;
    Ok(Some(arena.app(Head::Equal, &[product, zero])?))
}

/// `f · g = 0` → `{f = 0, g = 0}`.
///
/// Los factores **constantes se descartan**, y no por limpieza: `2(x-1) = 0`
/// daría la ecuación `2 = 0`, que es falsa y no es una solución de nada. Un
/// producto es cero cuando lo es alguno de sus factores, y una constante no nula
/// nunca lo es.
pub fn zero_product(arena: &mut Arena, id: ExprId) -> Result<Option<ExprId>, CoreError> {
    let Some((left, right)) = sides(arena, id) else {
        return Ok(None);
    };
    if !is_zero(arena, right) || arena.head_of(left) != Some(Head::Times) {
        return Ok(None);
    }
    let Some(factors) = arena.app_args(left).map(<[ExprId]>::to_vec) else {
        return Ok(None);
    };

    let zero = arena.int(0)?;
    let mut equations = Vec::with_capacity(factors.len());
    for factor in factors {
        // Un factor constante no aporta ecuación. Que sea **no nulo** lo
        // garantiza la factorización, que nunca emite un factor cero: si lo
        // hiciera, la ecuación entera sería `0 = 0` y no tendría incógnita.
        if matches!(arena.get(factor), Some(Node::Int(_) | Node::Rat(_))) {
            continue;
        }
        let equation = arena.app(Head::Equal, &[factor, zero])?;
        // Un factor repetido —una raíz doble— no aporta una solución más. El
        // conjunto de soluciones de `\left(x-2\right)^{2}=0` es `\{2\}`, y
        // escribirlo `\{x = 2, x = 2\}` se lee como dos soluciones que resultan
        // valer lo mismo, que es otra cosa.
        //
        // La multiplicidad se pierde aquí a sabiendas: es información sobre la
        // ecuación, no una solución, y su sitio es un hecho — como la paridad de
        // un entero— cuando E3 traiga los hechos sobre expresiones.
        //
        // Comparar ids basta porque la arena hace hash-consing: dos ecuaciones
        // estructuralmente iguales **son** el mismo id.
        if !equations.contains(&equation) {
            equations.push(equation);
        }
    }

    if equations.is_empty() {
        return Ok(None);
    }
    Ok(Some(arena.app(Head::Solutions, &equations)?))
}

/// `ax + b = 0` → `x = -b/a`.
pub fn isolate_linear(arena: &mut Arena, id: ExprId) -> Result<Option<ExprId>, CoreError> {
    let Some((left, right)) = sides(arena, id) else {
        return Ok(None);
    };
    if !is_zero(arena, right) || is_solved(arena, id) {
        return Ok(None);
    }
    let Some(univariate) = as_univariate(arena, left) else {
        return Ok(None);
    };
    if univariate.poly.degree() != Some(1) {
        return Ok(None);
    }

    let slope = univariate.poly.coefficient(1);
    let intercept = univariate.poly.coefficient(0);
    // Grado 1 garantiza que la pendiente no es nula —un coeficiente nulo no se
    // guarda— pero dividir sin comprobarlo sería confiar en una invariante de
    // otro módulo en vez de en un dato que se tiene delante.
    if slope.is_zero() {
        return Ok(None);
    }
    let root = -intercept / slope;

    let unknown = arena.sym_id(univariate.variable)?;
    let solution = to_expr(arena, univariate.variable, &Poly::constant(root))?;
    Ok(Some(arena.app(Head::Equal, &[unknown, solution])?))
}

/// ¿Es `p = 0` una ecuación de verdad cuyas raíces **ninguna** es racional?
///
/// Se pregunta al final, cuando factorizar y despejar ya se han rendido, y es
/// restrictiva a propósito: exige que el lado izquierdo sea un polinomio
/// univariado de grado ≥ 1 y que el derecho sea cero. Una expresión que este
/// motor no sabe leer no es una ecuación sin raíces racionales — es una que no
/// se ha entendido, y confundirlas afirmaría un teorema sobre cualquier cosa.
pub fn has_no_rational_solutions(arena: &Arena, id: ExprId) -> bool {
    let Some((left, right)) = sides(arena, id) else {
        return false;
    };
    if !is_zero(arena, right) {
        return false;
    }
    let Some(univariate) = as_univariate(arena, left) else {
        return false;
    };
    if univariate.poly.degree().is_none_or(|degree| degree < 1) {
        return false;
    }
    ms_alg::rational_roots(&univariate.poly).is_empty()
}

/// Las raíces racionales del lado izquierdo de `p = 0`, para quien quiera
/// comprobarlas en vez de calcularlas.
///
/// No lo usa ninguna regla: existe para los tests, que necesitan una respuesta
/// contra la que contrastar la que salió reescribiendo.
pub fn rational_solutions(arena: &Arena, id: ExprId) -> Option<Vec<Rational>> {
    let (left, right) = sides(arena, id)?;
    if !is_zero(arena, right) {
        return None;
    }
    let (_, poly) = as_polynomial(arena, left)?;
    Some(ms_alg::rational_roots(&poly))
}
