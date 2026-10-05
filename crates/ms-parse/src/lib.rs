//! `ms-parse` — LaTeX → Expr.
//!
//! **La entrada del motor es LaTeX y sólo LaTeX** (02-architecture.md §2). Nada
//! de `∫`, `√` ni unicode matemático: esos glifos los dibuja la interfaz. Un
//! parser que acepta dos notaciones acaba manteniendo dos gramáticas.
//!
//! Subconjunto de E1: enteros, símbolos de una letra, `+ - * /`, `\cdot`,
//! `\times`, `\div`, `\frac`, `^`, paréntesis y `\left( \right)`, con
//! multiplicación implícita.

mod error;
mod lexer;
mod parser;

pub use error::{ParseError, ParseErrorKind, Span};
pub use parser::parse;

#[cfg(test)]
mod tests {
    use ms_core::Arena;
    use ms_serialize::to_latex;

    use super::*;

    /// Parsea y canonicaliza, que es como lo consume el motor.
    fn parse_canonical(arena: &mut Arena, source: &str) -> String {
        let expression = parse(arena, source).unwrap();
        let canonical = arena.canonicalize(expression).unwrap();
        to_latex(arena, canonical.id()).unwrap()
    }

    #[test]
    fn the_exit_criterion_of_e0_parses() {
        let mut arena = Arena::new();
        // Canonicalizar reordena: `Plus` es Orderless y 1/6 precede a 2/3 en el
        // orden estructural. Lo que el usuario escribió se conserva aparte, en
        // `ResultDoc.input.latex`; esto es ya la forma interna.
        assert_eq!(
            parse_canonical(&mut arena, "\\frac{2}{3}+\\frac{1}{6}"),
            "\\frac{1}{6} + \\frac{2}{3}"
        );
    }

    #[test]
    fn implicit_multiplication_binds_like_an_explicit_product() {
        let mut arena = Arena::new();
        assert_eq!(parse_canonical(&mut arena, "2x"), "2x");
        assert_eq!(parse_canonical(&mut arena, "2 \\cdot x"), "2x");
    }

    /// `\div` y `/` son el mismo árbol: el IR no tiene división, y el glifo que
    /// el usuario elija no puede cambiar lo que se calcula.
    #[test]
    fn the_division_glyph_and_the_slash_parse_to_the_same_tree() {
        let mut arena = Arena::new();

        let with_glyph = parse(&mut arena, "\\frac{7}{12} \\div \\frac{14}{9}").unwrap();
        let with_slash = parse(&mut arena, "\\frac{7}{12} / \\frac{14}{9}").unwrap();

        assert_eq!(
            arena.canonicalize(with_glyph).unwrap(),
            arena.canonicalize(with_slash).unwrap()
        );
    }

    #[test]
    fn products_bind_tighter_than_sums() {
        let mut arena = Arena::new();
        // Sale `2x + 1` y no `1 + 2x`: el escritor ordena los términos de una
        // suma de mayor a menor grado, que es como se escribe un polinomio. Lo
        // que se comprueba aquí es que el `2x` es **un** término, no que vaya
        // primero.
        assert_eq!(parse_canonical(&mut arena, "1 + 2x"), "2x + 1");
    }

    #[test]
    fn power_is_right_associative() {
        let mut arena = Arena::new();
        // 2^3^2 es 2^(3^2), no (2^3)^2.
        assert_eq!(parse_canonical(&mut arena, "2^3^2"), "2^{3^{2}}");
    }

    #[test]
    fn subtraction_becomes_addition_of_a_negative() {
        let mut arena = Arena::new();
        // `x - 3` es `Plus(x, -3)`. El orden canónico pone el número delante
        // —`Int` es el rango 0— y el escritor lo devuelve a su sitio: el término
        // de grado 1 primero, y el negativo escrito restando.
        assert_eq!(parse_canonical(&mut arena, "x - 3"), "x - 3");
    }

    #[test]
    fn division_becomes_a_negative_power() {
        let mut arena = Arena::new();
        assert_eq!(parse_canonical(&mut arena, "5/6"), "\\frac{5}{6}");
    }

