//! Un número escrito en otra base vale lo que dice su desarrollo posicional.
//!
//! La propiedad compara dos caminos que no se conocen: el desazucarado del
//! parser, que construye `1 \cdot 2^{3} + 0 \cdot 2^{2} + …`, y la cuenta de
//! toda la vida —multiplicar por la base y sumar la cifra siguiente— hecha aquí
//! con enteros y sin tocar el motor.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_core::Arena;
use ms_num::{Integer, Rational};
use ms_parse::parse;
use proptest::prelude::*;

fn value_of(source: &str) -> Option<Rational> {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, source).ok()?;
    ms_verify::evaluate(&arena, parsed)
}

/// Las cifras escritas en `base`, con el alfabeto `0-9a-z`.
fn write_digits(digits: &[u32], base: u32) -> String {
    digits
        .iter()
        .map(|digit| char::from_digit(*digit, base).expect("cifra válida"))
        .collect()
}

/// El valor de esas cifras, por la cuenta clásica: por cada cifra, multiplicar
/// lo que llevas por la base y sumar la nueva.
fn horner(digits: &[u32], base: u32) -> Integer {
    digits.iter().fold(Integer::from(0), |accumulated, digit| {
        accumulated * Integer::from(base) + Integer::from(*digit)
    })
}

fn digits_in(base: u32) -> impl Strategy<Value = Vec<u32>> {
    prop::collection::vec(0u32..base, 1..8)
}

proptest! {
    /// El desarrollo posicional del parser y la cuenta de Horner dan el mismo
    /// número, en cualquier base de las que se pueden escribir.
    #[test]
    fn a_number_in_another_base_is_worth_its_positional_expansion(
        base in 2u32..37, seed in prop::array::uniform8(0u32..36)
    ) {
        // Las cifras se generan sobre 0..36 y se recortan a la base: así el
        // generador no depende del valor de `base`, que es otro parámetro.
        let digits: Vec<u32> = seed.iter().map(|digit| digit % base).collect();

        let source = format!("{}_{{{base}}}", write_digits(&digits, base));

        prop_assert_eq!(
            value_of(&source),
            Some(Rational::from(horner(&digits, base)))
        );
    }

    /// Una base y otra distinta del mismo número comparan iguales: la notación
    /// no cambia lo que se calcula, que es lo mismo que se comprueba de `\div`.
    #[test]
    fn the_same_number_written_in_two_bases_is_the_same_number(value in 0u32..5000) {
        let binary = format!("{:b}_{{2}}", value);
        let hex = format!("{:x}_{{16}}", value);

        let expected = Some(Rational::from(Integer::from(value)));
        prop_assert_eq!(value_of(&binary), expected.clone());
        prop_assert_eq!(value_of(&hex), expected);
    }

    /// Y en base 10 es el número escrito tal cual: si esto fallara, el
    /// desazucarado estaría inventando posiciones.
    #[test]
    fn base_ten_agrees_with_the_plain_literal(value in 0u32..100_000) {
        prop_assert_eq!(
            value_of(&format!("{value}_{{10}}")),
            value_of(&value.to_string())
        );
    }
}

/// Escribir un resultado en otra base: la vuelta de lo de arriba.
///
/// Es un **hecho**, no un paso: `\operatorname{enBase}(2, 11)` sigue valiendo
/// `11`, y lo que cambia es que el documento trae además cómo se escribe. La
/// petición se quita del árbol en la frontera, así que la derivación y la
/// verificación son las mismas que sin ella.
mod writing {
    use ms_serialize::{FactDoc, VerificationStatus};
    use ms_wasm::solve_to_document;

    fn in_base_fact(latex: &str) -> Option<FactDoc> {
        solve_to_document(latex)
            .facts
            .into_iter()
            .find(|fact| matches!(fact, FactDoc::InBase { .. }))
    }

