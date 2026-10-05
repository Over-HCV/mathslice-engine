//! `\frac{d}{dx}` es un operador, y `\frac{d}{2x}` sigue siendo una fracción.
//!
//! El round-trip de la cabeza está en `roundtrip.rs`, con el resto. Aquí está lo
//! que aquel no puede decir: que las **dos** escrituras de una derivada dan la
//! misma expresión, y que reconocer el operador no le ha quitado a `\frac` ni un
//! caso de los que ya sabía leer.

// Los `allow-*-in-tests` de clippy.toml sólo alcanzan a los módulos
// `#[cfg(test)]`; un test de integración es un crate aparte y no los ve.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_core::{Arena, Builtin, ExprId, Head};
use ms_parse::parse;
use ms_serialize::to_latex;

fn parse_canonical(arena: &mut Arena, latex: &str) -> ExprId {
    let parsed = parse(arena, latex).unwrap_or_else(|error| panic!("«{latex}»: {error}"));
    arena.canonicalize(parsed).expect("canonicalize").id()
}

fn parse_and_write(latex: &str) -> String {
    let mut arena = Arena::new();
    let canonical = parse_canonical(&mut arena, latex);
    to_latex(&arena, canonical).expect("to_latex")
}

/// La forma canónica del IR y la que se escribe en un cuaderno son la misma
/// expresión. Si dejaran de serlo, el motor daría dos respuestas distintas a la
/// misma pregunta según cómo se hubiera escrito.
#[test]
fn the_operator_and_the_named_call_are_the_same_expression() {
    let mut arena = Arena::new();

    let operator = parse_canonical(&mut arena, "\\frac{d}{dx}\\left(x^{2}\\right)");
    let call = parse_canonical(&mut arena, "\\operatorname{derivar}(x^{2}, x)");

    assert_eq!(operator, call);
}

/// Y la que sale escrita es la del cuaderno, que es la que se lee.
#[test]
fn a_derivative_is_written_with_the_operator() {
    assert_eq!(
        parse_and_write("\\operatorname{derivar}(x^{2}, x)"),
        "\\frac{d}{dx}\\left(x^{2}\\right)"
    );
}

#[test]
fn the_head_is_the_derivative_and_it_keeps_its_variable() {
    let mut arena = Arena::new();
    let derivative = parse_canonical(&mut arena, "\\frac{d}{dt}\\left(t^{3}\\right)");

    assert_eq!(
        arena.head_of(derivative),
        Some(Head::Fn(Builtin::Derivative))
    );

    let args = arena.app_args(derivative).expect("args").to_vec();
    let [_, variable] = args.as_slice() else {
        panic!("una derivada tiene operando y variable");
    };
    assert_eq!(to_latex(&arena, *variable).expect("to_latex"), "t");
}

/// El operando se lleva la potencia y no el producto, igual que el menos unario.
/// `\frac{d}{dx}x^{2}` es la derivada de `x^{2}`; si se llevara sólo la `x`, la
/// respuesta sería `1` elevado al cuadrado y nadie se enteraría de nada.
#[test]
fn the_operand_takes_the_power_but_not_the_product() {
    assert_eq!(
        parse_and_write("\\frac{d}{dx}x^{2}"),
        "\\frac{d}{dx}\\left(x^{2}\\right)"
    );
    // El producto no se lo lleva el operando: la derivada es un factor, y el
    // orden en que sale escrito lo decide la canonicalización.
    let mut arena = Arena::new();
    let written = parse_canonical(&mut arena, "\\frac{d}{dx}\\left(x\\right)y");
    let by_hand = parse_canonical(&mut arena, "\\operatorname{derivar}(x, x) \\cdot y");
    assert_eq!(written, by_hand);
}

/// Bajo un exponente sí necesita paréntesis, y es lo que encontró el round-trip:
/// sin ellos, «la derivada, al cuadrado» se relee como «la derivada de f al
/// cuadrado».
#[test]
fn a_derivative_under_an_exponent_keeps_its_parentheses() {
    assert_eq!(
        parse_and_write("\\left(\\frac{d}{dx}\\left(x\\right)\\right)^{2}"),
        "\\left(\\frac{d}{dx}\\left(x\\right)\\right)^{2}"
    );
}

/// **La otra mitad del caso especial.** Una fracción cuyo denominador empieza por
/// `d` sigue siendo una fracción: si el reconocimiento del operador fuera más
/// ancho de la cuenta, `\frac{d}{2x}` desaparecería como división y nadie lo
/// notaría hasta ver un resultado incorrecto.
#[test]
fn a_fraction_that_only_looks_like_the_operator_stays_a_fraction() {
    // Denominador que no empieza por `d`.
    assert_eq!(parse_and_write("\\frac{d}{2x}"), "\\frac{d}{2x}");
    // Numerador que no es la `d` sola.
    assert_eq!(parse_and_write("\\frac{2d}{dx}"), "\\frac{2d}{dx}");
    // Denominador con más de un factor detrás de la `d`.
    assert_eq!(parse_and_write("\\frac{d}{dxy}"), "\\frac{d}{dxy}");
}

/// `\frac{d}{dd}` no dice cuál de las dos letras es el diferencial y cuál la
/// variable. Se lee como la fracción que es, que es la lectura que no adivina.
#[test]
fn the_variable_cannot_be_the_differential_itself() {
    assert_eq!(parse_and_write("\\frac{d}{dd}"), "\\frac{d}{dd}");
}

/// La `d` es un símbolo como cualquier otro en todas partes menos en el operador.
#[test]
fn the_letter_d_is_still_an_ordinary_symbol() {
    assert_eq!(parse_and_write("d + 1"), "d + 1");
    assert_eq!(parse_and_write("2d"), "2d");
}
