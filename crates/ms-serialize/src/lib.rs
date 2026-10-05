//! `ms-serialize` — Expr → JSON estructural y Expr → LaTeX.
//!
//! **Serialización, no representación.** Ninguna de las dos salidas elige
//! tipografía, tamaño ni layout: eso vive en la capa de TypeScript
//! (02-architecture.md §1).

mod error;
mod facts;
mod json;
mod latex;
mod result;

pub use error::SerializeError;
pub use facts::{
    DigitPlace, FactDoc, GraphPoint, ParityValue, PrimalityValue, PrimePower, SignValue,
    decimal_latex,
    factorization_latex, in_base_latex, mixed_latex, percent_latex, periodic_latex,
};
pub use json::to_json;
pub use latex::to_latex;
pub use result::{
    EngineError, InputDoc, Numeric, ResultDoc, ResultKind, ResultValue, Span, TraceStep,
    Verification, VerificationMethod, VerificationStatus,
};

#[cfg(test)]
mod tests {
    use ms_core::{Arena, ExprId};
    use ms_num::Rational;
    use num_bigint::BigInt;

    use super::*;

    fn int(arena: &mut Arena, value: i64) -> ExprId {
        arena.int(value).unwrap()
    }

    #[test]
    fn integers_render_plainly() {
        let mut arena = Arena::new();
        let two = int(&mut arena, 2);

        assert_eq!(to_latex(&arena, two).unwrap(), "2");
    }

    #[test]
    fn rationals_put_the_sign_outside_the_fraction() {
        let mut arena = Arena::new();
        let value = arena
            .rat(Rational::new(BigInt::from(-1), BigInt::from(2)))
            .unwrap();

        assert_eq!(to_latex(&arena, value).unwrap(), "-\\frac{1}{2}");
    }

    #[test]
    fn a_reciprocal_factor_becomes_a_fraction() {
        let mut arena = Arena::new();
        let five = int(&mut arena, 5);
        let six = int(&mut arena, 6);
        let minus_one = int(&mut arena, -1);
        let sixth = arena.power(six, minus_one).unwrap();
        let five_sixths = arena.times(&[five, sixth]).unwrap();

        // Por esto el IR no necesita una cabeza `Divide`.
        assert_eq!(to_latex(&arena, five_sixths).unwrap(), "\\frac{5}{6}");
    }

    /// `\frac{14}{9}^{-1}` es LaTeX válido, se renderiza sin protestar y no dice
    /// qué se está elevando. El estudiante lee una cosa y el motor calculó otra,
    /// que es la peor forma de equivocarse que tiene una traza.
    #[test]
    fn a_fraction_raised_to_a_power_is_grouped_so_it_says_what_is_raised() {
        let mut arena = Arena::new();
        let fourteen = int(&mut arena, 14);
        let nine = int(&mut arena, 9);
        let minus_one = int(&mut arena, -1);
        let ninth = arena.power(nine, minus_one).unwrap();
        let fourteen_ninths = arena.times(&[fourteen, ninth]).unwrap();
        let reciprocal = arena.power(fourteen_ninths, minus_one).unwrap();

        assert_eq!(
            to_latex(&arena, reciprocal).unwrap(),
            "\\left(\\frac{14}{9}\\right)^{-1}"
        );
    }

    /// `Times(2, \frac{1}{3}, x)` se escribía `2\frac{1}{3}x`: un producto
    /// escrito exactamente como se escribe un número mixto. El valor es
    /// `\frac{2}{3}x` y el estudiante lee `\frac{7}{3}x`.
    ///
    /// El motor no acepta mixtas de entrada, y precisamente por eso no puede
    /// escribir algo que las imite: nadie leería esa cadena como el producto que
    /// es. Tercer defecto de la familia «el valor correcto y la lectura
    /// equivocada», después de `12^{-1}14^{-1}` y de `\frac{14}{9}^{-1}`.
    #[test]
    fn a_fraction_after_a_number_cannot_be_read_as_a_mixed_number() {
        let mut arena = Arena::new();
        let two = int(&mut arena, 2);
        let third = arena
            .rat(Rational::new(BigInt::from(1), BigInt::from(3)))
            .unwrap();
        let x = arena.sym("x").unwrap();
        let product = arena.times(&[two, third, x]).unwrap();

        assert_eq!(to_latex(&arena, product).unwrap(), "2 \\cdot \\frac{1}{3}x");
    }

    /// Dentro de una suma o de un producto la fracción NO lleva paréntesis: ya
    /// es un grupo visual y ponérselos sólo añade ruido.
    #[test]
    fn a_fraction_inside_a_sum_keeps_its_parentheses_off() {
        let mut arena = Arena::new();
        let one = int(&mut arena, 1);
        let two = int(&mut arena, 2);
        let minus_one = int(&mut arena, -1);
        let half = arena.power(two, minus_one).unwrap();
        let half = arena.times(&[one, half]).unwrap();
        let x = arena.sym("x").unwrap();
        let sum = arena.plus(&[half, x]).unwrap();

        // La `x` va delante porque los términos se escriben de mayor a menor
        // grado. Lo que se comprueba aquí es que la fracción no lleva
        // paréntesis, no en qué orden sale.
        assert_eq!(to_latex(&arena, sum).unwrap(), "x + \\frac{1}{2}");
    }

