//! El orden de las operaciones y los dos frenos del reescritor, contra el rule
//! set completo.
//!
//! Las propiedades de `laws.rs` cubren una ley cada una; esto cubre lo que sólo
//! aparece cuando **todas** las reglas corren sobre la misma expresión: que
//! PEMDAS se respeta, que el presupuesto no se agota en una entrada legítima
//! larga, y que la detección de ciclos no dispara en falso.
//!
//! Se comprobó por última vez en la fase 2, con 9 reglas. La fase 4 añadió tres
//! —entre ellas `round-to-digits`, que **pierde información a propósito**— y una
//! regla que no conserva el valor es exactamente la que puede hacer que el
//! reescritor vuelva sobre sus pasos.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_assume::Assumptions;
use ms_core::{Arena, Builtin, ExprId, Head};
use ms_num::Rational;
use ms_rules::{DEFAULT_BUDGET, RULES, RuleError, rational_value, rewrite};
use num_bigint::BigInt;

/// Una fracción tal y como sale del parser: `Times(n, Power(d, -1))`.
fn fraction(arena: &mut Arena, numerator: i64, denominator: i64) -> ExprId {
    let numerator = arena.int(numerator).expect("arena");
    let denominator = arena.int(denominator).expect("arena");
    let minus_one = arena.int(-1).expect("arena");
    let reciprocal = arena.power(denominator, minus_one).expect("arena");
    arena.times(&[numerator, reciprocal]).expect("arena")
}

fn exact(numerator: i64, denominator: i64) -> Rational {
    Rational::new(BigInt::from(numerator), BigInt::from(denominator))
}

/// Reescribe con el rule set completo y el presupuesto de producción.
fn solve(arena: &mut Arena, expression: ExprId) -> Result<(Option<Rational>, usize), RuleError> {
    let outcome = rewrite(
        arena,
        expression,
        RULES,
        &Assumptions::new(),
        DEFAULT_BUDGET,
    )?;
    Ok((
        rational_value(arena, outcome.expression),
        outcome.steps.len(),
    ))
}

/// `\frac{1}{2} + \frac{1}{3} \cdot \frac{3}{4} - 2^{3} \cdot \frac{1}{4}`.
///
/// Las cuatro precedencias en una sola expresión: la potencia antes que el
/// producto, el producto antes que la suma, y una resta que en el IR es un
/// sumando negativo. El valor correcto es `\frac{1}{2} + \frac{1}{4} - 2`, que es
/// `-\frac{5}{4}`; cualquier orden equivocado da otro número.
#[test]
fn the_order_of_operations_holds_across_the_whole_rule_set() {
    let mut arena = Arena::new();

    let half = fraction(&mut arena, 1, 2);
    let product = {
        let third = fraction(&mut arena, 1, 3);
        let three_quarters = fraction(&mut arena, 3, 4);
        arena.times(&[third, three_quarters]).expect("arena")
    };
    let power_term = {
        let two = arena.int(2).expect("arena");
        let three = arena.int(3).expect("arena");
        let power = arena.power(two, three).expect("arena");
        let quarter = fraction(&mut arena, -1, 4);
        arena.times(&[power, quarter]).expect("arena")
    };

    let expression = arena
        .plus(&[half, product, power_term])
        .expect("arena");

    let (value, _) = solve(&mut arena, expression).expect("el reescritor termina");
    assert_eq!(value, Some(exact(-5, 4)));
}

