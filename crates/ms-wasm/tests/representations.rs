//! Qué denota un número escrito de otra forma: decimales y porcentajes.
//!
//! Un caso de corpus fija **una** entrada; la propiedad cubre el espacio
//! (05-testing.md §10). Lo que se comprueba es que el desazucarado no pierde ni
//! inventa cifras, que es el fallo que ningún caso escrito a mano encuentra: el
//! cero a la izquierda que se cae y convierte `0.05` en `0.5`.
//!
//! Se mide con `ms_verify::evaluate`, no con el resultado del reescritor: el
//! evaluador exacto no conoce ninguna regla, así que una regla equivocada no
//! puede validar su propio error.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_core::Arena;
use ms_num::{Integer, Rational};
use ms_parse::parse;
use num_traits::Pow;
use proptest::prelude::*;

/// El valor exacto de lo que denota `source`, sin pasar por ninguna regla.
fn value_of(source: &str) -> Option<Rational> {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, source).ok()?;
    ms_verify::evaluate(&arena, parsed)
}

proptest! {
    /// `w.f` vale exactamente `\frac{w \cdot 10^{k} + f}{10^{k}}`, con `k` las
    /// cifras escritas detrás del punto — **las escritas**, que es donde vive el
    /// error: los ceros a la izquierda de `f` cuentan y no se pueden normalizar.
    #[test]
    fn a_decimal_is_worth_its_digits_over_a_power_of_ten(
        whole in 0u32..1000, fraction in 0u32..1000, padding in 0usize..3
    ) {
        let digits = format!("{}{fraction}", "0".repeat(padding));
        let scale = u32::try_from(digits.len()).expect("las cifras caben en u32");
        let power: Integer = Integer::from(10).pow(scale);

        let expected = Rational::new(
            Integer::from(whole) * &power + Integer::from(fraction),
            power,
        );

        prop_assert_eq!(value_of(&format!("{whole}.{digits}")), Some(expected));
    }

    /// `p\%` es `\frac{p}{100}` para todo `p`, no un caso especial del 50.
    #[test]
    fn a_percentage_is_always_a_hundredth(percentage in 0u32..10_000) {
        let expected = Rational::new(Integer::from(percentage), Integer::from(100));

        prop_assert_eq!(value_of(&format!("{percentage}\\%")), Some(expected));
    }

    /// El decimal y la fracción escrita a mano denotan el mismo número: dos
    /// notaciones de lo mismo no pueden dar dos resultados.
    #[test]
    fn a_decimal_and_the_fraction_written_by_hand_agree(
        whole in 0u32..100, hundredths in 0u32..100
    ) {
        let decimal = value_of(&format!("{whole}.{hundredths:02}"));
        let by_hand = value_of(&format!("\\frac{{{}}}{{100}}", whole * 100 + hundredths));

        prop_assert_eq!(decimal, by_hand);
    }
}
