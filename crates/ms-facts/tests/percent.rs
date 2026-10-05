//! El porcentaje de un valor vale lo mismo que el valor.
//!
//! La otra mitad de la propiedad es igual de importante: cuando **no** hay
//! porcentaje es porque no existe uno exacto. Un hecho que falta por descuido y
//! un hecho que falta porque no se puede afirmar se ven igual desde fuera, y sin
//! esta mitad la propiedad pasaría con una función que no devuelve nada nunca.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_facts::{Fact, about_rational};
use ms_num::{Integer, Rational};
use proptest::prelude::*;

const PERCENT_SCALE: i64 = 100;

fn percentage_of(value: &Rational) -> Option<Integer> {
    about_rational(value).into_iter().find_map(|fact| {
        if let Fact::Percent(percentage) = fact {
            return Some(percentage);
        }
        None
    })
}

fn hundred() -> Rational {
    Rational::from(Integer::from(PERCENT_SCALE))
}

proptest! {
    /// El porcentaje dividido entre 100 es el valor de partida.
    #[test]
    fn a_percentage_divided_by_a_hundred_is_the_value_it_came_from(
        a in -500i64..500, b in 1i64..50
    ) {
        let value = Rational::new(Integer::from(a), Integer::from(b));

        if let Some(percentage) = percentage_of(&value) {
            prop_assert_eq!(Rational::from(percentage) / hundred(), value);
        }
    }

    /// Y falta exactamente en dos casos: cuando no hay porcentaje exacto que
    /// escribir, y cuando el valor es entero — de un entero se dicen sus cinco
    /// clases, y «300 %» no es lo que nadie pregunta al escribir `3`.
    #[test]
    fn a_percentage_appears_exactly_when_it_is_exact_and_the_value_is_not_an_integer(
        a in -500i64..500, b in 1i64..50
    ) {
        let value = Rational::new(Integer::from(a), Integer::from(b));
        let expected = !value.is_integer() && (&value * hundred()).is_integer();

        prop_assert_eq!(percentage_of(&value).is_some(), expected);
    }
}
