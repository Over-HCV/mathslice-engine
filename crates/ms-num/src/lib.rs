//! `ms-num` — la torre numérica.
//!
//! Exacto por defecto. La aproximación decimal es una **salida**, nunca un paso
//! intermedio: en cuanto un cálculo pasa por un flotante, la respuesta exacta ya
//! no se puede recuperar.
//!
//! Los traits existen para que ningún algoritmo algebraico se escriba contra un
//! tipo concreto (01-rust-principles.md §4). Cuando llegue `ms-alg` en E2, la
//! factorización modular necesita el mismo código corriendo sobre 𝔽ₚ para varios
//! primos y luego levantado a ℤ por Hensel; escribirlo dos veces sería duplicar
//! el algoritmo más delicado del crate.
//!
//! No depende de ningún crate del workspace (02-architecture.md §2).

use core::fmt::Debug;
use core::ops::{Add, Mul, Neg, Sub};

use num_integer::Integer as _;
use num_traits::{One, Pow, Signed, ToPrimitive, Zero};

mod bases;
mod integers;
mod periodic;
mod rounding;

pub use bases::{MAX_BASE, MIN_BASE, digits_in_base};
pub use integers::{
    Factorization, MAX_DIVISORS, MAX_FACTOR_BITS, factorize, is_even, is_prime,
};
pub use periodic::{MAX_PERIOD_DIGITS, Periodic, periodic_decimal};
pub use rounding::{MAX_ROUND_DIGITS, round_half_away, round_half_even};

/// Entero exacto. Es bignum por diseño, así que no desborda (§9).
pub type Integer = num_bigint::BigInt;

/// Racional exacto, siempre reducido.
pub type Rational = num_rational::BigRational;

/// Un anillo conmutativo: lo mínimo para que los polinomios de E2 sean genéricos.
pub trait Coefficient:
    Clone
    + Eq
    + Debug
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Neg<Output = Self>
    + Sized
{
    fn zero() -> Self;
    fn one() -> Self;

    fn is_zero(&self) -> bool {
        *self == Self::zero()
    }

    fn is_one(&self) -> bool {
        *self == Self::one()
    }
}

/// Un cuerpo: un anillo donde todo elemento no nulo tiene inverso.
///
/// `inv` devuelve `Option` en lugar de fallar: dividir entre cero es un caso
/// legítimo y frecuente, y su respuesta correcta es un dato, no un panic (§6).
pub trait Field: Coefficient {
    fn inv(&self) -> Option<Self>;
}

impl Coefficient for Integer {
    fn zero() -> Self {
        Zero::zero()
    }

    fn one() -> Self {
        One::one()
    }
}

impl Coefficient for Rational {
    fn zero() -> Self {
        Zero::zero()
    }

    fn one() -> Self {
        One::one()
    }
}

// Los enteros no son un cuerpo, así que `Field` se implementa sólo para
// `Rational`. Que 2 no tenga inverso entero es una verdad matemática, y aquí es
// además un error de compilación.
impl Field for Rational {
    fn inv(&self) -> Option<Self> {
        if Zero::is_zero(self) {
            return None;
        }
        Some(self.recip())
    }
}

/// Máximo común divisor, siempre no negativo.
pub fn gcd(a: &Integer, b: &Integer) -> Integer {
    a.gcd(b)
}

/// Mínimo común múltiplo. Es lo que convierte `\frac{2}{3}+\frac{1}{6}` en una
/// suma con denominador 6 y no con denominador 18: el paso pedagógico correcto
/// usa el mínimo, no el producto.
pub fn lcm(a: &Integer, b: &Integer) -> Integer {
    if Zero::is_zero(a) || Zero::is_zero(b) {
        return Zero::zero();
    }
    a.lcm(b)
}

/// Tope de tamaño de un valor exacto, en bits del numerador o del denominador.
///
/// **El presupuesto del reescritor cuenta iteraciones, no magnitud**, y esa es
/// la diferencia que mata la pestaña del usuario: `2^{10^{9}}` no cicla, calcula,
/// y un bignum de mil millones de bits agota la memoria sin gastar una sola
/// iteración del presupuesto.
///
/// 131072 bits son unos 39 000 dígitos decimales: de sobra para cualquier
/// aritmética que un estudiante escriba a mano —`2^{100}` ocupa 101— y aun así
/// finito. Vive aquí, y no en la regla que eleva, porque el verificador tiene que
/// negarse ante exactamente lo mismo: dos topes distintos serían un resultado que
/// se calcula y no se puede comprobar.
pub const MAX_EXACT_BITS: u64 = 1 << 17;