    #[test]
    fn a_bare_reciprocal_renders_with_one_on_top() {
        let mut arena = Arena::new();
        let six = int(&mut arena, 6);
        let minus_one = int(&mut arena, -1);
        let sixth = arena.power(six, minus_one).unwrap();
        let product = arena.times(&[sixth]).unwrap();

        assert_eq!(to_latex(&arena, product).unwrap(), "\\frac{1}{6}");
    }

    /// Las llaves del `\frac` ya son el grupo: un numerador negativo no lleva
    /// paréntesis. Antes salía `\frac{\left(-1\right)}{12}`.
    #[test]
    fn a_negative_numerator_needs_no_parentheses() {
        let mut arena = Arena::new();
        let minus_one = int(&mut arena, -1);
        let twelve = int(&mut arena, 12);
        let twelfth = arena.power(twelve, minus_one).unwrap();
        let value = arena.times(&[minus_one, twelfth]).unwrap();

        assert_eq!(to_latex(&arena, value).unwrap(), "\\frac{-1}{12}");
    }

    /// Un numerador con varios factores sí los necesita: sin ellos, juntar
    /// `-1` con `x` daría `-1x`, que se lee como otro producto.
    #[test]
    fn a_numerator_with_several_factors_keeps_its_parentheses() {
        let mut arena = Arena::new();
        let minus_one = int(&mut arena, -1);
        let three = int(&mut arena, 3);
        let third = arena.power(three, minus_one).unwrap();
        let x = arena.sym("x").unwrap();
        let value = arena.times(&[minus_one, x, third]).unwrap();

        assert_eq!(
            to_latex(&arena, value).unwrap(),
            "\\frac{\\left(-1\\right)x}{3}"
        );
    }

    /// Un solo factor tampoco necesita paréntesis por ser una suma.
    #[test]
    fn a_sum_alone_in_the_numerator_needs_no_parentheses() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let one = int(&mut arena, 1);
        let sum = arena.plus(&[x, one]).unwrap();
        let two = int(&mut arena, 2);
        let minus_one = int(&mut arena, -1);
        let half = arena.power(two, minus_one).unwrap();
        let value = arena.times(&[sum, half]).unwrap();

        assert_eq!(to_latex(&arena, value).unwrap(), "\\frac{x + 1}{2}");
    }

    #[test]
    fn negative_terms_are_subtracted_not_added() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let minus_three = int(&mut arena, -3);
        let sum = arena.plus(&[x, minus_three]).unwrap();

        assert_eq!(to_latex(&arena, sum).unwrap(), "x - 3");
    }

    #[test]
    fn sums_inside_products_get_parentheses() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let one = int(&mut arena, 1);
        let sum = arena.plus(&[x, one]).unwrap();
        let two = int(&mut arena, 2);
        let product = arena.times(&[two, sum]).unwrap();

        assert_eq!(to_latex(&arena, product).unwrap(), "2\\left(x + 1\\right)");
    }

    #[test]
    fn adjacent_numeric_factors_need_an_explicit_dot() {
        let mut arena = Arena::new();
        let two = int(&mut arena, 2);
        let three = int(&mut arena, 3);
        let product = arena.times(&[two, three]).unwrap();

        // Sin el \cdot, "2 3" se leería como el número 23.
        assert_eq!(to_latex(&arena, product).unwrap(), "2 \\cdot 3");
    }

    #[test]
    fn a_number_times_a_symbol_is_juxtaposed() {
        let mut arena = Arena::new();
        let two = int(&mut arena, 2);
        let x = arena.sym("x").unwrap();
        let product = arena.times(&[two, x]).unwrap();

        assert_eq!(to_latex(&arena, product).unwrap(), "2x");
    }

    #[test]
    fn powers_brace_their_exponent() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = int(&mut arena, 2);
        let power = arena.power(x, two).unwrap();

        assert_eq!(to_latex(&arena, power).unwrap(), "x^{2}");
    }

    #[test]
    fn json_encodes_bignums_as_strings() {
        let mut arena = Arena::new();
        let big = arena.int(BigInt::from(2).pow(80)).unwrap();

        let encoded = to_json(&arena, big).unwrap();

        // Como número JSON, 2^80 perdería precisión al pasar por JSON.parse.
        assert_eq!(encoded["int"], "1208925819614629174706176");
    }

    #[test]
    fn json_keeps_the_structural_tree() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = int(&mut arena, 2);
        let sum = arena.plus(&[x, two]).unwrap();

        let encoded = to_json(&arena, sum).unwrap();

        assert_eq!(encoded["h"], "Plus");
        assert_eq!(encoded["a"][0]["sym"], "x");
        assert_eq!(encoded["a"][1]["int"], "2");
    }
}