    #[test]
    fn a_result_comes_back_written_in_the_base_that_was_asked_for() {
        let document = solve_to_document("\\operatorname{enBase}(2, 1011_{2}+1_{2})");

        let result = document.result.expect("hay resultado");
        assert_eq!(result.latex, "12");
        // La petición no toca ni el valor ni su comprobación.
        assert_eq!(document.verification.status, VerificationStatus::Verified);

        assert_eq!(
            in_base_fact("\\operatorname{enBase}(2, 1011_{2}+1_{2})"),
            Some(FactDoc::InBase {
                digits: vec![1, 1, 0, 0],
                base: 2,
                negative: false,
                latex: "1100_{2}".to_owned(),
            })
        );
    }

    /// Las cifras por encima de nueve se escriben con letra, que es la notación
    /// que el lexer sabe volver a leer.
    #[test]
    fn hexadecimal_digits_are_written_with_letters() {
        assert_eq!(
            in_base_fact("\\operatorname{enBase}(16, 255)"),
            Some(FactDoc::InBase {
                digits: vec![15, 15],
                base: 16,
                negative: false,
                latex: "ff_{16}".to_owned(),
            })
        );
    }

    /// Sin envoltorio, base diez y ningún hecho de base: lo de siempre. La base
    /// por defecto no es una decisión nueva, es la que ya había.
    #[test]
    fn a_query_without_a_wrapper_asks_for_nothing_and_gets_no_base_fact() {
        assert_eq!(in_base_fact("11"), None);
    }

    /// El signo va fuera de las cifras, como en la mixta y en el periódico.
    #[test]
    fn the_sign_goes_outside_the_digits() {
        assert_eq!(
            in_base_fact("\\operatorname{enBase}(2, -11)"),
            Some(FactDoc::InBase {
                digits: vec![1, 0, 1, 1],
                base: 2,
                negative: true,
                latex: "-1011_{2}".to_owned(),
            })
        );
    }

    /// Pedir la base dentro de la expresión no es pedir una base de salida, y se
    /// explica en vez de devolver un documento a medias sin verificar.
    #[test]
    fn asking_for_a_base_inside_the_expression_is_refused_with_an_explanation() {
        for source in [
            "2+\\operatorname{enBase}(2, 11)",
            "\\operatorname{enBase}(2, \\operatorname{enBase}(3, 11))",
        ] {
            let document = solve_to_document(source);
            let error = document.error.expect("hay error");
            assert_eq!(error.kind, "output-base", "«{source}»");
            assert!(document.result.is_none(), "«{source}»");
        }
    }

    /// Una base que no se puede escribir y un valor que no es entero: las dos
    /// peticiones se rechazan con su motivo, en vez de contestar otra pregunta.
    #[test]
    fn a_request_that_cannot_be_answered_says_so_instead_of_answering_another() {
        for source in [
            "\\operatorname{enBase}(1, 11)",
            "\\operatorname{enBase}(37, 11)",
            "\\operatorname{enBase}(16, \\frac{1}{2})",
            "\\operatorname{enBase}(2, x)",
        ] {
            let document = solve_to_document(source);
            let error = document.error.expect("hay error");
            assert_eq!(error.kind, "output-base", "«{source}»");
        }
    }

    /// La ida y la vuelta: lo que el motor escribe se vuelve a leer y da el
    /// mismo número. Es la propiedad `parse(latex(e)) == e` de esta capacidad.
    #[test]
    fn what_the_engine_writes_reads_back_as_the_same_number() {
        for (value, base) in [(11_i64, 2), (255, 16), (1204, 10), (35, 36), (0, 2)] {
            let Some(FactDoc::InBase { latex, .. }) =
                in_base_fact(&format!("\\operatorname{{enBase}}({base}, {value})"))
            else {
                panic!("{value} en base {base} no trajo su hecho");
            };

            let document = solve_to_document(&latex);
            let result = document.result.expect("el motor lee lo que escribió");
            assert_eq!(result.latex, value.to_string(), "{latex}");
        }
    }
}
