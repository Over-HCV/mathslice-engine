//! Las leyes del redondeo.
//!
//! Un redondeo correcto no es «el número que sale parecido»: es el múltiplo de
//! `10^{-n}` más cercano, y eso se puede afirmar. La cota es lo que separa un
//! redondeo de un truncamiento, que también «sale parecido» y está mal.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_num::{Integer, Rational, round_half_away, round_half_even};
use num_traits::{Pow, Signed};
use proptest::prelude::*;

fn value(numerator: i64, denominator: i64) -> Rational {
    Rational::new(Integer::from(numerator), Integer::from(denominator))
}

/// `\frac{1}{2} \cdot 10^{-digits}`, la distancia máxima que puede moverse un
/// número al redondearlo a `digits` cifras.
fn tolerance(digits: u32) -> Rational {
    let scale = Rational::from(Pow::pow(Integer::from(10), digits));
    value(1, 2) / scale
}

proptest! {
    /// El redondeo mueve el número **como mucho** media unidad de la última
    /// cifra. Es la cota que un truncamiento no cumple: truncar `0.99` a una
    /// cifra da `0.9`, y eso se mueve `0.09` > `0.05`.
    #[test]
    fn rounding_moves_a_number_by_at_most_half_a_unit(
        numerator in -10_000i64..10_000, denominator in 1i64..1000, digits in 0u32..5
    ) {
        let original = value(numerator, denominator);
        let places = i32::try_from(digits).expect("cifras");

        for rounded in [
            round_half_away(&original, places).expect("dentro del tope"),
            round_half_even(&original, places).expect("dentro del tope"),
        ] {
            prop_assert!((rounded - &original).abs() <= tolerance(digits));
        }
    }

    /// El resultado es un múltiplo exacto de `10^{-digits}`: redondear a dos
    /// cifras y quedarse con tres sería no haber redondeado.
    #[test]
    fn the_result_is_a_multiple_of_the_last_place(
        numerator in -10_000i64..10_000, denominator in 1i64..1000, digits in 0u32..5
    ) {
        let original = value(numerator, denominator);
        let places = i32::try_from(digits).expect("cifras");
        let scale = Rational::from(Pow::pow(Integer::from(10), digits));

        let rounded = round_half_away(&original, places).expect("dentro del tope");

        prop_assert!((rounded * scale).is_integer());
    }

    /// Los dos criterios son simétricos: el signo sale del cálculo y vuelve a
    /// entrar, y ninguno arrastra el `-2.5` hacia el `-2` como haría un `floor`.
    #[test]
    fn both_criteria_are_symmetric_around_zero(
        numerator in -10_000i64..10_000, denominator in 1i64..1000, digits in 0u32..4
    ) {
        let original = value(numerator, denominator);
        let places = i32::try_from(digits).expect("cifras");

        prop_assert_eq!(
            round_half_away(&-original.clone(), places),
            round_half_away(&original, places).map(core::ops::Neg::neg)
        );
        prop_assert_eq!(
            round_half_even(&-original.clone(), places),
            round_half_even(&original, places).map(core::ops::Neg::neg)
        );
    }

    /// Los dos criterios sólo pueden discrepar en el empate exacto, y cuando lo
    /// hacen se llevan la última cifra a un lado y a otro: la diferencia entre
    /// los dos resultados es una unidad de esa cifra, nunca más.
    #[test]
    fn the_criteria_differ_only_on_a_tie(
        numerator in -10_000i64..10_000, denominator in 1i64..1000, digits in 0u32..4
    ) {
        let original = value(numerator, denominator);
        let places = i32::try_from(digits).expect("cifras");
        let scale = Rational::from(Pow::pow(Integer::from(10), digits));

        let away = round_half_away(&original, places).expect("dentro del tope");
        let even = round_half_even(&original, places).expect("dentro del tope");

        if away == even {
            return Ok(());
        }

        // Discrepan: entonces el original estaba exactamente en el medio.
        let scaled = original * &scale;
        let remainder = scaled.clone() - Rational::from(scaled.to_integer());
        prop_assert_eq!(remainder.abs(), value(1, 2));
        prop_assert_eq!((away - even).abs() * scale, Rational::from(Integer::from(1)));
    }
}
