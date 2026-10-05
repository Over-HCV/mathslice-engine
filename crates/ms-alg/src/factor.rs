//! Factorización sobre ℚ por raíces racionales.
//!
//! **Concreto sobre `Rational`, no genérico en `C`, y no es un descuido.** El
//! teorema de la raíz racional es una afirmación *sobre ℚ*: dice que si `p/q` es
//! raíz de un polinomio con coeficientes enteros, entonces `p` divide al término
//! independiente y `q` al principal. Sobre un anillo cualquiera eso no se
//! cumple, y escribirlo genérico sería generalizar un teorema que no generaliza
//! — la forma más silenciosa de que un CAS dé una respuesta incorrecta.
//!
//! Lo que sí es genérico vive en [`crate::poly`], y esta función lo usa: la
//! división euclídea que baja el grado con cada raíz encontrada es la misma para
//! cualquier cuerpo.
//!
//! **Alcance acotado a propósito.** Sólo factores lineales con raíz racional, y
//! el factor común. Cantor–Zassenhaus y el levantamiento de Hensel —que es lo
//! que hace falta para factores irreducibles de grado ≥ 2— son el algoritmo más
//! delicado que este crate llegará a tener, y con una cuadrática delante serían
//! ~400 LOC para un caso que ningún estudiante escribe. Un polinomio que no
//! factorice del todo sale con lo que quedó sin factorizar **a la vista**, que
//! es el mismo trato que `ms-num` da a una factorización entera incompleta.

use ms_num::{Integer, Rational, gcd, lcm};
use num_traits::{One, Signed, Zero};

use crate::poly::Poly;

/// Tope de candidatos a raíz que se prueban.
///
/// Los candidatos son los divisores del término independiente por los del
/// principal, y ese producto crece rápido: con coeficientes de seis cifras
/// altamente compuestos son miles de pruebas, cada una una evaluación sobre
/// bignum. El presupuesto del reescritor cuenta iteraciones y **no vería** este
/// trabajo, igual que no ve una potencia enorme; por eso el tope vive aquí.
pub const MAX_ROOT_CANDIDATES: usize = 4096;

/// Un polinomio factorizado: un escalar, sus factores lineales, y lo que quedó.
///
/// `remaining` es lo que no se supo factorizar, y va aparte en vez de fundido
/// con los factores porque significa otra cosa: los factores son un resultado, y
/// esto es una confesión. Si es de grado 0 —una constante— la factorización fue
/// completa.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Factored {
    /// Los factores lineales `x - r`, uno por raíz racional y con su
    /// multiplicidad: una raíz doble aparece dos veces.
    pub linear: Vec<Rational>,
    /// El cociente que quedó tras dividir por todos los factores encontrados.
    pub remaining: Poly<Rational>,
}

impl Factored {
    /// ¿Salió entero en factores lineales? Lo es cuando lo que queda es una
    /// constante, que es el coeficiente principal.
    pub fn is_complete(&self) -> bool {
        self.remaining.degree() == Some(0)
    }
}

/// Factoriza extrayendo raíces racionales, una a una.
///
/// Devuelve `None` sólo para el polinomio nulo, que no tiene factorización: todo
/// polinomio lo divide.
pub fn factor(poly: &Poly<Rational>) -> Option<Factored> {
    poly.degree()?;

    let mut remaining = poly.clone();
    let mut linear = Vec::new();

    // Cada raíz encontrada baja el grado en uno, así que el bucle termina. Se
    // vuelven a calcular los candidatos en cada vuelta porque el cociente tiene
    // coeficientes distintos, y con ellos otros divisores.
    while remaining.degree().is_some_and(|degree| degree > 0) {
        let Some(root) = rational_roots(&remaining).into_iter().next() else {
            break;
        };

        let divisor = linear_factor(&root);
        let Some((quotient, rest)) = remaining.divide(&divisor) else {
            break;
        };
        // Una raíz que no divide exacto no era una raíz. No debería pasar —sale
        // de evaluar a cero— pero seguir adelante convertiría un fallo aritmético
        // en una factorización incorrecta que nadie vería.
        if !rest.is_zero() {
            break;
        }

        linear.push(root);
        remaining = quotient;
    }

    Some(Factored { linear, remaining })
}

/// `x - root`.
fn linear_factor(root: &Rational) -> Poly<Rational> {
    Poly::from_terms([(1, Rational::one()), (0, -root.clone())])
}

