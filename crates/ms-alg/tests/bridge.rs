//! El puente `Expr ↔ Poly`, en los dos sentidos.
//!
//! La propiedad que importa es que el viaje de ida y vuelta no cambie el
//! polinomio: si `as_polynomial` lee mal un exponente o `to_expr` escribe mal un
//! coeficiente, lo que el estudiante lee y lo que el motor calculó dejan de ser
//! lo mismo — que es la familia de defectos más cara de este motor.

#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_alg::{Poly, as_polynomial, as_univariate, to_expr};
use ms_core::Arena;
use ms_num::Rational;
use ms_parse::parse;
use ms_serialize::to_latex;
use proptest::prelude::*;

fn read(latex: &str) -> (Arena, Poly<Rational>) {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, latex).expect("parse");
    let canonical = arena.canonicalize(parsed).expect("canonicalize").id();
    let univariate = as_univariate(&arena, canonical)
        .unwrap_or_else(|| panic!("«{latex}» no se leyó como polinomio"));
    (arena, univariate.poly)
}

fn coefficients(latex: &str) -> Vec<(u32, i64)> {
    let (_, poly) = read(latex);
    poly.terms()
        .iter()
        .map(|(exponent, coefficient)| {
            assert_eq!(
                *coefficient.denom(),
                1.into(),
                "este caso esperaba coeficientes enteros"
            );
            (
                *exponent,
                i64::try_from(coefficient.numer().clone()).expect("cabe"),
            )
        })
        .collect()
}

#[test]
fn a_quadratic_reads_as_its_three_coefficients() {
    assert_eq!(coefficients("x^{2}-5x+6"), [(0, 6), (1, -5), (2, 1)]);
}

#[test]
fn a_product_of_binomials_is_read_expanded() {
    assert_eq!(
        coefficients("\\left(x-2\\right)\\left(x-3\\right)"),
        coefficients("x^{2}-5x+6")
    );
}

#[test]
fn like_terms_collapse_when_they_are_read() {
    assert_eq!(coefficients("x+2x-3x+1"), [(0, 1)]);
}

/// Un polinomio ralo no paga por los huecos: `x^{100}+1` son dos términos.
#[test]
fn a_sparse_polynomial_keeps_only_the_terms_it_has() {
    assert_eq!(coefficients("x^{100}+1"), [(0, 1), (100, 1)]);
}

#[test]
fn the_zero_polynomial_has_no_degree() {
    let (_, poly) = read("x-x+0x^{2}");
    assert!(poly.is_zero());
    assert_eq!(poly.degree(), None, "el grado del nulo no es cero");
}

/// Lo que **no** es un polinomio univariado, y por qué cada caso importa.
#[test]
fn what_is_not_a_univariate_polynomial_is_refused() {
    for source in [
        // Dos variables: legítimo, y este crate todavía no sabe de dos.
        "xy",
        // Exponente negativo: es una función racional, otra familia.
        "x^{-1}",
        // Una función no es un polinomio, y ésta ni siquiera es continua.
        "\\operatorname{redondear}(x, 1)",
    ] {
        let mut arena = Arena::new();
        let parsed = parse(&mut arena, source).expect("parse");
        let canonical = arena.canonicalize(parsed).expect("canonicalize").id();
        assert!(
            as_univariate(&arena, canonical).is_none(),
            "«{source}» no debería leerse como polinomio univariado"
        );
    }
}

/// Una constante sí es un polinomio —el de grado cero— pero no tiene variable, y
/// `as_univariate` lo distingue para que ninguna regla se lo encuentre por
/// sorpresa.
#[test]
fn a_constant_is_a_polynomial_without_a_variable() {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, "7").expect("parse");
    let canonical = arena.canonicalize(parsed).expect("canonicalize").id();

    assert!(as_univariate(&arena, canonical).is_none());
    let (variable, poly) = as_polynomial(&arena, canonical).expect("es un polinomio");
    assert_eq!(variable, None);
    assert_eq!(poly.degree(), Some(0));
}

#[test]
fn division_leaves_the_remainder_it_should() {
    let (_, dividend) = read("x^{2}-5x+6");
    let (_, divisor) = read("x-2");

    let (quotient, remainder) = dividend.divide(&divisor).expect("divisor no nulo");

    assert!(remainder.is_zero(), "x-2 divide a x^2-5x+6");
    assert_eq!(quotient, read("x-3").1);
}

#[test]
fn a_division_that_does_not_come_out_keeps_its_remainder() {
    let (_, dividend) = read("x^{2}+1");
    let (_, divisor) = read("x-1");

    let (quotient, remainder) = dividend.divide(&divisor).expect("divisor no nulo");

    assert_eq!(quotient, read("x+1").1);
    // El resto es una constante, y una constante no tiene variable: `read` pasa
    // por `as_univariate`, que la rechaza a propósito.
    assert_eq!(
        remainder,
        Poly::constant(Rational::from_integer(2.into())),
        "p(1) = 2, que es el resto"
    );
}

