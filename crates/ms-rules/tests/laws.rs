//! Las leyes de la aritmética racional, comprobadas sobre entradas generadas.
//!
//! Un caso de corpus fija **un** comportamiento; una propiedad cubre el espacio.
//! Cada capacidad de E1 trae al menos una (05-testing.md §10), y aquí están las
//! que corresponden a las reglas de la fase 2: conmutatividad y asociatividad de
//! la suma, `\frac{a}{b} \cdot \frac{b}{a} = 1`, y `(a^{m})^{n} = a^{mn}`.
//!
//! Lo que se comprueba es el **valor**, no la derivación: dos caminos distintos
//! hasta el mismo número son correctos, y exigir la misma traza convertiría cada
//! propiedad en un test de regresión disfrazado.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_assume::Assumptions;
use ms_core::{Arena, ExprId};
use ms_num::Rational;
use ms_rules::{DEFAULT_BUDGET, RULES, rational_value, rewrite};
use num_bigint::BigInt;
use proptest::prelude::*;

/// Una fracción tal y como sale del parser: `Times(n, Power(d, -1))`.
fn fraction(arena: &mut Arena, numerator: i64, denominator: i64) -> ExprId {
    let numerator = arena.int(numerator).expect("arena");
    let denominator = arena.int(denominator).expect("arena");
    let minus_one = arena.int(-1).expect("arena");
    let reciprocal = arena.power(denominator, minus_one).expect("arena");
    arena.times(&[numerator, reciprocal]).expect("arena")
}

/// El valor exacto al que el motor lleva una expresión, o `None` si no llega a
/// un número. Un fallo del reescritor se propaga como `None` a propósito: la
/// propiedad compara dos lados, y «los dos fallan igual» no es una coincidencia
/// que valga como prueba, así que las propiedades exigen `Some` explícitamente.
fn solve(arena: &mut Arena, expression: ExprId) -> Option<Rational> {
    let outcome = rewrite(
        arena,
        expression,
        RULES,
        &Assumptions::new(),
        DEFAULT_BUDGET,
    )
    .ok()?;
    rational_value(arena, outcome.expression)
}

/// Denominadores no nulos: `\frac{1}{0}` no denota nada y no tiene ley que
/// cumplir. Numeradores con signo, que es donde vive la mitad de los errores.
fn numerator() -> impl Strategy<Value = i64> {
    -50i64..50
}

fn denominator() -> impl Strategy<Value = i64> {
    prop_oneof![(-50i64..-1), (1i64..50)]
}

proptest! {
    /// `a + b = b + a`. El orden canónico debería hacerla trivial, y eso es
    /// precisamente lo que comprueba: que el orden canónico funciona.
    #[test]
    fn adding_rationals_commutes(
        a in numerator(), b in denominator(), c in numerator(), d in denominator()
    ) {
        let mut arena = Arena::new();
        let (left, right) = (fraction(&mut arena, a, b), fraction(&mut arena, c, d));

        let forwards = arena.plus(&[left, right]).expect("arena");
        let backwards = arena.plus(&[right, left]).expect("arena");

        let forwards = solve(&mut arena, forwards);
        let backwards = solve(&mut arena, backwards);

        prop_assert!(forwards.is_some());
        prop_assert_eq!(forwards, backwards);
    }

    /// `(a + b) + c = a + (b + c)`. Con tres sumandos entra en juego el mcm de
    /// tres denominadores, que es donde el denominador común se puede torcer.
    #[test]
    fn adding_rationals_associates(
        a in numerator(), b in denominator(),
        c in numerator(), d in denominator(),
        e in numerator(), f in denominator()
    ) {
        let mut arena = Arena::new();
        let first = fraction(&mut arena, a, b);
        let second = fraction(&mut arena, c, d);
        let third = fraction(&mut arena, e, f);

        let left_first = arena.plus(&[first, second]).expect("arena");
        let left_first = arena.plus(&[left_first, third]).expect("arena");

        let right_first = arena.plus(&[second, third]).expect("arena");
        let right_first = arena.plus(&[first, right_first]).expect("arena");

        let left_first = solve(&mut arena, left_first);
        let right_first = solve(&mut arena, right_first);

        prop_assert!(left_first.is_some());
        prop_assert_eq!(left_first, right_first);
    }

    /// `\frac{a}{b} \cdot \frac{b}{a} = 1` para `a` y `b` no nulos. Es la
    /// propiedad de la multiplicación **y** de la reducción a la vez: el producto
    /// da `\frac{ab}{ab}`, y sin simplificar eso no es 1.
    #[test]
    fn a_rational_times_its_reciprocal_is_one(
        a in denominator(), b in denominator()
    ) {
        let mut arena = Arena::new();
        let value = fraction(&mut arena, a, b);
        let inverse = fraction(&mut arena, b, a);
        let product = arena.times(&[value, inverse]).expect("arena");

        prop_assert_eq!(solve(&mut arena, product), Some(Rational::from(BigInt::from(1))));
    }

    /// `\frac{a}{b} \div \frac{c}{d} = \frac{a}{b} \cdot \frac{d}{c}`: dividir y
    /// multiplicar por el recíproco llegan al mismo número.
    #[test]
    fn dividing_agrees_with_multiplying_by_the_reciprocal(
        a in numerator(), b in denominator(),
        c in denominator(), d in denominator()
    ) {
        let mut arena = Arena::new();
        let dividend = fraction(&mut arena, a, b);
        let divisor = fraction(&mut arena, c, d);
        let minus_one = arena.int(-1).expect("arena");

        let reciprocal = arena.power(divisor, minus_one).expect("arena");
        let quotient = arena.times(&[dividend, reciprocal]).expect("arena");

        let flipped = fraction(&mut arena, d, c);
        let product = arena.times(&[dividend, flipped]).expect("arena");

        let quotient = solve(&mut arena, quotient);
        prop_assert!(quotient.is_some());
        prop_assert_eq!(quotient, solve(&mut arena, product));
    }

    /// `(a^{m})^{n} = a^{m \cdot n}`. Exponentes pequeños: la ley es sobre la
    /// potencia, no sobre el tope de magnitud, que tiene su propio caso.
    #[test]
    fn powers_of_powers_multiply_their_exponents(
        base in -6i64..6, m in 0i64..5, n in 0i64..5
    ) {
        // `0^{0}` no vale 1, vale nada. Es una forma indeterminada y queda fuera
        // de la ley, no dentro con una excepción escrita a mano.
        prop_assume!(base != 0 || (m != 0 && n != 0));

        let mut arena = Arena::new();
        let base_id = arena.int(base).expect("arena");
        let m_id = arena.int(m).expect("arena");
        let n_id = arena.int(n).expect("arena");

        let inner = arena.power(base_id, m_id).expect("arena");
        let nested = arena.power(inner, n_id).expect("arena");

        let product = arena.int(m * n).expect("arena");
        let flattened = arena.power(base_id, product).expect("arena");

        let nested = solve(&mut arena, nested);
        prop_assert!(nested.is_some());
        prop_assert_eq!(nested, solve(&mut arena, flattened));
    }

    /// Un producto con un factor nulo vale cero, estén donde estén los demás.
    #[test]
    fn zero_absorbs_any_product(a in numerator(), b in denominator()) {
        let mut arena = Arena::new();
        let value = fraction(&mut arena, a, b);
        let zero = arena.int(0).expect("arena");
        let symbol = arena.sym("x").expect("arena");
        let product = arena.times(&[value, zero, symbol]).expect("arena");

        prop_assert_eq!(solve(&mut arena, product), Some(Rational::from(BigInt::from(0))));
    }
}
