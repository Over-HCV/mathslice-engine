//! Lo que las reglas de polinomios necesitan saber hacer.
//!
//! El álgebra está en `ms-alg`; aquí sólo se traduce entre ella y la arena, y se
//! decide **cómo se escribe** el resultado, que es lo que el estudiante lee.

use ms_alg::{Factored, Poly, as_polynomial, factor, to_expr};
use ms_core::{Arena, ExprId, SymId};
use ms_num::{Integer, Rational};
use num_traits::One;

use crate::rule::RuleResult;

/// El argumento de una llamada de un solo parámetro.
pub(crate) fn single_argument(arena: &Arena, id: ExprId) -> Option<ExprId> {
    let [only] = arena.app_args(id)? else {
        return None;
    };
    Some(*only)
}

/// ¿Es esto un polinomio en una variable?
///
/// Lo pregunta la regla que afirma que algo es irreducible, y por eso es una
/// pregunta y no un intento: afirmar que `\operatorname{redondear}(x, 1)` no
/// factoriza sería inventarse un teorema sobre algo que ni siquiera es un
/// polinomio.
pub(crate) fn is_polynomial(arena: &Arena, id: ExprId) -> bool {
    matches!(as_polynomial(arena, id), Some((Some(_), _)))
}

/// `expandir(p)` → `p` sin paréntesis.
///
/// Un argumento que no es un polinomio se queda como está: `expandir` de algo
/// que no se sabe expandir no es un error, es una petición que este motor
/// todavía no atiende, y contestar con el argumento intacto sería afirmar que ya
/// estaba expandido.
pub(crate) fn expand(arena: &mut Arena, argument: ExprId) -> RuleResult {
    let Some((variable, poly)) = as_polynomial(arena, argument) else {
        return Ok(None);
    };

    // Sin variable es una constante: el plegado de enteros ya la resuelve, y
    // reescribirla aquí sería robarle el trabajo a una regla silenciosa.
    let Some(variable) = variable else {
        return Ok(None);
    };

    let expanded = to_expr(arena, variable, &poly)?;
    Ok(Some(expanded))
}

/// `x + x + 1` → `2x + 1`: los sumandos con la misma parte simbólica se juntan.
///
/// **Es un paso pedagógico, no una limpieza**, y por eso no es silenciosa:
/// «agrupamos los términos semejantes» es lo que se dice en clase.
///
/// # Por qué hacía falta en E3 y no antes
///
/// Hasta E2, las sumas simbólicas salían de `ms-alg`, que devuelve el polinomio
/// ya con un término por grado. La regla del producto de una derivada no pasa por
/// ahí: `\frac{d}{dx}\left(x\left(x+1\right)\right)` produce `x + \left(x+1\right)`,
/// que es correcto y se lee `x + x + 1`.
///
/// # Por qué no puede expandir
///
/// Sólo junta lo que **ya** tiene la misma forma. `x\left(x+1\right)` y `x^{2}+x`
/// son la misma función y no la misma expresión; convertir la primera en la
/// segunda es expandir, y expandir es una petición con nombre precisamente para
/// que ninguna regla lo haga por su cuenta (ver `Builtin::Expand`).
pub(crate) fn collect_like_terms(arena: &mut Arena, id: ExprId) -> RuleResult {
    let Some(summands) = arena.app_args(id).map(<[ExprId]>::to_vec) else {
        return Ok(None);
    };

    // En el orden en que aparecen: agrupar por un mapa sin orden haría que la
    // respuesta dependiera del hash, y la respuesta tiene que ser la misma
    // siempre.
    let mut groups: Vec<(Vec<ExprId>, Rational)> = Vec::with_capacity(summands.len());
    let mut rest = Vec::new();
    let mut grouped = false;

    for summand in summands {
        let Some((coefficient, symbolic)) = split_coefficient(arena, summand) else {
            rest.push(summand);
            continue;
        };
        match groups.iter_mut().find(|(part, _)| *part == symbolic) {
            Some((_, total)) => {
                *total += coefficient;
                grouped = true;
            }
            None => groups.push((symbolic, coefficient)),
        }
    }

    if !grouped {
        return Ok(None);
    }

    let mut terms = rest;
    for (symbolic, coefficient) in groups {
        terms.push(write_term(arena, &symbolic, &coefficient)?);
    }
    Ok(Some(arena.plus(&terms)?))
}

