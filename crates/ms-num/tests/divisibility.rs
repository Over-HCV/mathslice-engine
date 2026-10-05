//! Las propiedades de la divisibilidad, comprobadas sobre entradas generadas.
//!
//! Un caso fija **un** comportamiento; una propiedad cubre el espacio
//! (05-testing.md §10). Aquí están las de la fase 3: que la factorización
//! multiplica al número, que sus factores son primos, y que los divisores son
//! todos los divisores y nada más.
//!
//! La factorización es el sitio donde un error no se nota: `12 = 2 · 3` se lee
//! plausible, y sólo el producto lo delata.

// Ver la nota de ms-rules/tests/laws.rs: los `allow-*-in-tests` de clippy.toml
// no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_num::{Integer, factorize, is_even, is_prime};
use proptest::prelude::*;

/// El producto de los factores por lo que quedó sin factorizar.
fn reconstruct(value: &Integer) -> Integer {
    let factorization = factorize(value).expect("hay factorización");
    let mut product = Integer::from(1_u8);

    for (prime, exponent) in factorization.factors() {
        for _ in 0..*exponent {
            product *= prime;
        }
    }
    if let Some(remaining) = factorization.remaining() {
        product *= remaining;
    }
    product
}

/// Criba de Eratóstenes hasta `limit`, para contrastar con Miller–Rabin: dos
/// caminos independientes hasta la misma respuesta, que es lo que hace que la
/// comprobación signifique algo.
fn sieve(limit: usize) -> Vec<bool> {
    let mut is_prime = vec![true; limit + 1];
    is_prime[0] = false;
    if limit >= 1 {
        is_prime[1] = false;
    }
    let mut candidate = 2;
    while candidate * candidate <= limit {
        if is_prime[candidate] {
            let mut multiple = candidate * candidate;
            while multiple <= limit {
                is_prime[multiple] = false;
                multiple += candidate;
            }
        }
        candidate += 1;
    }
    is_prime
}

proptest! {
    /// La propiedad que sostiene todo lo demás: si el producto no vuelve a dar
    /// el número, la factorización es decorado.
    #[test]
    fn the_factors_multiply_back_to_the_magnitude(value in 2_i64..=1_000_000_000_000) {
        prop_assert_eq!(reconstruct(&Integer::from(value)), Integer::from(value));
    }

    /// Un número negativo factoriza por su magnitud, y su signo es otro hecho.
    #[test]
    fn a_negative_number_reconstructs_to_its_magnitude(value in 2_i64..=1_000_000_000) {
        prop_assert_eq!(reconstruct(&Integer::from(-value)), Integer::from(value));
    }

    #[test]
    fn every_reported_factor_is_prime(value in 2_i64..=1_000_000_000_000) {
        let factorization = factorize(&Integer::from(value)).expect("hay factorización");

        for (prime, exponent) in factorization.factors() {
            prop_assert_eq!(is_prime(prime), Some(true));
            prop_assert!(*exponent >= 1);
        }
    }

    /// Los primos salen crecientes, sin repetirse: `2^{2} \cdot 3` y no
    /// `2 \cdot 3 \cdot 2`.
    #[test]
    fn the_factors_come_out_strictly_increasing(value in 2_i64..=1_000_000_000_000) {
        let factorization = factorize(&Integer::from(value)).expect("hay factorización");

        for window in factorization.factors().windows(2) {
            prop_assert!(window[0].0 < window[1].0);
        }
    }

    /// Todo divisor divide, la lista es creciente y su longitud es el recuento.
    /// Cualquiera de las tres por separado se puede cumplir con una lista mal.
    #[test]
    fn the_divisors_are_all_the_divisors_and_nothing_else(value in 2_i64..=100_000) {
        let number = Integer::from(value);
        let factorization = factorize(&number).expect("hay factorización");
        let Some(divisors) = factorization.divisors() else {
            return Ok(());
        };

        prop_assert_eq!(
            u64::try_from(divisors.len()).expect("cabe"),
            factorization.divisor_count().expect("completa")
        );
        for divisor in &divisors {
            prop_assert_eq!(&number % divisor, Integer::from(0_u8));
        }
        for window in divisors.windows(2) {
            prop_assert!(window[0] < window[1]);
        }
        prop_assert_eq!(divisors.first(), Some(&Integer::from(1_u8)));
        prop_assert_eq!(divisors.last(), Some(&number));

        // Y ninguno se quedó fuera: la fuerza bruta es lenta y es la definición.
        let counted = (1..=value).filter(|candidate| value % candidate == 0).count();
        prop_assert_eq!(divisors.len(), counted);
    }

    /// Miller–Rabin contra la criba, que es el algoritmo que nadie duda.
    #[test]
    fn primality_agrees_with_a_sieve(value in 0_usize..=10_000) {
        let sieve = sieve(10_000);
        let expected = sieve[value];

        prop_assert_eq!(is_prime(&Integer::from(value)), Some(expected));
    }

    #[test]
    fn parity_agrees_with_the_remainder(value in i64::MIN..=i64::MAX) {
        prop_assert_eq!(is_even(&Integer::from(value)), value % 2 == 0);
    }
}
