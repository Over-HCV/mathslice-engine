//! `ms-rules` — reglas de reescritura con traza.
//!
//! **La traza es el objetivo primario del motor.** Un CAS que sólo devuelve la
//! respuesta es una calculadora; uno que traza la derivación es material
//! didáctico, y es la razón entera de no envolver SymPy.
//!
//! Ninguna regla se escribe a mano: todas pasan por [`rules!`], que es lo que
//! garantiza que cada una tenga id estable, nombre legible, plantilla de paso y
//! campo de soundness (01-rust-principles.md §5).

// El macro genera rutas `ms_rules::Rule`, que dentro de este mismo crate no
// existirían sin este alias.
extern crate self as ms_rules;

mod fraction;
mod polynomial;
mod set;
mod rewrite;
mod rule;

pub use fraction::{as_fraction, make_fraction, rational_value};
pub use ms_rules_macro::rules;
pub use set::RULES;
pub use rewrite::{DEFAULT_BUDGET, Rewritten, Step, rewrite};
pub use rule::{Rule, RuleError, RuleResult, Soundness};

#[cfg(test)]
mod tests {
    use ms_assume::Assumptions;
    use ms_core::{Arena, ExprId};
    use ms_num::Rational;
    use num_bigint::BigInt;
    use num_traits::One;

    use super::*;

    /// Construye una fracción `n/d` tal y como sale del parser.
    fn fraction(arena: &mut Arena, numerator: i64, denominator: i64) -> ExprId {
        let numerator = arena.int(numerator).unwrap();
        let denominator = arena.int(denominator).unwrap();
        let minus_one = arena.int(-1).unwrap();
        let reciprocal = arena.power(denominator, minus_one).unwrap();
        arena.times(&[numerator, reciprocal]).unwrap()
    }

    fn run(arena: &mut Arena, expression: ExprId) -> Rewritten {
        rewrite(arena, expression, RULES, &Assumptions::new(), DEFAULT_BUDGET).unwrap()
    }

    #[test]
    fn the_exit_criterion_of_e0() {
        let mut arena = Arena::new();
        let two_thirds = fraction(&mut arena, 2, 3);
        let one_sixth = fraction(&mut arena, 1, 6);
        let sum = arena.plus(&[two_thirds, one_sixth]).unwrap();

        let outcome = run(&mut arena, sum);

        assert_eq!(
            rational_value(&arena, outcome.expression),
            Some(Rational::new(BigInt::from(5), BigInt::from(6)))
        );
        // Dos pasos, no tres: 5/6 ya es irreducible, así que "simplificamos la
        // fracción" no tiene nada que hacer y no se inventa un paso.
        assert_eq!(
            outcome.steps.iter().map(|step| step.rule).collect::<Vec<_>>(),
            vec!["rational-common-denominator", "rational-add-numerators"]
        );
    }

    #[test]
    fn a_reducible_result_gets_its_third_step() {
        let mut arena = Arena::new();
        let one_quarter = fraction(&mut arena, 1, 4);
        let another_quarter = fraction(&mut arena, 1, 4);
        let sum = arena.plus(&[one_quarter, another_quarter]).unwrap();

        let outcome = run(&mut arena, sum);

        assert_eq!(
            rational_value(&arena, outcome.expression),
            Some(Rational::new(BigInt::from(1), BigInt::from(2)))
        );
        assert_eq!(
            outcome.steps.last().map(|step| step.rule),
            Some("rational-reduce")
        );
    }

    #[test]
    fn the_first_step_of_the_exit_criterion_is_the_common_denominator() {
        let mut arena = Arena::new();
        let one_sixth = fraction(&mut arena, 1, 6);
        let two_thirds = fraction(&mut arena, 2, 3);
        let sum = arena.plus(&[one_sixth, two_thirds]).unwrap();

        let outcome = run(&mut arena, sum);
        let first = outcome.steps.first().unwrap();

        assert_eq!(first.rule, "rational-common-denominator");
        // Su resultado sigue siendo una suma: puso denominador común, no sumó.
        assert_eq!(as_fraction(&arena, first.after), None);
    }

    #[test]
    fn integer_arithmetic_folds_without_polluting_the_trace() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let three = arena.int(3).unwrap();
        let sum = arena.plus(&[two, three]).unwrap();

