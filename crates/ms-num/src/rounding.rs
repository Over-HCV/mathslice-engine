//! Redondeo exacto sobre racionales.
//!
//! Redondear es la única operación de este crate que **pierde información a
//! propósito**, y por eso vive aquí y no dentro de una regla: el reescritor y el
//! verificador tienen que redondear igual, y dos implementaciones del mismo
//! criterio de desempate acabarían discrepando en el `.5` — que es el único caso
//! interesante.
//!
//! Es el mismo reparto que [`crate::checked_pow`], que también usan los dos.
//! Lo que no se comparte es la **reescritura**: `ms-verify` no ve `ms-rules`, y
//! ahí sigue habiendo dos caminos independientes hasta el mismo número.
//!
//! Los dos criterios de desempate que se enseñan:
//!
//! - [`round_half_away`] manda el `.5` lejos del cero: `2.5 → 3`, `-2.5 → -3`.
//!   Es el redondeo de clase, y es simétrico, así que `round(-x) = -round(x)`.
//! - [`round_half_even`] manda el `.5` a la cifra par: `2.5 → 2`, `3.5 → 4`. No
//!   arrastra sesgo al sumar muchos redondeos, que es para lo que existe.

use core::cmp::Ordering;

use num_integer::Integer as _;
use num_traits::{One, Pow, Signed};

use crate::{Integer, Rational};

/// Tope de cifras a las que se puede redondear, en cualquiera de los dos
/// sentidos.
///
/// Escalar por `10^{d}` multiplica el tamaño del número por unos `3.33 · d`
/// bits, así que sin tope `\operatorname{redondear}(x, 10^{9})` agota la memoria
/// sin gastar una iteración del presupuesto — el mismo agujero que
/// [`crate::MAX_EXACT_BITS`] tapa para las potencias. Mil cifras decimales son
/// más de las que cabe pedir a mano y siguen siendo finitas.
pub const MAX_ROUND_DIGITS: u32 = 1000;

/// Redondea a `digits` cifras decimales mandando el empate lejos del cero.
///
/// `digits` negativo redondea a la izquierda de la coma: `-2` es a centenas.
///
/// `None` es «no se va a calcular esto» —la escala supera [`MAX_ROUND_DIGITS`]—
/// y a quien llama le toca convertirlo en una explicación, nunca en un resultado
/// inventado.
pub fn round_half_away(value: &Rational, digits: i32) -> Option<Rational> {
    round_to_scale(value, digits, |_| true)
}

/// Redondea a `digits` cifras decimales mandando el empate a la cifra par.
pub fn round_half_even(value: &Rational, digits: i32) -> Option<Rational> {
    // Se sube cuando el suelo es **impar**, que es lo que deja el resultado par.
    // Al revés —subir con suelo par— manda `2.5` a `3`, que es el redondeo de
    // clase con otro nombre.
    round_to_scale(value, digits, Integer::is_odd)
}

/// El redondeo, con el desempate como parámetro.
///
/// Se trabaja sobre la **magnitud** y el signo se reaplica al final. Los dos
/// criterios son simétricos, y hacerlo así evita el error clásico de que `-2.5`
/// caiga en `-2` porque `floor` mira hacia abajo y no hacia afuera.
fn round_to_scale(
    value: &Rational,
    digits: i32,
    round_up_on_tie: fn(&Integer) -> bool,
) -> Option<Rational> {
    let scale = power_of_ten(digits.unsigned_abs())?;
    let scaled = if digits.is_negative() {
        value / &scale
    } else {
        value * &scale
    };

    let magnitude = scaled.abs();
    let floor = magnitude.floor().to_integer();
    let remainder = magnitude - Rational::from(floor.clone());

    let rounded = match remainder.cmp(&half()) {
        Ordering::Less => floor,
        Ordering::Greater => floor + Integer::one(),
        // El único caso donde los dos criterios difieren, y el único que un
        // test escrito a mano se salta si sólo prueba números «normales».
        Ordering::Equal => {
            if round_up_on_tie(&floor) {
                floor + Integer::one()
            } else {
                floor
            }
        }
    };

    let signed = if value.is_negative() { -rounded } else { rounded };
    let signed = Rational::from(signed);

    Some(if digits.is_negative() {
        signed * scale
    } else {
        signed / scale
    })
}