proptest! {
    /// Ida y vuelta: leer un polinomio, escribirlo y volver a leerlo da el mismo
    /// polinomio. Es el `parse(latex(e)) == e` del puente.
    #[test]
    fn writing_a_polynomial_and_reading_it_back_gives_the_same_one(
        terms in prop::collection::vec((0u32..6, -20i64..20), 1..5)
    ) {
        let mut arena = Arena::new();
        let variable = {
            let symbol = arena.sym("x").expect("arena");
            match arena.get(symbol) {
                Some(ms_core::Node::Sym(id)) => *id,
                _ => panic!("sym no devolvió un símbolo"),
            }
        };

        let original = Poly::from_terms(
            terms
                .into_iter()
                .map(|(exponent, coefficient)| (exponent, Rational::from_integer(coefficient.into()))),
        );

        let written = to_expr(&mut arena, variable, &original).expect("to_expr");
        let canonical = arena.canonicalize(written).expect("canonicalize").id();

        let (_, reread) = as_polynomial(&arena, canonical)
            .unwrap_or_else(|| panic!(
                "no se pudo releer «{}»",
                to_latex(&arena, canonical).expect("to_latex")
            ));

        prop_assert_eq!(
            original,
            reread,
            "escrito como «{}»",
            to_latex(&arena, canonical).expect("to_latex")
        );
    }
}

#[test]
fn factoring_a_quadratic_finds_its_two_roots() {
    let (_, poly) = read("x^{2}-5x+6");
    let factored = ms_alg::factor(&poly).expect("no es el polinomio nulo");

    assert_eq!(
        factored.linear,
        vec![
            Rational::from_integer(2.into()),
            Rational::from_integer(3.into())
        ],
        "las raíces salen ordenadas, para que los factores no bailen"
    );
    assert!(factored.is_complete());
}

#[test]
fn a_rational_root_is_found_too() {
    // 2x^2 - 3x + 1 = (2x - 1)(x - 1), con raíces 1/2 y 1.
    let (_, poly) = read("2x^{2}-3x+1");
    let factored = ms_alg::factor(&poly).expect("no nulo");

    assert_eq!(
        factored.linear,
        vec![
            Rational::new(1.into(), 2.into()),
            Rational::from_integer(1.into())
        ]
    );
}

/// Un irreducible sobre ℚ sale con lo que quedó a la vista, no con una mentira.
#[test]
fn what_does_not_factor_says_so() {
    let (_, poly) = read("x^{2}+1");
    let factored = ms_alg::factor(&poly).expect("no nulo");

    assert!(factored.linear.is_empty());
    assert!(!factored.is_complete(), "x^2+1 no tiene raíces racionales");
    assert_eq!(factored.remaining, poly);
}

#[test]
fn a_double_root_appears_twice() {
    let (_, poly) = read("x^{2}-4x+4");
    let factored = ms_alg::factor(&poly).expect("no nulo");

    assert_eq!(
        factored.linear,
        vec![
            Rational::from_integer(2.into()),
            Rational::from_integer(2.into())
        ]
    );
}

#[test]
fn a_zero_root_is_extracted_without_dividing_by_everything() {
    let (_, poly) = read("x^{2}-2x");
    let factored = ms_alg::factor(&poly).expect("no nulo");

    assert_eq!(
        factored.linear,
        vec![
            Rational::from_integer(0.into()),
            Rational::from_integer(2.into())
        ]
    );
    assert!(factored.is_complete());
}

proptest! {
    /// La propiedad que 05-testing.md §ms-alg nombra: multiplicar los factores
    /// devuelve el polinomio de partida. Es lo que hace que una factorización
    /// incorrecta no pueda pasar desapercibida.
    #[test]
    fn multiplying_the_factors_gives_back_the_polynomial(
        roots in prop::collection::vec(-8i64..8, 1..4)
    ) {
        let mut original = Poly::constant(Rational::from_integer(1.into()));
        for root in &roots {
            original = original.mul(&Poly::from_terms([
                (1, Rational::from_integer(1.into())),
                (0, Rational::from_integer((-root).into())),
            ]));
        }

        let factored = ms_alg::factor(&original).expect("no nulo");

        let mut rebuilt = factored.remaining.clone();
        for root in &factored.linear {
            rebuilt = rebuilt.mul(&Poly::from_terms([
                (1, Rational::from_integer(1.into())),
                (0, -root.clone()),
            ]));
        }

        prop_assert_eq!(original, rebuilt);
    }
}
