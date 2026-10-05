//! Las derivadas, entrando por LaTeX y saliendo por LaTeX.
//!
//! Lo que se comprueba aquí es lo que el estudiante ve: la respuesta **y los
//! pasos que la explican**, que en E3 valen más que la respuesta. Que la derivada
//! sea la correcta lo comprueba además `ms-verify` por su cuenta y con números
//! duales; esto es lo otro, y ningún test lo sustituye.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_assume::Assumptions;
use ms_core::Arena;
use ms_parse::parse;
use ms_rules::{DEFAULT_BUDGET, RULES, RuleError, rewrite};
use ms_serialize::to_latex;

/// La respuesta y los ids de las reglas que dejaron paso, en orden.
fn derive(latex: &str) -> (String, Vec<&'static str>) {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, latex).unwrap_or_else(|error| panic!("«{latex}»: {error}"));
    let outcome = rewrite(
        &mut arena,
        parsed,
        RULES,
        &Assumptions::new(),
        DEFAULT_BUDGET,
    )
    .unwrap_or_else(|error| panic!("«{latex}»: {error}"));

    let written = to_latex(&arena, outcome.expression).expect("to_latex");
    let steps = outcome.steps.iter().map(|step| step.rule).collect();
    (written, steps)
}

fn answer(latex: &str) -> String {
    derive(latex).0
}

fn failure(latex: &str) -> RuleError {
    let mut arena = Arena::new();
    let parsed = parse(&mut arena, latex).unwrap_or_else(|error| panic!("«{latex}»: {error}"));
    rewrite(
        &mut arena,
        parsed,
        RULES,
        &Assumptions::new(),
        DEFAULT_BUDGET,
    )
    .err()
    .unwrap_or_else(|| panic!("«{latex}» se resolvió, y no debería"))
}

/// **El criterio de salida de E3.** La cadena sobre un polinomio, sin expandirlo,
/// y los cuatro pasos que lo explican.
///
/// **Los dos últimos van en orden canónico, no en el de lectura.** El reescritor
/// recorre los sumandos como los tiene la arena, y el escritor los ordena por
/// grado sólo al escribirlos: por eso el `+1` se deriva antes que el `x^{2}`
/// aunque salga escrito después. Cruzarlos exigiría que el reescritor conociera
/// el orden del escritor, que es presentación — y esa frontera se mantiene
/// separada a propósito (02-architecture.md §1). Queda anotado como cabo suelto.
#[test]
fn the_exit_criterion_of_e3() {
    let (answer, steps) = derive("\\frac{d}{dx}\\left(\\left(x^{2}+1\\right)^{3}\\right)");

    assert_eq!(answer, "6x\\left(x^{2} + 1\\right)^{2}");
    assert_eq!(
        steps,
        vec![
            "derivative-chain",
            "derivative-sum",
            "derivative-constant",
            "derivative-power",
        ]
    );
}

#[test]
fn the_derivative_of_a_power_of_the_variable() {
    assert_eq!(answer("\\frac{d}{dx}\\left(x^{2}\\right)"), "2x");
    assert_eq!(
        answer("\\frac{d}{dx}\\left(x^{5}\\right)"),
        "5x^{4}"
    );
}

#[test]
fn the_derivative_of_the_variable_is_one_and_of_a_constant_is_zero() {
    assert_eq!(answer("\\frac{d}{dx}\\left(x\\right)"), "1");
    assert_eq!(answer("\\frac{d}{dx}\\left(7\\right)"), "0");
}

/// Otra letra es una constante respecto de `x`. La lectura contraria —`y` es una
/// función de `x`— es la diferenciación implícita, que exige poder escribir
/// `y(x)`.
#[test]
fn another_symbol_is_a_constant_with_respect_to_the_variable() {
    assert_eq!(answer("\\frac{d}{dx}\\left(y\\right)"), "0");
}

/// La constante sale fuera en vez de anunciarse como derivada de un producto.
#[test]
fn a_constant_factor_comes_out_of_the_derivative() {
    let (answer, steps) = derive("\\frac{d}{dx}\\left(3x^{2}\\right)");

    assert_eq!(answer, "6x");
    assert_eq!(
        steps,
        vec!["derivative-constant-multiple", "derivative-power"]
    );
}

#[test]
fn a_polynomial_is_derived_term_by_term() {
    assert_eq!(
        answer("\\frac{d}{dx}\\left(x^{3}+2x^{2}-5x+7\\right)"),
        "3x^{2} + 4x - 5"
    );
}

/// Con todos los factores dependiendo de la variable sí es la regla del producto.
#[test]
fn a_genuine_product_uses_the_product_rule() {
    let (_, steps) = derive("\\frac{d}{dx}\\left(x\\left(x+1\\right)\\right)");

    assert!(
        steps.contains(&"derivative-product"),
        "los pasos fueron {steps:?}"
    );
}

/// La segunda derivada sale por composición: el reescritor va de fuera adentro y
/// la de dentro se resuelve primero. No hace falta ninguna regla nueva.
#[test]
fn a_second_derivative_is_a_derivative_of_a_derivative() {
    assert_eq!(
        answer("\\frac{d}{dx}\\left(\\frac{d}{dx}\\left(x^{3}\\right)\\right)"),
        "6x"
    );
}

/// Un exponente negativo es una fracción, y se deriva igual.
#[test]
fn a_negative_exponent_is_derived_like_any_other() {
    assert_eq!(
        answer("\\frac{d}{dx}\\left(x^{-2}\\right)"),
        "-2x^{-3}"
    );
}

/// **Lo que no se sabe hacer, se dice.** Devolver la pregunta sin responder es la
/// forma de fallar en la que nadie se entera de que falló.
#[test]
fn a_derivative_out_of_reach_says_so() {
    assert_eq!(
        failure("\\frac{d}{dx}\\left(x^{\\frac{1}{2}}\\right)"),
        RuleError::UnsupportedDerivative
    );
}