/// Un sumando partido en coeficiente racional y factores simbólicos.
///
/// `None` cuando el sumando es un número puro: ésos los junta el plegado de
/// enteros, que además es silencioso, y anunciar «términos semejantes» sobre
/// `2 + 3` sería ponerle título pedagógico a una suma.
///
/// Los factores simbólicos se devuelven **en su orden**, que es el canónico
/// porque el reescritor canonicaliza entre paso y paso: así `2xy` y `3xy` caen en
/// el mismo grupo y `2xy` y `3xz` no.
fn split_coefficient(arena: &Arena, id: ExprId) -> Option<(Rational, Vec<ExprId>)> {
    if arena.head_of(id) != Some(ms_core::Head::Times) {
        return match arena.get(id)? {
            // Un número no tiene parte simbólica.
            ms_core::Node::Int(_) | ms_core::Node::Rat(_) => None,
            // Un símbolo, una potencia o una llamada son ellos mismos con
            // coeficiente 1.
            ms_core::Node::Sym(_) | ms_core::Node::App { .. } => {
                Some((Rational::one(), vec![id]))
            }
        };
    }

    let factors = arena.app_args(id)?.to_vec();
    let mut coefficient = Rational::one();
    let mut symbolic = Vec::with_capacity(factors.len());
    for factor in factors {
        match crate::fraction::rational_value(arena, factor) {
            Some(value) => coefficient *= value,
            None => symbolic.push(factor),
        }
    }

    if symbolic.is_empty() {
        return None;
    }
    Some((coefficient, symbolic))
}

/// `coeficiente · parte`, escrito como se escribe: sin el `1` delante.
fn write_term(
    arena: &mut Arena,
    symbolic: &[ExprId],
    coefficient: &Rational,
) -> Result<ExprId, crate::rule::RuleError> {
    let part = match symbolic {
        [only] => *only,
        many => arena.times(many)?,
    };
    if coefficient.is_one() {
        return Ok(part);
    }
    let coefficient = crate::fraction::make_fraction(
        arena,
        coefficient.numer().clone(),
        coefficient.denom().clone(),
    )?;
    Ok(arena.times(&[coefficient, part])?)
}

/// `factorizar(p)` → el producto de sus factores.
pub(crate) fn factorize(arena: &mut Arena, argument: ExprId) -> RuleResult {
    let Some((variable, poly)) = as_polynomial(arena, argument) else {
        return Ok(None);
    };
    let Some(variable) = variable else {
        return Ok(None);
    };
    let Some(factored) = factor(&poly) else {
        return Ok(None);
    };

    // Sin ningún factor encontrado no hay nada que escribir, y escribir el mismo
    // polinomio de vuelta dejaría la llamada disparándose sobre su resultado.
    if factored.linear.is_empty() {
        return Ok(None);
    }

    let factors = write_factors(arena, variable, &factored)?;
    if let [single] = factors.as_slice() {
        return Ok(Some(*single));
    }
    Ok(Some(arena.times(&factors)?))
}

/// Los factores, escritos con coeficientes enteros.
///
/// Una raíz `p/q` da el factor `qx - p`, no `x - \frac{p}{q}`: es como se
/// escribe en clase, y `2x^{2}-3x+1` sale `\left(2x-1\right)\left(x-1\right)` en
/// vez de `2\left(x-\frac{1}{2}\right)\left(x-1\right)`. Las dos son correctas y
/// sólo una se lee.
///
/// El escalar que sobra —el coeficiente principal dividido entre los
/// denominadores que se subieron— se emite sólo si no es 1.
fn write_factors(
    arena: &mut Arena,
    variable: SymId,
    factored: &Factored,
) -> Result<Vec<ExprId>, ms_core::CoreError> {
    // Cada raíz `p/q` se lleva **su** denominador: el factor pasa de
    // `x - \frac{p}{q}` a `qx - p`. Lo que se saca fuera es el producto de todos
    // ellos, no su mcm — subir dos denominadores 2 multiplica por 4.
    let mut extracted = Integer::one();
    for root in &factored.linear {
        extracted *= root.denom();
    }

    let scalar = factored.remaining.mul(&Poly::constant(Rational::new(
        Integer::one(),
        extracted,
    )));

    let mut factors = Vec::with_capacity(factored.linear.len() + 1);

    // Lo que quedó sin factorizar se escribe delante, sea una constante o un
    // polinomio irreducible. El `1` no: multiplicar por uno no se escribe.
    if !(scalar.degree() == Some(0) && scalar.coefficient(0).is_one()) {
        factors.push(to_expr(arena, variable, &scalar)?);
    }

    for root in &factored.linear {
        let scale = Rational::from_integer(root.denom().clone());
        let linear = Poly::from_terms([
            (1, scale.clone()),
            (0, -root.clone() * scale),
        ]);
        factors.push(to_expr(arena, variable, &linear)?);
    }

    Ok(factors)
}