/// Una expresión larga y legítima no agota el presupuesto.
///
/// `DEFAULT_BUDGET` cuenta **iteraciones**, y lo que las gasta es la profundidad,
/// no la anchura: las reglas son n-arias, así que una suma de treinta fracciones
/// sueltas se lleva un denominador común y una suma de numeradores para todas
/// —tres iteraciones en total, medido—. Lo que cuesta es que cada sumando tenga
/// trabajo propio.
///
/// Treinta productos de fracciones, cada uno con su multiplicación y su
/// reducción, cuestan **120 iteraciones**: unas cuatro por término. De ahí sale
/// el margen real del presupuesto, que son ~128 términos de esta forma. Si esto
/// se pone en rojo, el presupuesto dejó de ser generoso y el número se vuelve a
/// decidir con una medida delante, no subiéndolo a ojo.
#[test]
fn a_long_but_legitimate_expression_finishes_well_inside_the_budget() {
    /// Medido: 120 iteraciones con 30 términos. El margen es el que dice el
    /// cociente contra `DEFAULT_BUDGET`, y está aquí para que se lea.
    const MEASURED_ITERATIONS: u32 = 120;
    const TERMS: i64 = 30;

    let mut arena = Arena::new();
    let mut summands = Vec::new();
    for k in 1..=TERMS {
        let left = fraction(&mut arena, 1, k);
        let right = fraction(&mut arena, k, k + 1);
        summands.push(arena.times(&[left, right]).expect("arena"));
    }
    let expression = arena.plus(&summands).expect("arena");

    let (value, _) = solve(&mut arena, expression).expect("el presupuesto alcanza");

    // Cada término es `\frac{1}{k} \cdot \frac{k}{k+1} = \frac{1}{k+1}`, así que
    // la suma es H₃₁ - 1. Calculado aparte: si el motor lo cambia, esto lo dice.
    assert_eq!(value, Some(exact(218_572_480_850_557, 72_201_776_446_800)));

    // El mismo cálculo con el presupuesto justo por debajo de lo medido tiene que
    // fallar: sin esto, la cifra de arriba es un comentario que nadie comprueba.
    let mut arena = Arena::new();
    let mut summands = Vec::new();
    for k in 1..=TERMS {
        let left = fraction(&mut arena, 1, k);
        let right = fraction(&mut arena, k, k + 1);
        summands.push(arena.times(&[left, right]).expect("arena"));
    }
    let expression = arena.plus(&summands).expect("arena");
    let tight = rewrite(
        &mut arena,
        expression,
        RULES,
        &Assumptions::new(),
        MEASURED_ITERATIONS - 1,
    );
    assert!(matches!(tight, Err(RuleError::BudgetExhausted { .. })));
    assert!(MEASURED_ITERATIONS * 4 < DEFAULT_BUDGET);
}

/// El presupuesto sigue reportándose como dato con el rule set completo.
///
/// Un tope que se agota tiene que salir como `BudgetExhausted`, nunca como un
/// cuelgue ni como un resultado a medias que nadie sabe leer.
#[test]
fn exhausting_the_budget_is_reported_instead_of_hanging() {
    let mut arena = Arena::new();
    let a = fraction(&mut arena, 1, 2);
    let b = fraction(&mut arena, 1, 3);
    let c = fraction(&mut arena, 1, 5);
    let expression = arena.plus(&[a, b, c]).expect("arena");

    let outcome = rewrite(&mut arena, expression, RULES, &Assumptions::new(), 2);

    assert!(matches!(
        outcome,
        Err(RuleError::BudgetExhausted { iterations: 2 })
    ));
}

/// `1 + \operatorname{redondear}(3.456, 2)` no dispara la detección de ciclos.
///
/// `round-to-digits` es la única regla del motor que **no conserva el valor**, y
/// la detección de ciclos es por id de expresión: si el redondeo devolviera a la
/// suma un estado por el que ya pasó, el reescritor lo reportaría como ciclo y el
/// estudiante vería un error donde hay una respuesta.
#[test]
fn rounding_inside_a_sum_does_not_trip_the_cycle_detector() {
    let mut arena = Arena::new();

    let value = fraction(&mut arena, 3456, 1000);
    let digits = arena.int(2).expect("arena");
    let rounded = arena
        .app(Head::Fn(Builtin::Round), &[value, digits])
        .expect("arena");
    let one = arena.int(1).expect("arena");
    let expression = arena.plus(&[one, rounded]).expect("arena");

    let (value, _) = solve(&mut arena, expression).expect("no hay ciclo");

    // 3.456 a dos cifras es 3.46, y uno más eso es 4.46 = 223/50.
    assert_eq!(value, Some(exact(223, 50)));
}

/// Redondear dos veces son dos redondeos, y el segundo no deshace al primero.
///
/// `redondear(redondear(3.456, 2), 1)` es `3.5`, no `3.4`: la anidación no se
/// aplana (`Head::is_flat` lo dice para `Fn`), y la regla dispara de dentro
/// afuera. Es el caso en el que una regla que pierde información se aplica sobre
/// su propia salida, que es donde un ciclo aparecería si fuera a aparecer.
#[test]
fn rounding_a_rounded_value_is_two_roundings_and_not_a_cycle() {
    let mut arena = Arena::new();

    let value = fraction(&mut arena, 3456, 1000);
    let two = arena.int(2).expect("arena");
    let one = arena.int(1).expect("arena");
    let inner = arena
        .app(Head::Fn(Builtin::Round), &[value, two])
        .expect("arena");
    let outer = arena
        .app(Head::Fn(Builtin::Round), &[inner, one])
        .expect("arena");

    let (value, _) = solve(&mut arena, outer).expect("no hay ciclo");

    assert_eq!(value, Some(exact(35, 10)));
}
