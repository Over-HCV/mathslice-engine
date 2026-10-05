//! `Expr` ↔ `Poly<Rational>`: el único módulo de este crate que ve un `ExprId`.
//!
//! **El polinomio es una vista, no un nodo.** No hay `Node::Poly` ni la habrá:
//! se construye desde `Plus`/`Times`/`Power` y se colapsa de vuelta a lo mismo.
//! Es exactamente lo que `as_fraction`/`make_fraction` hacen en `ms-rules` con
//! los racionales, y es el único precedente que hay en el motor.
//!
//! Meterlo en el IR costaría un `match` nuevo en cada sitio que enumera `Node`
//! —el orden canónico, el hash-consing, el JSON, el LaTeX, el evaluador— a
//! cambio de nada: el reescritor no necesita saber que algo es un polinomio, sólo
//! la regla que lo está mirando.

use ms_core::{Arena, CoreError, ExprId, Head, Node, SymId};
use ms_num::{Integer, Rational};
use num_traits::{One, Signed, Zero};

use crate::factor::Factored;
use crate::poly::Poly;

/// El coeficiente con el que trabaja E2.
///
/// Concreto y no genérico porque la traducción **es** concreta: la arena guarda
/// `Int` y `Rat`, no un `C` cualquiera. Lo genérico es el álgebra, que vive en
/// [`crate::poly`]; esto es la frontera donde el IR se convierte en álgebra, y
/// una frontera genérica no tendría de dónde sacar los coeficientes.
pub type Coefficients = Rational;

/// Un polinomio leído de la arena, con la variable de la que lo es.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Univariate {
    pub variable: SymId,
    pub poly: Poly<Coefficients>,
}

/// Lee `id` como polinomio en una sola variable.
///
/// `None` cuando no lo es, y eso incluye tres cosas que la regla que pregunte
/// tiene que tratar igual: hay más de una variable, hay un exponente que no es
/// un natural, o hay algo que no es suma, producto ni potencia. Distinguirlas
/// aquí obligaría a cada llamante a enumerarlas sin poder hacer nada distinto
/// con ninguna.
///
/// Una expresión **sin** variables sí es un polinomio: el constante. Sale con
/// `variable = None` a través de [`as_univariate`], que es quien decide si eso
/// sirve para lo que se estaba haciendo.
pub fn as_polynomial(arena: &Arena, id: ExprId) -> Option<(Option<SymId>, Poly<Coefficients>)> {
    let node = arena.get(id)?;

    match node {
        Node::Int(value) => Some((
            None,
            Poly::constant(Rational::from_integer(value.clone())),
        )),
        Node::Rat(value) => Some((None, Poly::constant(value.clone()))),
        Node::Sym(symbol) => Some((Some(*symbol), Poly::variable())),
        Node::App { head, .. } => {
            let args = arena.app_args(id)?.to_vec();
            match head {
                Head::Plus => combine(arena, &args, Poly::zero(), |left, right| {
                    Some(left.add(right))
                }),
                Head::Times => combine(arena, &args, Poly::constant(Rational::one()), |left, right| {
                    Some(left.mul(right))
                }),
                Head::Power => power_as_polynomial(arena, &args),
                // Una llamada a función, una ecuación y un conjunto no son
                // polinomios. `redondear(x, 1)` ni siquiera es continua.
                Head::Fn(_) | Head::Equal | Head::Solutions => None,
            }
        }
    }
}

/// Como [`as_polynomial`], pero exigiendo que haya una variable.
///
/// Es lo que quiere casi todo llamante: factorizar una constante o resolver una
/// ecuación sin incógnita son casos degenerados con respuestas distintas, y
/// forzar la distinción aquí evita que cada regla se olvide de una.
pub fn as_univariate(arena: &Arena, id: ExprId) -> Option<Univariate> {
    let (variable, poly) = as_polynomial(arena, id)?;
    Some(Univariate {
        variable: variable?,
        poly,
    })
}

/// Escribe un polinomio de vuelta como expresión.
///
/// Sale sin canonicalizar: quien la reciba es una regla, y el reescritor
/// canonicaliza el resultado de toda regla antes de mirarlo.
pub fn to_expr(
    arena: &mut Arena,
    variable: SymId,
    poly: &Poly<Coefficients>,
) -> Result<ExprId, CoreError> {
    if poly.is_zero() {
        return arena.int(0);
    }

    // De mayor a menor grado: es como se escribe un polinomio en clase. El orden
    // canónico lo reordenará —`Plus` es orderless— pero construirlo al revés
    // dejaría el orden a merced de un detalle interno en vez de a una decisión.
    let mut terms = Vec::with_capacity(poly.terms().len());
    for (exponent, coefficient) in poly.terms().iter().rev() {
        terms.push(term_to_expr(arena, variable, *exponent, coefficient)?);
    }

    if let [single] = terms.as_slice() {
        return Ok(*single);
    }
    arena.plus(&terms)
}