/// `base^exponent` exacto, o `None` si el resultado excedería [`MAX_EXACT_BITS`].
///
/// El tamaño se estima **antes** de calcular: comprobarlo después es haber
/// reservado ya la memoria que se quería evitar. La estimación es
/// `|exponente| · bits(base)`, que acota por arriba el tamaño real.
///
/// `None` no es «no se puede elevar»: es «no se va a calcular esto», y a quien
/// llama le toca convertirlo en una explicación para el usuario, nunca en un
/// resultado inventado (§6).
pub fn checked_pow(base: &Rational, exponent: &Integer) -> Option<Rational> {
    // `0^{0}` no vale 1: no vale nada. Es una forma indeterminada, y devolver 1
    // porque es la convención cómoda es exactamente inventarse una respuesta.
    // Elevar 0 a un exponente negativo es dividir entre cero, y tampoco.
    if Zero::is_zero(base) && !exponent.is_positive() {
        return None;
    }
    if Zero::is_zero(exponent) {
        return Some(One::one());
    }

    let magnitude = base.numer().bits().max(base.denom().bits()).max(1);
    let steps = exponent.magnitude().to_u64()?;
    if steps.checked_mul(magnitude)? > MAX_EXACT_BITS {
        return None;
    }

    // Cabe en un `i32` porque `steps · magnitude` no pasó del tope y `magnitude`
    // es al menos 1. La conversión se hace igual con `try_into`: un cast que
    // trunca en silencio es la vía de entrada de las respuestas incorrectas (§9).
    let steps: i32 = steps.try_into().ok()?;
    let power = Pow::pow(base.clone(), steps);
    if exponent.is_negative() {
        return power.inv();
    }
    Some(power)
}

/// Aproximación decimal de un racional exacto.
///
/// Devuelve `None` cuando el valor no cabe en un `f64` (un racional exacto puede
/// ser arbitrariamente grande). La UI muestra entonces sólo el valor exacto, que
/// es preferible a enseñar `inf`.
pub fn approx_f64(value: &Rational) -> Option<f64> {
    let approximation = value.to_f64()?;
    if approximation.is_finite() {
        return Some(approximation);
    }
    None
}

/// Aproximación decimal de un entero exacto. Mismo criterio que [`approx_f64`].
pub fn approx_integer_f64(value: &Integer) -> Option<f64> {
    let approximation = value.to_f64()?;
    if approximation.is_finite() {
        return Some(approximation);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integer(value: i64) -> Integer {
        Integer::from(value)
    }

    fn rational(numerator: i64, denominator: i64) -> Rational {
        Rational::new(integer(numerator), integer(denominator))
    }

    #[test]
    fn gcd_is_non_negative_for_negative_inputs() {
        assert_eq!(gcd(&integer(-12), &integer(18)), integer(6));
    }

    #[test]
    fn lcm_of_three_and_six_is_six_not_eighteen() {
        // El paso pedagógico usa el mínimo común múltiplo, no el producto.
        assert_eq!(lcm(&integer(3), &integer(6)), integer(6));
    }

    #[test]
    fn lcm_with_zero_is_zero_instead_of_dividing_by_zero() {
        assert_eq!(lcm(&integer(0), &integer(5)), integer(0));
    }

    #[test]
    fn rationals_invert_but_zero_does_not() {
        assert_eq!(rational(2, 3).inv(), Some(rational(3, 2)));
        assert_eq!(<Rational as Coefficient>::zero().inv(), None);
    }

    #[test]
    fn coefficient_identities_hold_for_both_towers() {
        // Calificado explícitamente: `num_traits` trae sus propios `is_zero` e
        // `is_one` al ámbito y sin esto la llamada es ambigua.
        assert!(Coefficient::is_zero(&<Integer as Coefficient>::zero()));
        assert!(Coefficient::is_one(&<Integer as Coefficient>::one()));
        assert!(Coefficient::is_zero(&<Rational as Coefficient>::zero()));
        assert!(Coefficient::is_one(&<Rational as Coefficient>::one()));
    }

    #[test]
    fn five_sixths_approximates_to_the_expected_decimal() {
        let approximation = approx_f64(&rational(5, 6)).unwrap();
        assert!((approximation - 0.8333333333333334).abs() < f64::EPSILON);
    }

    #[test]
    fn powers_that_a_student_would_write_are_computed_exactly() {
        let two_to_the_hundred = checked_pow(&rational(2, 1), &integer(100)).unwrap();

        assert_eq!(two_to_the_hundred.numer(), &Integer::from(2).pow(100_u32));
        assert_eq!(*two_to_the_hundred.denom(), integer(1));
    }

    #[test]
    fn a_negative_exponent_inverts_instead_of_failing() {
        assert_eq!(checked_pow(&rational(2, 3), &integer(-2)), Some(rational(9, 4)));
    }

    #[test]
    fn anything_to_the_zero_is_one_without_computing_anything() {
        assert_eq!(checked_pow(&rational(7, 5), &integer(0)), Some(rational(1, 1)));
    }

    /// El caso que motiva el tope: no cicla, calcula, y calcular esto es agotar
    /// la memoria de la pestaña.
    #[test]
    fn an_absurd_exponent_is_refused_before_allocating_anything() {
        let absurd = Integer::from(10).pow(9_u32);

        assert_eq!(checked_pow(&rational(2, 1), &absurd), None);
    }

    #[test]
    fn zero_to_a_negative_exponent_is_refused_rather_than_answered() {
        assert_eq!(checked_pow(&rational(0, 1), &integer(-1)), None);
    }

    /// La convención cómoda es que `0^{0}` vale 1. Es una convención, no un
    /// valor, y un motor que la aplica en silencio se ha inventado una respuesta.
    #[test]
    fn zero_to_the_zero_is_refused_instead_of_conveniently_being_one() {
        assert_eq!(checked_pow(&rational(0, 1), &integer(0)), None);
    }

    #[test]
    fn absurdly_large_rationals_report_no_approximation_instead_of_infinity() {
        let huge = Integer::from(10).pow(400_u32);
        let value = Rational::new(huge, integer(1));

        assert_eq!(approx_f64(&value), None);
    }
}
