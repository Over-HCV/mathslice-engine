//! `=` y el conjunto de soluciones: dónde se aceptan y dónde no.
//!
//! El round-trip de las dos cabezas nuevas está en `roundtrip.rs`, con el resto.
//! Aquí está lo contrario: las formas que se **rechazan**, que son la mitad de la
//! decisión de que una ecuación sólo viva en la raíz.

// Los `allow-*-in-tests` de clippy.toml sólo alcanzan a los módulos
// `#[cfg(test)]`; un test de integración es un crate aparte y no los ve.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_core::{Arena, Head};
use ms_parse::parse;
use ms_serialize::to_latex;

fn parse_and_write(latex: &str) -> String {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, latex).expect("parse");
    let canonical = arena.canonicalize(parsed).expect("canonicalize").id();
    to_latex(&arena, canonical).expect("to_latex")
}

fn rejection_kind(latex: &str) -> &'static str {
    let mut arena = Arena::new();
    parse(&mut arena, latex)
        .err()
        .unwrap_or_else(|| panic!("«{latex}» se aceptó, y no debería"))
        .kind_id()
}

#[test]
fn an_equation_parses_with_its_two_sides() {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, "x^{2}-5x+6 = 0").expect("parse");

    assert_eq!(arena.head_of(parsed), Some(Head::Equal));
    assert_eq!(arena.app_args(parsed).map(<[_]>::len), Some(2));
}

#[test]
fn a_solution_set_reads_back_as_it_was_written() {
    assert_eq!(
        parse_and_write("\\left\\{x = 2, x = 3\\right\\}"),
        "\\left\\{x = 2, x = 3\\right\\}"
    );
}

/// Un conjunto no tiene orden, así que las mismas soluciones escritas al revés
/// son el mismo conjunto — y salen escritas igual. Es lo que hace que la
/// respuesta no dependa del orden en que la factorización encontró las raíces.
#[test]
fn the_order_of_the_solutions_does_not_change_the_set() {
    assert_eq!(
        parse_and_write("\\left\\{x = 3, x = 2\\right\\}"),
        parse_and_write("\\left\\{x = 2, x = 3\\right\\}")
    );
}

/// `Equal(Equal(x, 2), 3)` se escribiría `x = 2 = 3`, igual que lo tecleado, y
/// significaría otra cosa. Por eso el `=` no es un operador del Pratt.
#[test]
fn equalities_do_not_chain() {
    assert_eq!(rejection_kind("x = 2 = 3"), "chained-relation");
}

/// Una ecuación sólo vive en la raíz. Meterla dentro de algo se renderizaría sin
/// protestar y no denotaría nada.
#[test]
fn an_equation_is_rejected_anywhere_but_the_root() {
    assert_eq!(rejection_kind("(x = 2)"), "missing-delimiter");
    assert_eq!(rejection_kind("\\frac{x = 1}{2}"), "missing-delimiter");
}

#[test]
fn a_solution_set_is_not_a_factor() {
    assert_eq!(rejection_kind("2\\left\\{x = 2\\right\\}"), "unexpected-token");
}

/// El cierre que falta se nombra por lo que falta, no por lo que ya se escribió.
/// Con el campo antiguo, `(x = 2)` contaba «falta un delimitador después de «)»»
/// — con el `)` delante de las narices del usuario.
///
/// La entrada llega al desajuste y no al final del texto: `2(x + 1` se queda sin
/// tokens y sale por `UnexpectedEnd`, que es otro error y otro mensaje.
#[test]
fn the_missing_delimiter_is_the_one_that_is_missing() {
    let mut arena = Arena::new();
    let error = parse(&mut arena, "(x = 2)").expect_err("una ecuación no va aquí");
    assert_eq!(error.kind_id(), "missing-delimiter");
    assert!(
        error.to_string().contains(')'),
        "el mensaje debería nombrar el paréntesis que falta: {error}"
    );
}

/// Los dos casos que destapó la propiedad del round-trip al escribir los
/// coeficientes negativos sin paréntesis. Se fijan a mano porque un fichero de
/// semillas de `proptest` no dice **qué** se rompió, y esto sí.
///
/// `Times(-1, p)` se escribe restando, y al volver a entrar el menos unario
/// tiene que empujar el signo al literal que ya está delante en vez de añadir un
/// factor `-1` nuevo. Si no, queda un `1` de más que no cambia el valor y sí la
/// expresión.
#[test]
fn a_negative_coefficient_written_as_a_subtraction_reads_back_the_same() {
    for source in [
        "0 - 1 \\cdot 0^{0}",
        "0 - 1 \\cdot 0 \\cdot 0",
        "x^{2} - 5x + 6",
        "-5x",
        "1 - \\frac{1}{2}x",
    ] {
        let mut arena = Arena::new();
        let parsed = parse(&mut arena, source).expect("parse");
        let canonical = arena.canonicalize(parsed).expect("canonicalize").id();
        let written = to_latex(&arena, canonical).expect("to_latex");

        let reparsed = parse(&mut arena, &written)
            .unwrap_or_else(|error| panic!("no se pudo releer «{written}»: {error}"));
        let reparsed = arena.canonicalize(reparsed).expect("canonicalize").id();

        assert_eq!(
            canonical, reparsed,
            "«{source}» se escribió «{written}» y volvió a entrar como otra cosa"
        );
    }
}

/// Los términos se escriben de mayor a menor grado, que es como se escribe un
/// polinomio. El orden canónico ordena por rango estructural y pondría el `6`
/// delante por ser un `Int`.
#[test]
fn the_terms_of_a_sum_are_written_by_descending_degree() {
    assert_eq!(parse_and_write("6 - 5x + x^{2}"), "x^{2} - 5x + 6");
    assert_eq!(parse_and_write("1 + 2x"), "2x + 1");
}
