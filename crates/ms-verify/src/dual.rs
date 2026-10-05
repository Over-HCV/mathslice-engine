//! Números duales: derivar **evaluando**, que es el otro camino.
//!
//! # Por qué esto y no derivar otra vez
//!
//! Comprobar `\frac{d}{dx}\left(f\right)` volviendo a derivar simbólicamente
//! sería un eco: dos implementaciones de las mismas reglas se equivocan en los
//! mismos sitios, y la segunda no encontraría nada que la primera no supiera ya.
//! `deny.toml` lo impide además por construcción — este crate no ve `ms-diff`.
//!
//! Un número dual es `a + b\varepsilon` con `\varepsilon^{2} = 0`. Evaluando la
//! expresión con `x = a + \varepsilon` y las reglas de la aritmética dual, la
//! parte real sale `f(a)` y la dual, **`f'(a)`**, sin haber derivado nada: sale
//! de multiplicar y sumar. Es aritmética exacta en ℚ, no una diferencia finita:
//! no hay `h` que elegir ni error que acotar.
//!
//! Lo que afirma es que la derivada del motor coincide con la verdadera **en los
//! puntos que se prueben**. No es una demostración —dos funciones distintas
//! coinciden en doce puntos— y por eso es nivel 0.

use ms_core::{Arena, Builtin, ExprId, Head, Node, SymId};
use ms_num::{Rational, checked_pow};
use num_traits::{One, Signed, Zero};

/// `value + prime · ε`, con `ε² = 0`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Dual {
    value: Rational,
    prime: Rational,
}

impl Dual {
    /// Una constante: no cambia, así que su parte dual es cero.
    fn constant(value: Rational) -> Self {
        Dual {
            value,
            prime: Rational::zero(),
        }
    }

    /// La variable respecto de la que se deriva, evaluada en `point`. Es el único
    /// sitio donde entra un `1` en la parte dual, y de ahí sale todo lo demás.
    fn variable(point: Rational) -> Self {
        Dual {
            value: point,
            prime: Rational::one(),
        }
    }

    fn add(self, other: Self) -> Self {
        Dual {
            value: self.value + other.value,
            prime: self.prime + other.prime,
        }
    }

    /// `(a + b\varepsilon)(c + d\varepsilon) = ac + (ad + bc)\varepsilon`, que es
    /// la regla del producto saliendo sola de que `\varepsilon^{2} = 0`.
    fn multiply(self, other: Self) -> Self {
        let prime = self.value.clone() * other.prime.clone() + self.prime * other.value.clone();
        Dual {
            value: self.value * other.value,
            prime,
        }
    }

    /// `u^{n}` con `n` entero: `u^{n} + nu^{n-1}u'\varepsilon`.
    ///
    /// El exponente negativo exige base no nula, y ahí `None` es la respuesta
    /// correcta: en ese punto la expresión no denota nada, y el muestreo lo salta
    /// como salta un denominador nulo.
    fn power(self, exponent: &ms_num::Integer) -> Option<Self> {
        if exponent.is_zero() {
            return Some(Dual::constant(Rational::one()));
        }
        if self.value.is_zero() && exponent.is_negative() {
            return None;
        }

        let value = checked_pow(&self.value, exponent)?;
        let lowered = checked_pow(&self.value, &(exponent - ms_num::Integer::one()))?;
        let prime = Rational::from(exponent.clone()) * lowered * self.prime;
        Some(Dual { value, prime })
    }
}

/// `f'(point)` exacto, derivando respecto de `variable` sin reescribir nada.
///
/// `None` cuando la expresión no se puede evaluar en ese punto: otro símbolo sin
/// ligar, un exponente fraccionario, un denominador que se anula. No saber no es
/// lo mismo que estar mal, y el muestreo salta ese punto.
pub(crate) fn differentiate(
    arena: &Arena,
    id: ExprId,
    variable: SymId,
    point: &Rational,
) -> Option<Rational> {
    Some(evaluate_dual(arena, id, variable, point)?.prime)
}

fn evaluate_dual(
    arena: &Arena,
    id: ExprId,
    variable: SymId,
    point: &Rational,
) -> Option<Dual> {
    match arena.get(id)? {
        Node::Int(value) => Some(Dual::constant(Rational::from(value.clone()))),
        Node::Rat(value) => Some(Dual::constant(value.clone())),
        Node::Sym(symbol) if *symbol == variable => Some(Dual::variable(point.clone())),
        // Otro símbolo no tiene valor aquí. Tratarlo como constante exigiría
        // saber que no depende de la variable, que es una afirmación sobre algo
        // que este evaluador no puede ver.
        Node::Sym(_) => None,
        Node::App { head, args } => {
            let args = arena.args_of(*args)?.to_vec();
            match head {
                Head::Plus => fold_dual(arena, &args, variable, point, Dual::add),
                Head::Times => fold_dual(arena, &args, variable, point, Dual::multiply),
                Head::Power => {
                    let [base, exponent] = args.as_slice() else {
                        return None;
                    };
                    // El exponente tiene que ser un entero literal. Uno con la
                    // variable dentro es `u^{v}`, que exige el logaritmo; uno
                    // fraccionario es una raíz. Los dos son E5.
                    let Node::Int(exponent) = arena.get(*exponent)? else {
                        return None;
                    };
                    let exponent = exponent.clone();
                    evaluate_dual(arena, *base, variable, point)?.power(&exponent)
                }
                // Una derivada anidada sí se sabe: es la derivada de la parte dual
                // de dentro, y este evaluador no la tiene. Se resuelve por el
                // camino corriente —el resultado ya derivado— y aquí no llega.
                //
                // Las demás funciones no son aritmética dual: `redondear` no es
                // derivable y `enBase` no es una operación.
                Head::Fn(Builtin::Derivative | Builtin::Round | Builtin::RoundHalfEven
                    | Builtin::InBase | Builtin::Gcd) => None,
                // Expandir y factorizar no cambian el valor, así que tampoco
                // cambian la derivada: se deriva lo de dentro.
                Head::Fn(Builtin::Expand | Builtin::Factor) => {
                    let [only] = args.as_slice() else {
                        return None;
                    };
                    evaluate_dual(arena, *only, variable, point)
                }
                // Una ecuación no tiene valor, así que tampoco tiene derivada.
                Head::Equal | Head::Solutions => None,
            }
        }
    }
}

fn fold_dual(
    arena: &Arena,
    args: &[ExprId],
    variable: SymId,
    point: &Rational,
    combine: fn(Dual, Dual) -> Dual,
) -> Option<Dual> {
    let mut accumulated: Option<Dual> = None;
    for arg in args {
        let next = evaluate_dual(arena, *arg, variable, point)?;
        accumulated = Some(match accumulated {
            Some(previous) => combine(previous, next),
            None => next,
        });
    }
    accumulated
}