fn power_of_ten(digits: u32) -> Option<Rational> {
    if digits > MAX_ROUND_DIGITS {
        return None;
    }
    Some(Rational::from(Pow::pow(Integer::from(10), digits)))
}

fn half() -> Rational {
    Rational::new(Integer::one(), Integer::from(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(numerator: i64, denominator: i64) -> Rational {
        Rational::new(Integer::from(numerator), Integer::from(denominator))
    }

    /// `3.456` a dos cifras es `3.46`: el 6 empuja al 5.
    #[test]
    fn a_number_rounds_to_the_requested_decimals() {
        let value = rational(3456, 1000);

        assert_eq!(round_half_away(&value, 2), Some(rational(346, 100)));
        assert_eq!(round_half_away(&value, 1), Some(rational(35, 10)));
        assert_eq!(round_half_away(&value, 0), Some(rational(3, 1)));
    }

    /// El empate exacto es donde los dos criterios se separan, y es el único
    /// sitio donde el redondeo tiene algo interesante que decir.
    #[test]
    fn the_two_criteria_disagree_exactly_on_the_tie() {
        let two_and_a_half = rational(5, 2);
        let three_and_a_half = rational(7, 2);

        assert_eq!(round_half_away(&two_and_a_half, 0), Some(rational(3, 1)));
        assert_eq!(round_half_even(&two_and_a_half, 0), Some(rational(2, 1)));

        assert_eq!(round_half_away(&three_and_a_half, 0), Some(rational(4, 1)));
        assert_eq!(round_half_even(&three_and_a_half, 0), Some(rational(4, 1)));
    }

    /// Los dos criterios son simétricos: `-2.5` va a `-3` con el de clase, y no
    /// a `-2` como haría un `floor` mal aplicado.
    #[test]
    fn rounding_is_symmetric_around_zero() {
        assert_eq!(round_half_away(&rational(-5, 2), 0), Some(rational(-3, 1)));
        assert_eq!(round_half_even(&rational(-5, 2), 0), Some(rational(-2, 1)));
        assert_eq!(round_half_even(&rational(-7, 2), 0), Some(rational(-4, 1)));
    }

    /// Cifras negativas redondean a la izquierda de la coma.
    #[test]
    fn negative_digits_round_to_tens_and_hundreds() {
        let value = rational(1234, 1);

        assert_eq!(round_half_away(&value, -1), Some(rational(1230, 1)));
        assert_eq!(round_half_away(&value, -2), Some(rational(1200, 1)));
        assert_eq!(round_half_away(&value, -3), Some(rational(1000, 1)));
    }

    /// Un racional periódico no tiene forma decimal finita, y redondearlo es
    /// justo para lo que sirve el redondeo.
    #[test]
    fn a_repeating_rational_rounds_to_a_terminating_one() {
        let third = rational(1, 3);

        assert_eq!(round_half_away(&third, 2), Some(rational(33, 100)));
        assert_eq!(round_half_away(&rational(2, 3), 2), Some(rational(67, 100)));
    }

    /// Redondear algo que ya está redondeado no lo mueve.
    #[test]
    fn rounding_an_already_round_number_changes_nothing() {
        assert_eq!(round_half_away(&rational(7, 1), 3), Some(rational(7, 1)));
        assert_eq!(round_half_even(&rational(-7, 1), 0), Some(rational(-7, 1)));
    }

    /// Pedir un millón de cifras no es un cálculo, es una forma de agotar la
    /// memoria. Se dice que no en vez de intentarlo.
    #[test]
    fn an_absurd_precision_is_refused_instead_of_attempted() {
        let value = rational(1, 3);

        assert_eq!(round_half_away(&value, 1_000_000), None);
        assert_eq!(round_half_even(&value, -1_000_000), None);
    }
}
