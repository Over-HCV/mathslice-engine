//! El círculo del decimal periódico: fracción → cifras → fracción.
//!
//! Las dos direcciones viven en sitios distintos y no se conocen: las cifras las
//! saca `ms_num::periodic_decimal` por división larga, y la fracción la
//! reconstruye el **parser** al desazucarar `0.\overline{3}` en `\frac{3}{9}`.
//! Que el viaje de ida y vuelta cierre es lo que dice que las dos recetas son la
//! misma matemática, y ninguna de las dos puede validarse sola.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_core::Arena;
use ms_facts::{Fact, about_rational};
use ms_num::{Integer, Rational};
use ms_parse::parse;
use proptest::prelude::*;

/// El desarrollo periódico de un racional, escrito como lo escribe el motor.
fn period_latex(value: &Rational) -> Option<String> {
    about_rational(value).into_iter().find_map(|fact| {
        if let Fact::Periodic(periodic) = fact {
            let digits = |cifras: &[u8]| -> String {
                cifras.iter().map(|d| char::from(b'0' + d)).collect()
            };
            let sign = if periodic.negative { "-" } else { "" };
            return Some(format!(
                "{sign}{}.{}\\overline{{{}}}",
                periodic.whole,
                digits(&periodic.nonrepeating),
                digits(&periodic.repeating)
            ));
        }
        None
    })
}

/// Lo que denota una entrada de LaTeX, evaluado exacto y sin pasar por reglas.
fn value_of(source: &str) -> Option<Rational> {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, source).ok()?;
    ms_verify::evaluate(&arena, parsed)
}

proptest! {
    /// Un periódico escrito por el motor, releído, es el número de partida.
    #[test]
    fn a_period_written_and_read_back_is_the_same_number(
        numerator in -300i64..300, denominator in 1i64..300
    ) {
        let value = Rational::new(Integer::from(numerator), Integer::from(denominator));

        // Sin periodo no hay nada que comprobar: o es entero, o su decimal es
        // finito, o el periodo pasó del tope. Los tres son respuestas.
        let Some(latex) = period_latex(&value) else {
            return Ok(());
        };

        prop_assert_eq!(value_of(&latex), Some(value));
    }
}