    /// Un decimal es una fracción de denominador 10^k **sin reducir**: así el
    /// `\frac{3}{4}` no sale de la nada, lo produce un paso que se lee.
    #[test]
    fn a_decimal_is_a_fraction_over_a_power_of_ten() {
        let mut arena = Arena::new();
        assert_eq!(parse_canonical(&mut arena, "0.75"), "\\frac{75}{100}");
        assert_eq!(parse_canonical(&mut arena, "3.456"), "\\frac{3456}{1000}");
        // Las cifras a la izquierda del punto no se pierden ni se duplican.
        assert_eq!(parse_canonical(&mut arena, "12.5"), "\\frac{125}{10}");
    }

    /// `0.` no denota nada. Completarlo con un cero sería inventar una cifra que
    /// el usuario no escribió.
    #[test]
    fn a_decimal_point_without_digits_is_an_error() {
        let mut arena = Arena::new();
        let error = parse(&mut arena, "0.").unwrap_err();

        assert_eq!(error.kind_id(), "incomplete-decimal");
        assert_eq!(error.span, Span::new(0, 2));
    }

    /// El porcentaje es postfijo y liga más fuerte que la suma: se lleva el
    /// número que tiene pegado y nada más.
    #[test]
    fn a_percentage_takes_only_the_number_it_is_stuck_to() {
        let mut arena = Arena::new();
        assert_eq!(parse_canonical(&mut arena, "50\\%"), "\\frac{50}{100}");
        assert_eq!(
            parse_canonical(&mut arena, "1 + 20\\%"),
            "1 + \\frac{20}{100}"
        );
    }

    /// `p\%` y `\frac{p}{100}` son el mismo árbol, como `\div` y `/`.
    #[test]
    fn a_percentage_is_a_hundredth_and_nothing_else() {
        let mut arena = Arena::new();

        let with_sign = parse(&mut arena, "20\\%").unwrap();
        let written_out = parse(&mut arena, "\\frac{20}{100}").unwrap();

        assert_eq!(
            arena.canonicalize(with_sign).unwrap(),
            arena.canonicalize(written_out).unwrap()
        );
    }

    /// **Este motor no acepta números mixtos de entrada**, y por tanto un entero
    /// pegado a una fracción es el producto que la notación dice que es. La
    /// mixta existe sólo como vista de un resultado, porque la fracción es la
    /// forma seria de escribir esto.
    ///
    /// Está fijado con un test porque el silencio invitaría a «arreglarlo»: en
    /// un libro de primaria `2\frac{1}{3}` vale `\frac{7}{3}`, y aquí vale
    /// `\frac{2}{3}`.
    #[test]
    fn an_integer_next_to_a_fraction_is_a_product_not_a_mixed_number() {
        let mut arena = Arena::new();

        let juxtaposed = parse(&mut arena, "2\\frac{1}{3}").unwrap();
        let explicit = parse(&mut arena, "2 \\cdot \\frac{1}{3}").unwrap();

        assert_eq!(
            arena.canonicalize(juxtaposed).unwrap(),
            arena.canonicalize(explicit).unwrap()
        );
    }

    /// Una llamada se escribe con `\operatorname`, y vuelve a entrar igual.
    #[test]
    fn a_function_call_parses_and_writes_itself_back() {
        let mut arena = Arena::new();

        assert_eq!(
            parse_canonical(&mut arena, "\\operatorname{redondear}(3.5, 0)"),
            "\\operatorname{redondear}\\left(\\frac{35}{10}, 0\\right)"
        );
    }

    /// La aridad se comprueba **al parsear**: una llamada con otro número de
    /// argumentos no denota nada, y dejarla entrar en la arena convertiría un
    /// error del usuario en un misterio del motor.
    #[test]
    fn a_call_with_the_wrong_arity_is_refused_where_it_was_written() {
        let mut arena = Arena::new();

        let error = parse(&mut arena, "\\operatorname{redondear}(3.5)").unwrap_err();
        assert_eq!(error.kind_id(), "wrong-arity");
    }

    /// Un nombre que el motor no conoce se rechaza con su span sobre el nombre,
    /// no sobre el `\operatorname` ni sobre toda la entrada.
    #[test]
    fn an_unknown_function_names_itself_and_points_at_the_name() {
        let mut arena = Arena::new();
        let source = "\\operatorname{raiz}(4)";

        let error = parse(&mut arena, source).unwrap_err();

        assert_eq!(error.kind_id(), "unknown-function");
        assert_eq!(&source[error.span.start..error.span.end], "{raiz}");
    }