fn term_to_expr(
    arena: &mut Arena,
    variable: SymId,
    exponent: u32,
    coefficient: &Coefficients,
) -> Result<ExprId, CoreError> {
    let scalar = rational_to_expr(arena, coefficient)?;

    if exponent == 0 {
        return Ok(scalar);
    }

    let base = arena.sym_id(variable)?;

    let power = if exponent == 1 {
        base
    } else {
        let exponent = arena.int(i64::from(exponent))?;
        arena.power(base, exponent)?
    };

    // El `1` no se escribe: `1 \cdot x^{2}` es `x^{2}`. El `-1` sí se conserva
    // como factor, porque quitarlo exigiría un nodo de negación que el IR no
    // tiene — `-x` es `Times(-1, x)`.
    if coefficient.is_one() {
        return Ok(power);
    }
    arena.times(&[scalar, power])
}

fn rational_to_expr(arena: &mut Arena, value: &Coefficients) -> Result<ExprId, CoreError> {
    if value.denom().is_one() {
        return arena.int(value.numer().clone());
    }
    arena.rat(value.clone())
}

/// Los factores de una factorización, escritos con coeficientes **enteros**.
///
/// Una raíz `p/q` da el factor `qx - p`, no `x - \frac{p}{q}`: es como se
/// escribe en clase, y `2x^{2}-3x+1` sale `\left(2x-1\right)\left(x-1\right)` en
/// vez de `2\left(x-\frac{1}{2}\right)\left(x-1\right)`. Las dos son correctas y
/// sólo una se lee.
///
/// El escalar que sobra —lo que quedó sin factorizar, dividido entre los
/// denominadores que se subieron— va delante, y no se emite si es 1: multiplicar
/// por uno no se escribe.
///
/// Vive aquí y no en la regla que factoriza porque **la ecuación también los
/// necesita**: resolver `p = 0` factoriza el lado izquierdo, y dos copias de
/// esta decisión escribirían los mismos factores de dos formas distintas según
/// por dónde se hubiera llegado.
pub fn factored_to_exprs(
    arena: &mut Arena,
    variable: SymId,
    factored: &Factored,
) -> Result<Vec<ExprId>, CoreError> {
    // Cada raíz `p/q` se lleva **su** denominador. Lo que se saca fuera es el
    // producto de todos ellos, no su mcm: subir dos denominadores 2 multiplica
    // por 4.
    let mut extracted = Integer::one();
    for root in &factored.linear {
        extracted *= root.denom();
    }

    let scalar = factored.remaining.mul(&Poly::constant(Rational::new(
        Integer::one(),
        extracted,
    )));

    let mut factors = Vec::with_capacity(factored.linear.len() + 1);

    if !(scalar.degree() == Some(0) && scalar.coefficient(0).is_one()) {
        factors.push(to_expr(arena, variable, &scalar)?);
    }

    for root in &factored.linear {
        let scale = Rational::from_integer(root.denom().clone());
        let linear = Poly::from_terms([(1, scale.clone()), (0, -root.clone() * scale)]);
        factors.push(to_expr(arena, variable, &linear)?);
    }

    Ok(factors)
}

/// `base^{exponente}` como polinomio, si el exponente es un natural literal.
///
/// Un exponente negativo no lo es —`x^{-1}` es una función racional, que es otra
/// familia y otro trabajo— y uno simbólico tampoco: `x^{n}` no tiene grado hasta
/// que `n` valga algo.
fn power_as_polynomial(
    arena: &Arena,
    args: &[ExprId],
) -> Option<(Option<SymId>, Poly<Coefficients>)> {
    let [base, exponent] = args else {
        return None;
    };

    let Node::Int(exponent) = arena.get(*exponent)? else {
        return None;
    };
    if exponent.is_negative() {
        return None;
    }
    let exponent = u32::try_from(exponent.clone()).ok()?;

    let (variable, base) = as_polynomial(arena, *base)?;

    let mut accumulated = Poly::constant(Rational::one());
    for _ in 0..exponent {
        accumulated = accumulated.mul(&base);
    }
    Some((variable, accumulated))
}

/// Recorre los argumentos acumulando, y falla si aparecen dos variables
/// distintas.
///
/// Dos variables no son un error del usuario: `xy` es legítimo y frecuente. Lo
/// que pasa es que este crate todavía sólo sabe de una, y contestar como si
/// `xy` fuera un polinomio en `x` con coeficiente `y` daría un resultado
/// correcto por accidente y equivocado en cuanto la regla mirase el grado.
fn combine(
    arena: &Arena,
    args: &[ExprId],
    identity: Poly<Coefficients>,
    operation: fn(&Poly<Coefficients>, &Poly<Coefficients>) -> Option<Poly<Coefficients>>,
) -> Option<(Option<SymId>, Poly<Coefficients>)> {
    let mut accumulated = identity;
    let mut variable = None;

    for arg in args {
        let (argument_variable, poly) = as_polynomial(arena, *arg)?;

        if let Some(found) = argument_variable {
            match variable {
                Some(known) if known != found => return None,
                _ => variable = Some(found),
            }
        }

        accumulated = operation(&accumulated, &poly)?;
    }

    Some((variable, accumulated))
}

/// Un racional no nulo, o `None`. Azúcar para las reglas que necesitan el valor
/// de una constante sin volver a enumerar `Node`.
pub fn is_zero_expr(arena: &Arena, id: ExprId) -> bool {
    match arena.get(id) {
        Some(Node::Int(value)) => value.is_zero(),
        Some(Node::Rat(value)) => Zero::is_zero(value),
        Some(Node::Sym(_) | Node::App { .. }) | None => false,
    }
}