/// Las raíces racionales, por el teorema de la raíz racional.
///
/// Se devuelven ordenadas de menor a mayor para que factorizar el mismo
/// polinomio dé siempre los mismos factores en el mismo orden: sin eso, el
/// estudiante vería `(x-3)(x-2)` unas veces y `(x-2)(x-3)` otras, sin que nada
/// hubiera cambiado.
pub fn rational_roots(poly: &Poly<Rational>) -> Vec<Rational> {
    let Some(degree) = poly.degree() else {
        return Vec::new();
    };
    if degree == 0 {
        return Vec::new();
    }

    let integral = with_integer_coefficients(poly);

    // El término independiente nulo significa que 0 es raíz, y entonces los
    // divisores de 0 son todos los enteros. Se extrae aparte.
    let constant = integral.coefficient(0);
    if constant.is_zero() {
        return vec![Rational::zero()];
    }
    let Some(leading) = integral.leading_coefficient() else {
        return Vec::new();
    };

    let numerators = divisors(&constant);
    let denominators = divisors(&leading);
    if numerators.len().saturating_mul(denominators.len()) > MAX_ROOT_CANDIDATES {
        return Vec::new();
    }

    let mut roots = Vec::new();
    for numerator in &numerators {
        for denominator in &denominators {
            for sign in [Integer::one(), -Integer::one()] {
                let candidate = Rational::new(numerator.clone() * &sign, denominator.clone());
                if poly.evaluate(&candidate).is_zero() && !roots.contains(&candidate) {
                    roots.push(candidate);
                }
            }
        }
    }

    roots.sort();
    roots
}

/// El mismo polinomio multiplicado por el mcm de los denominadores, para que los
/// coeficientes sean enteros.
///
/// Las raíces no cambian: multiplicar por una constante no nula no mueve ningún
/// cero. Y sin esto no hay teorema que aplicar — habla de coeficientes enteros.
fn with_integer_coefficients(poly: &Poly<Rational>) -> Poly<Rational> {
    let mut multiplier = Integer::one();
    for (_, coefficient) in poly.terms() {
        multiplier = lcm(&multiplier, coefficient.denom());
    }

    let scale = Rational::from_integer(multiplier);
    Poly::from_terms(
        poly.terms()
            .iter()
            .map(|(exponent, coefficient)| (*exponent, coefficient.clone() * scale.clone())),
    )
}

/// Los divisores positivos de un entero, por división por prueba hasta su raíz.
///
/// El propio valor entra siempre, así que la lista nunca sale vacía para un
/// entero no nulo — y `1` también, que es el divisor que hace que el candidato
/// `constante/1` se pruebe.
fn divisors(value: &Rational) -> Vec<Integer> {
    let value = value.numer().abs();
    if value.is_zero() {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut candidate = Integer::one();
    while &candidate * &candidate <= value {
        if (&value % &candidate).is_zero() {
            let complement = &value / &candidate;
            if !found.contains(&candidate) {
                found.push(candidate.clone());
            }
            if !found.contains(&complement) {
                found.push(complement);
            }
        }
        candidate += Integer::one();
    }
    found.sort();
    found
}

/// El máximo común divisor de dos polinomios, mónico.
///
/// Por Euclides sobre ℚ, que es donde la división con resto existe. Mónico
/// porque el MCD de dos polinomios está definido salvo constante, y elegir una
/// —la que hace 1 el coeficiente principal— es lo que lo vuelve una función.
pub fn poly_gcd(left: &Poly<Rational>, right: &Poly<Rational>) -> Option<Poly<Rational>> {
    let mut current = left.clone();
    let mut next = right.clone();

    while !next.is_zero() {
        let (_, remainder) = current.divide(&next)?;
        current = next;
        next = remainder;
    }

    current.monic()
}

/// El máximo común divisor de dos enteros, que es lo que `mcd(a, b)` consulta.
///
/// Vive aquí y no en `ms-num` —donde está [`gcd`]— porque lo que E2 añade no es
/// el cálculo sino la consulta: el signo se normaliza a no negativo y el caso
/// `mcd(0, 0)` sale como 0, que es la convención.
pub fn integer_gcd(left: &Integer, right: &Integer) -> Integer {
    gcd(left, right).abs()
}
