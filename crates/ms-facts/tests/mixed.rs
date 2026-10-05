//! La vista mixta tiene que valer lo mismo que la fracción de la que sale.
//!
//! Un caso fija un número; la propiedad cubre el espacio (05-testing.md §10). Y
//! aquí hay dos sitios donde equivocarse, los dos invisibles en los ejemplos
//! bonitos: el signo —que en una mixta afecta al conjunto y no al numerador— y
//! el resto, que tiene que quedar por debajo de 1 o la mixta no es mixta.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_facts::{Fact, about_rational};
use ms_num::{Integer, Rational};
use num_traits::Signed;
use proptest::prelude::*;

/// La mixta de `value`, si la tiene.
///
/// Se **busca** en la lista en vez de exigir que sea el único hecho: `\frac{5}{2}`
/// trae también su porcentaje, y una propiedad que se rompe al añadir un hecho
/// nuevo estaría comprobando la lista, no la mixta.
fn mixed_of(value: &Rational) -> Option<(Integer, Integer, Integer, bool)> {
    // `if let` y no `match` con `_`: el lint `wildcard_enum_match_arm` está
    // denegado en todo el workspace, y aquí por el mismo motivo que en el
    // motor — cuando `Fact` gane una variante, quiero elegir qué hago con ella.
    about_rational(value).into_iter().find_map(|fact| {
        if let Fact::Mixed {
            whole,
            numerator,
            denominator,
            negative,
        } = fact
        {
            return Some((whole, numerator, denominator, negative));
        }
        None
    })
}

fn numerator() -> impl Strategy<Value = i64> {
    -500i64..500
}

fn denominator() -> impl Strategy<Value = i64> {
    2i64..50
}

proptest! {
    /// `whole + \frac{numerator}{denominator}`, con el signo aplicado al
    /// conjunto, es la fracción de partida. Reconstruirla es la única forma de
    /// comprobar que no se ha perdido nada por el camino.
    #[test]
    fn a_mixed_view_adds_back_up_to_the_fraction_it_came_from(
        a in numerator(), b in denominator()
    ) {
        let value = Rational::new(Integer::from(a), Integer::from(b));

        // Falta la mixta exactamente en dos casos, y los dos son correctos: el
        // valor es entero —y entonces trae los hechos de un entero— o es una
        // fracción propia, que no tiene parte entera que sacar.
        let Some((whole, numerator, denominator, negative)) = mixed_of(&value) else {
            let is_integer = value.is_integer();
            let is_proper = value.abs() < Rational::from(Integer::from(1));
            prop_assert!(is_integer || is_proper);
            return Ok(());
        };

        let magnitude = Rational::from(whole) + Rational::new(numerator, denominator);
        let rebuilt = if negative { -magnitude } else { magnitude };

        prop_assert_eq!(rebuilt, value);
    }

    /// El resto de una mixta es menor que 1 y no nulo: con un resto de 0 la
    /// fracción era entera y no hay mixta que mostrar, y con un resto de 1 o más
    /// la parte entera se quedó corta.
    #[test]
    fn the_remainder_of_a_mixed_view_is_a_proper_fraction(
        a in numerator(), b in denominator()
    ) {
        let value = Rational::new(Integer::from(a), Integer::from(b));

        if let Some((_, numerator, denominator, _)) = mixed_of(&value) {
            prop_assert!(numerator > Integer::from(0));
            prop_assert!(numerator < denominator);
        }
    }
}