        let outcome = run(&mut arena, sum);

        assert_eq!(
            rational_value(&arena, outcome.expression),
            Some(Rational::from(BigInt::from(5)))
        );
        // La regla de plegado es #[silent]: hizo el trabajo y no dejó paso.
        assert!(outcome.steps.is_empty());
    }

    #[test]
    fn a_sum_that_cancels_reaches_an_integer() {
        let mut arena = Arena::new();
        let third = fraction(&mut arena, 1, 3);
        let two_thirds = fraction(&mut arena, 2, 3);
        let sum = arena.plus(&[third, two_thirds]).unwrap();

        let outcome = run(&mut arena, sum);

        assert_eq!(
            rational_value(&arena, outcome.expression),
            Some(Rational::from(BigInt::one()))
        );
    }

    #[test]
    fn symbolic_terms_are_left_alone_instead_of_guessed() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let half = fraction(&mut arena, 1, 2);
        let sum = arena.plus(&[x, half]).unwrap();

        let outcome = run(&mut arena, sum);

        // Ninguna regla de E0 sabe sumar x con 1/2, así que no se toca.
        assert!(outcome.steps.is_empty());
        assert_eq!(rational_value(&arena, outcome.expression), None);
    }

    #[test]
    fn division_by_zero_is_detected_not_computed() {
        let mut arena = Arena::new();
        let over_zero = fraction(&mut arena, 2, 0);

        assert_eq!(
            rewrite(&mut arena, over_zero, RULES, &Assumptions::new(), DEFAULT_BUDGET),
            Err(RuleError::DivisionByZero)
        );
        assert_eq!(as_fraction(&arena, over_zero), None);
    }

    /// El denominador no era cero al entrar: lo fue al plegar `1 - 1`. Mirar sólo
    /// la entrada devolvía `\frac{2}{0}` como si fuera una respuesta.
    #[test]
    fn a_denominator_that_only_becomes_zero_while_rewriting_is_caught_too() {
        let mut arena = Arena::new();
        let one = arena.int(1).unwrap();
        let minus_one = arena.int(-1).unwrap();
        let zero_sum = arena.plus(&[one, minus_one]).unwrap();
        let reciprocal = arena.power(zero_sum, minus_one).unwrap();
        let two = arena.int(2).unwrap();
        let expression = arena.times(&[two, reciprocal]).unwrap();

        assert_eq!(
            rewrite(&mut arena, expression, RULES, &Assumptions::new(), DEFAULT_BUDGET),
            Err(RuleError::DivisionByZero)
        );
    }

    /// Un exponente simbólico no autoriza a afirmar nada: `0^{n}` puede estar
    /// perfectamente definido, así que no se reporta un error que no consta.
    #[test]
    fn a_symbolic_exponent_over_zero_is_not_called_a_division_by_zero() {
        let mut arena = Arena::new();
        let zero = arena.int(0).unwrap();
        let n = arena.sym("n").unwrap();
        let power = arena.power(zero, n).unwrap();

        assert!(rewrite(&mut arena, power, RULES, &Assumptions::new(), DEFAULT_BUDGET).is_ok());
    }

    /// La deuda que E0 dejó anotada: el valor era correcto y la traza no lo
    /// explicaba, porque el trabajo lo hacía una regla silenciosa.
    #[test]
    fn multiplying_fractions_now_says_what_it_did() {
        let mut arena = Arena::new();
        let half = fraction(&mut arena, 1, 2);
        let two_thirds = fraction(&mut arena, 2, 3);
        let product = arena.times(&[half, two_thirds]).unwrap();

        let outcome = run(&mut arena, product);

        assert_eq!(
            rational_value(&arena, outcome.expression),
            Some(Rational::new(BigInt::one(), BigInt::from(3)))
        );
        // Dos pasos y no uno: multiplicar y simplificar son cosas distintas, y
        // cancelar en cruz dentro de la multiplicación escondería la segunda.
        assert_eq!(
            outcome.steps.iter().map(|step| step.rule).collect::<Vec<_>>(),
            vec!["rational-multiply", "rational-reduce"]
        );
    }

    /// Un producto de enteros no es «multiplicar fracciones»: lo pliega la regla
    /// silenciosa y no ensucia la traza.
    #[test]
    fn multiplying_plain_integers_leaves_no_step_behind() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let three = arena.int(3).unwrap();
        let product = arena.times(&[two, three]).unwrap();

        let outcome = run(&mut arena, product);

        assert!(outcome.steps.is_empty());
        assert_eq!(
            rational_value(&arena, outcome.expression),
            Some(Rational::from(BigInt::from(6)))
        );
    }

    #[test]
    fn dividing_by_a_fraction_multiplies_by_its_reciprocal() {
        let mut arena = Arena::new();
        let seven_twelfths = fraction(&mut arena, 7, 12);
        let fourteen_ninths = fraction(&mut arena, 14, 9);
        let minus_one = arena.int(-1).unwrap();
        let reciprocal = arena.power(fourteen_ninths, minus_one).unwrap();
        let quotient = arena.times(&[seven_twelfths, reciprocal]).unwrap();

        let outcome = run(&mut arena, quotient);

        assert_eq!(
            rational_value(&arena, outcome.expression),
            Some(Rational::new(BigInt::from(3), BigInt::from(8)))
        );
        assert_eq!(
            outcome.steps.iter().map(|step| step.rule).collect::<Vec<_>>(),
            vec![
                "rational-reciprocal",
                "rational-multiply",
                "rational-reduce"
            ]
        );
    }

    /// El recíproco de un entero ya es la forma normal de un denominador. Darle
    /// la vuelta lo devolvería a su sitio, y el reescritor ciclaría.
    #[test]
    fn the_reciprocal_of_an_integer_is_left_where_it_is() {
        let mut arena = Arena::new();
        let one_fifth = fraction(&mut arena, 1, 5);

        let outcome = run(&mut arena, one_fifth);

        assert!(outcome.steps.is_empty());
    }

    /// Invertir `\frac{0}{9}` no es un paso: es una división entre cero, y la
    /// base no es el literal `0` sino un producto que vale cero.
    #[test]
    fn a_structurally_zero_denominator_is_caught_and_not_inverted() {
        let mut arena = Arena::new();
        let zero_ninths = fraction(&mut arena, 0, 9);
        let minus_one = arena.int(-1).unwrap();
        let reciprocal = arena.power(zero_ninths, minus_one).unwrap();

        assert_eq!(
            rewrite(
                &mut arena,
                reciprocal,
                RULES,
                &Assumptions::new(),
                DEFAULT_BUDGET
            ),
            Err(RuleError::DivisionByZero)
        );
    }

    /// La regla del recíproco, **en aislamiento**: sin la certeza de que el
    /// numerador no es cero, no invierte.
    ///
    /// Se prueba llamando a la regla y no al reescritor porque el reescritor
    /// atrapa `\frac{0}{9}` antes, con `has_division_by_zero`. Que hoy nadie
    /// llegue hasta aquí no hace prescindible la comprobación: una regla que
    /// sólo es correcta porque otro comprueba primero deja de serlo en cuanto
    /// cambia el orden, y ése es el fallo que no se ve venir.
    #[test]
    fn the_reciprocal_rule_refuses_to_invert_what_might_be_zero() {
        let mut arena = Arena::new();
        let zero_ninths = fraction(&mut arena, 0, 9);
        let minus_one = arena.int(-1).unwrap();
        let reciprocal = arena.power(zero_ninths, minus_one).unwrap();
        let canonical = arena.canonicalize(reciprocal).unwrap().id();

        let rule = RULES
            .iter()
            .find(|rule| rule.id == "rational-reciprocal")
            .unwrap();

        assert_eq!(
            (rule.apply)(&mut arena, canonical, &Assumptions::new()).unwrap(),
            None
        );
    }

    #[test]
    fn every_rule_carries_the_metadata_the_macro_demands() {
        for rule in RULES {
            assert!(!rule.id.is_empty(), "regla sin id");
            assert!(!rule.title.is_empty(), "«{}» sin título", rule.id);
            assert!(!rule.step_template.is_empty(), "«{}» sin paso", rule.id);
        }
    }

    #[test]
    fn rule_ids_are_unique() {
        let mut ids: Vec<&str> = RULES.iter().map(|rule| rule.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();

        assert_eq!(ids.len(), count, "hay ids de regla repetidos");
    }
}