    /// Una coma fuera de una llamada no separa nada.
    #[test]
    fn a_comma_outside_a_call_is_refused() {
        let mut arena = Arena::new();

        assert!(parse(&mut arena, "2, 3").is_err());
    }

    /// Un número en otra base entra con su desarrollo posicional escrito: es de
    /// donde salen los pasos, y lo que hace que el `11` no aparezca de la nada.
    #[test]
    fn a_number_in_another_base_expands_by_positions() {
        let mut arena = Arena::new();

        // El `1 \cdot` se conserva: quitarlo es trabajo de la regla del neutro,
        // no del parser, y el estudiante ve la posición de cada cifra.
        assert_eq!(
            parse_canonical(&mut arena, "1011_{2}"),
            "1 + 0 \\cdot 2^{2} + 1 \\cdot 2^{1} + 1 \\cdot 2^{3}"
        );
    }

    /// Las cifras de una base grande son letras, y el lexer las lee **antes** de
    /// partirlas en variables: sin eso, `ff_{16}` serían dos veces la letra f.
    #[test]
    fn hexadecimal_digits_are_read_as_digits_not_as_variables() {
        let mut arena = Arena::new();

        assert_eq!(parse_canonical(&mut arena, "ff_{16}"), "15 + 15 \\cdot 16^{1}");
    }

    /// Una sola cifra no se desarrolla: `7 \cdot 8^{0}` no explica nada.
    #[test]
    fn a_single_digit_in_another_base_is_just_that_digit() {
        let mut arena = Arena::new();

        assert_eq!(parse_canonical(&mut arena, "7_{8}"), "7");
    }

    /// Una cifra que en esa base no existe es un error del usuario, y decírselo
    /// es mejor que calcular otro número.
    #[test]
    fn a_digit_outside_its_base_is_refused() {
        let mut arena = Arena::new();

        assert_eq!(
            parse(&mut arena, "2_{2}").unwrap_err().kind_id(),
            "digit-outside-base"
        );
    }

    /// En base 1 no hay notación posicional y por encima de 36 se acaban las
    /// letras.
    #[test]
    fn a_base_outside_the_writable_range_is_refused() {
        let mut arena = Arena::new();

        assert_eq!(
            parse(&mut arena, "10_{1}").unwrap_err().kind_id(),
            "unsupported-base"
        );
        assert_eq!(
            parse(&mut arena, "10_{37}").unwrap_err().kind_id(),
            "unsupported-base"
        );
    }

    #[test]
    fn unicode_math_is_rejected_with_its_position() {
        let mut arena = Arena::new();
        let source = "2 + √2";
        let error = parse(&mut arena, source).unwrap_err();

        assert_eq!(error.kind_id(), "unicode-math");
        // El span cae sobre el √, no sobre el principio de la entrada.
        assert_eq!(&source[error.span.start..error.span.end], "√");
    }

    #[test]
    fn unsupported_commands_name_themselves() {
        let mut arena = Arena::new();
        let error = parse(&mut arena, "\\int x").unwrap_err();

        assert_eq!(error.kind_id(), "unknown-command");
        assert_eq!(
            error.kind,
            ParseErrorKind::UnknownCommand {
                name: "int".to_owned()
            }
        );
    }

    #[test]
    fn empty_input_is_an_error_not_a_zero() {
        let mut arena = Arena::new();
        assert_eq!(
            parse(&mut arena, "   ").unwrap_err().kind_id(),
            "empty-input"
        );
    }

    #[test]
    fn a_dangling_operator_reports_the_end_of_input() {
        let mut arena = Arena::new();
        assert_eq!(
            parse(&mut arena, "2 +").unwrap_err().kind_id(),
            "unexpected-end"
        );
    }

    #[test]
    fn an_unclosed_parenthesis_is_reported() {
        let mut arena = Arena::new();
        assert!(parse(&mut arena, "2(x + 1").is_err());
    }

    #[test]
    fn frac_without_braces_is_reported() {
        let mut arena = Arena::new();
        assert_eq!(
            parse(&mut arena, "\\frac 2 3").unwrap_err().kind_id(),
            "expected-delimiter"
        );
    }
}
