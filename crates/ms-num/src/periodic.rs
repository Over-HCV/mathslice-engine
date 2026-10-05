//! Decimales periódicos: de una fracción a sus cifras.
//!
//! La dirección contraria —de las cifras a la fracción— no vive aquí: la hace el
//! parser al desazucarar `0.\overline{3}` en `\frac{3}{9}`, que es la receta
//! clásica y deja la simplificación a la vista como paso.
//!
//! Un racional reducido tiene decimal periódico exactamente cuando su
//! denominador conserva algún factor que no sea 2 ni 5. La parte no periódica
//! dura tantas cifras como el mayor exponente de 2 y de 5, y el periodo empieza
//! ahí: es el orden multiplicativo de 10 módulo lo que queda del denominador.

use num_integer::Integer as _;
use num_traits::{One, Signed, Zero};
use rustc_hash::FxHashMap;

use crate::{Integer, Rational};

/// Tope de cifras del periodo.
///
/// El periodo de `\frac{1}{n}` puede tener hasta `n - 1` cifras: para
/// `\frac{1}{99991}` son 99 990, que no es una lectura, es un volcado. Con 40 se
/// cubre todo lo que un estudiante escribe y se acota el trabajo.
pub const MAX_PERIOD_DIGITS: usize = 40;

/// Las cifras de un decimal periódico.
///
/// Las cifras van como dígitos y no como entero: `0.\overline{03}` tiene un cero
/// que un entero se comería, y con él se iría la longitud del periodo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Periodic {
    pub negative: bool,
    /// La parte entera, en magnitud.
    pub whole: Integer,
    /// Las cifras que no se repiten, entre la coma y el periodo.
    pub nonrepeating: Vec<u8>,
    /// El periodo. Nunca está vacío: si lo estuviera, el decimal sería finito.
    pub repeating: Vec<u8>,
}

/// El desarrollo periódico de `value`, o `None` si no lo tiene —porque es finito
/// o entero— o si su periodo pasa de [`MAX_PERIOD_DIGITS`].
pub fn periodic_decimal(value: &Rational) -> Option<Periodic> {
    let magnitude = value.abs();
    let denominator = magnitude.denom().clone();
    if denominator.is_one() {
        return None;
    }

    // Un denominador sin más factores que 2 y 5 da un decimal finito, y ése es
    // otro hecho.
    let mut coprime = denominator.clone();
    strip_factor(&mut coprime, 2);
    strip_factor(&mut coprime, 5);
    if coprime.is_one() {
        return None;
    }

    let whole = magnitude.to_integer();
    let mut remainder = magnitude.numer() % magnitude.denom();

    // División larga, guardando en qué posición apareció cada resto. El primer
    // resto repetido marca el principio del periodo: a partir de ahí las cifras
    // se repiten porque el estado es el mismo.
    let mut digits: Vec<u8> = Vec::new();
    let mut seen: FxHashMap<Integer, usize> = FxHashMap::default();
    let ten = Integer::from(10);

    loop {
        if let Some(start) = seen.get(&remainder) {
            let (nonrepeating, repeating) = digits.split_at(*start);
            if repeating.len() > MAX_PERIOD_DIGITS {
                return None;
            }
            return Some(Periodic {
                negative: value.is_negative(),
                whole,
                nonrepeating: nonrepeating.to_vec(),
                repeating: repeating.to_vec(),
            });
        }
        seen.insert(remainder.clone(), digits.len());

        remainder *= &ten;
        let (digit, rest) = remainder.div_rem(&denominator);
        digits.push(digit_of(&digit)?);
        remainder = rest;

        // Un resto nulo significaría decimal finito, y eso ya se descartó arriba
        // mirando el denominador. Si pasara, no hay periodo que devolver.
        if remainder.is_zero() {
            return None;
        }
        // La longitud no puede crecer sin límite: el periodo está acotado por el
        // denominador. El tope evita gastar ese trabajo cuando ya no se va a
        // poder mostrar.
        if digits.len() > MAX_PERIOD_DIGITS * 2 {
            return None;
        }
    }
}

fn digit_of(value: &Integer) -> Option<u8> {
    u8::try_from(u32::try_from(value.magnitude().clone()).ok()?).ok()
}

/// Divide `value` por `factor` mientras se pueda.
fn strip_factor(value: &mut Integer, factor: i64) {
    let factor = Integer::from(factor);
    while (&*value % &factor).is_zero() {
        *value /= &factor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(numerator: i64, denominator: i64) -> Rational {
        Rational::new(Integer::from(numerator), Integer::from(denominator))
    }

    fn periodic(numerator: i64, denominator: i64) -> Option<Periodic> {
        periodic_decimal(&value(numerator, denominator))
    }

    #[test]
    fn a_third_is_zero_point_three_repeating() {
        let third = periodic(1, 3).expect("periódico");

        assert_eq!(third.whole, Integer::zero());
        assert!(third.nonrepeating.is_empty());
        assert_eq!(third.repeating, vec![3]);
    }

    /// `\frac{1}{6}` tiene una cifra antes del periodo: el 2 del denominador la
    /// pone, y confundirla con el periodo daría `0.\overline{16}`, que es otro
    /// número.
    #[test]
    fn a_sixth_has_one_digit_before_the_period() {
        let sixth = periodic(1, 6).expect("periódico");

        assert_eq!(sixth.nonrepeating, vec![1]);
        assert_eq!(sixth.repeating, vec![6]);
    }

    /// El periodo de `\frac{1}{7}` son las seis cifras clásicas.
    #[test]
    fn a_seventh_has_a_period_of_six_digits() {
        let seventh = periodic(1, 7).expect("periódico");

        assert!(seventh.nonrepeating.is_empty());
        assert_eq!(seventh.repeating, vec![1, 4, 2, 8, 5, 7]);
    }

    /// El cero del periodo de `\frac{1}{33}` es la razón de que las cifras no
    /// viajen como entero: `03` y `3` son periodos distintos.
    #[test]
    fn a_leading_zero_inside_the_period_survives() {
        let thirty_third = periodic(1, 33).expect("periódico");

        assert_eq!(thirty_third.repeating, vec![0, 3]);
    }

    #[test]
    fn the_whole_part_and_the_sign_travel_apart() {
        let value = periodic(-22, 3).expect("periódico");

        assert!(value.negative);
        assert_eq!(value.whole, Integer::from(7));
        assert_eq!(value.repeating, vec![3]);
    }

    /// Un decimal finito y un entero no tienen periodo, y decir que sí sería
    /// escribir `0.5\overline{0}`.
    #[test]
    fn terminating_decimals_and_integers_have_no_period() {
        assert_eq!(periodic(1, 2), None);
        assert_eq!(periodic(3, 8), None);
        assert_eq!(periodic(6, 3), None);
    }

    /// Un periodo enorme no se devuelve a medias: se dice que no.
    #[test]
    fn a_period_beyond_the_budget_is_refused() {
        // El periodo de 1/99991 tiene decenas de miles de cifras.
        assert_eq!(periodic(1, 99_991), None);
    }
}
