//! Divisibilidad: primalidad, factorización prima y divisores.
//!
//! **Factorizar no es una operación acotada.** Un semiprimo de 60 dígitos no
//! cicla y no desborda: calcula, durante horas, y se lleva la pestaña del
//! usuario. Por eso todo aquí tiene tope, y el tope se comprueba **antes** de
//! calcular, igual que en [`checked_pow`](crate::checked_pow).
//!
//! Cuando el tope no alcanza, la respuesta no es un error ni una mentira: es una
//! factorización parcial con su resto a la vista. «`n = 2 · 3 · r`, y `r` no lo
//! sé factorizar» es un dato verdadero; redondearlo a «no se puede» pierde
//! información que el estudiante sí puede usar.
//!
//! El cómputo va sobre `u64`, no sobre bignum: la división por prueba es el
//! único bucle caliente del crate y un `BigInt` por iteración lo hace diez veces
//! más lento sin ganar un solo número — por encima de 2⁶⁴ el tope ya se negó.

use num_integer::Integer as _;
use num_traits::ToPrimitive;

use crate::Integer;

/// Magnitud máxima con hechos de divisibilidad, en bits.
///
/// Por encima se devuelve `None` en vez de un resultado a medias: un entero de
/// 200 bits puede ser un primo cuya factorización cuesta más que toda la sesión.
/// Signo y paridad siguen siendo baratos para cualquier magnitud y no pasan por
/// aquí.
pub const MAX_FACTOR_BITS: u64 = 64;

/// Cuántos divisores se enumeran como mucho.
///
/// `963761198400` tiene 6720 divisores. Enumerarlos es correcto y mandarlos al
/// navegador es una lista que nadie lee: por encima del tope se informa del
/// recuento, que es el hecho, y no de la lista, que es el volcado.
pub const MAX_DIVISORS: u64 = 1024;

/// Hasta dónde llega la división por prueba.
///
/// Con 10⁶ se factoriza por completo todo `n ≤ 10¹²`, y todo `n` cuyo segundo
/// factor mayor no pase de 10⁶. Cuesta unos milisegundos en el peor caso.
const TRIAL_DIVISION_LIMIT: u64 = 1_000_000;

/// Los saltos entre los candidatos coprimos con 2·3·5, empezando en 7. Salta
/// las cuatro quintas partes de los candidatos sin comprobar nada.
const WHEEL: [u64; 8] = [4, 2, 4, 2, 4, 6, 2, 6];

/// Bases de Miller–Rabin **deterministas** para todo `n < 3,3·10²⁴`, que cubre
/// de sobra el rango de 64 bits. Determinista importa: «probablemente primo» es
/// un hecho distinto de «primo», y el motor no puede emitir el segundo cuando
/// sólo sabe el primero.
const MILLER_RABIN_BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];

/// La factorización prima de un entero, completa o con su resto a la vista.
///
/// Los campos son privados y el único constructor es [`factorize`]: una
/// factorización que alguien pudiera montar a mano podría afirmar «completa»
/// sobre factores que no multiplican al número.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Factorization {
    factors: Vec<(Integer, u32)>,
    remaining: Option<Integer>,
}

impl Factorization {
    /// Los primos y sus exponentes, en orden creciente de primo.
    pub fn factors(&self) -> &[(Integer, u32)] {
        &self.factors
    }

    /// El resto compuesto que el presupuesto no alcanzó a factorizar. `None`
    /// significa que el producto de los factores **es** el número.
    pub fn remaining(&self) -> Option<&Integer> {
        self.remaining.as_ref()
    }

    pub fn is_complete(&self) -> bool {
        self.remaining.is_none()
    }

    /// Cuántos divisores positivos tiene el número: `∏(eᵢ+1)`.
    ///
    /// `None` si la factorización está incompleta. Contar divisores sobre una
    /// factorización parcial daría un número menor que el real, y un recuento
    /// equivocado es peor que ninguno.
    pub fn divisor_count(&self) -> Option<u64> {
        if !self.is_complete() {
            return None;
        }
        let mut count: u64 = 1;
        for (_, exponent) in &self.factors {
            count = count.checked_mul(u64::from(*exponent).checked_add(1)?)?;
        }
        Some(count)
    }

    /// Los divisores positivos, en orden creciente.
    ///
    /// `None` si la factorización está incompleta o si hay más de
    /// [`MAX_DIVISORS`]; en ese caso [`divisor_count`](Self::divisor_count)
    /// sigue respondiendo.
    pub fn divisors(&self) -> Option<Vec<Integer>> {
        if self.divisor_count()? > MAX_DIVISORS {
            return None;
        }

        let mut divisors = vec![Integer::from(1_u8)];
        for (prime, exponent) in &self.factors {
            let mut extended = Vec::new();
            let mut power = Integer::from(1_u8);
            for _ in 0..=*exponent {
                extended.extend(divisors.iter().map(|divisor| divisor * &power));
                power *= prime;
            }
            divisors = extended;
        }

        divisors.sort();
        Some(divisors)
    }
}

/// La factorización prima de `|value|`, o `None` si no procede.
///
/// `None` cuando `|value| < 2` —el 0 no tiene factorización y el 1 es el
/// producto vacío, que no es un hecho que enseñar— o cuando `|value|` pasa de
/// [`MAX_FACTOR_BITS`].
///
/// **Es sobre el valor absoluto**: `-12` factoriza como `12`, y su signo es un
/// hecho aparte. Meter el `-1` entre los factores obligaría a llamarlo primo,
/// que no lo es.
pub fn factorize(value: &Integer) -> Option<Factorization> {
    let magnitude = within_budget(value)?;
    if magnitude < 2 {
        return None;
    }

    let (factors, remaining) = factorize_u64(magnitude);
    Some(Factorization {
        factors: factors
            .into_iter()
            .map(|(prime, exponent)| (Integer::from(prime), exponent))
            .collect(),
        remaining: (remaining > 1).then(|| Integer::from(remaining)),
    })
}

/// ¿Es `|value|` primo? `None` si pasa de [`MAX_FACTOR_BITS`].
///
/// Sobre el valor absoluto, por lo mismo que [`factorize`]. Ni 0 ni ±1 son
/// primos: son el cero y las unidades del anillo.
pub fn is_prime(value: &Integer) -> Option<bool> {
    Some(is_prime_u64(within_budget(value)?))
}

/// ¿Es par? Barato para cualquier magnitud, así que no pasa por el tope.
pub fn is_even(value: &Integer) -> bool {
    value.is_even()
}

/// La magnitud en `u64`, o `None` si el tope se niega. El único sitio donde se
/// decide qué es demasiado grande.
fn within_budget(value: &Integer) -> Option<u64> {
    let magnitude = value.magnitude();
    if magnitude.bits() > MAX_FACTOR_BITS {
        return None;
    }
    magnitude.to_u64()
}

/// Devuelve los factores hallados y lo que queda sin factorizar (1 si acabó).
fn factorize_u64(value: u64) -> (Vec<(u64, u32)>, u64) {
    let mut factors: Vec<(u64, u32)> = Vec::new();
    let mut remaining = value;

    for small in [2_u64, 3, 5] {
        remaining = extract(&mut factors, remaining, small);
    }

    let mut candidate: u64 = 7;
    let mut steps = WHEEL.iter().cycle();
    while candidate <= TRIAL_DIVISION_LIMIT && remaining > 1 {
        // Si ningún divisor llegó a la raíz cuadrada, lo que queda es primo.
        if candidate.saturating_mul(candidate) > remaining {
            factors.push((remaining, 1));
            return (factors, 1);
        }

        remaining = extract(&mut factors, remaining, candidate);

        // `cycle` sobre un array no vacío no se acaba nunca; el `else` existe
        // porque el tipo lo pide, no porque pueda pasar.
        let Some(step) = steps.next() else {
            break;
        };
        candidate = candidate.saturating_add(*step);
    }

    // Se agotó el presupuesto. Lo que queda puede ser primo de todos modos, y
    // comprobarlo cuesta doce exponenciaciones modulares en vez de una raíz de
    // divisiones.
    if remaining > 1 && is_prime_u64(remaining) {
        factors.push((remaining, 1));
        return (factors, 1);
    }
    (factors, remaining)
}

/// Saca todas las potencias de `prime` de `value` y las anota.
fn extract(factors: &mut Vec<(u64, u32)>, value: u64, prime: u64) -> u64 {
    let mut remaining = value;
    let mut exponent: u32 = 0;
    while remaining >= prime && remaining.is_multiple_of(prime) {
        remaining /= prime;
        exponent = exponent.saturating_add(1);
    }
    if exponent > 0 {
        factors.push((prime, exponent));
    }
    remaining
}

/// Miller–Rabin determinista sobre el rango de 64 bits.
///
/// Toda la aritmética modular vive en `u128` y nunca vuelve a `u64`: el producto
/// de dos residuos menores que 2⁶⁴ cabe exacto en 128 bits, así que no hay
/// desbordamiento que comprobar ni conversión que pueda truncar (§9).
fn is_prime_u64(value: u64) -> bool {
    if value < 2 {
        return false;
    }
    for base in MILLER_RABIN_BASES {
        if value == base {
            return true;
        }
        if value.is_multiple_of(base) {
            return false;
        }
    }

    // value - 1 = odd_part · 2^twos
    let mut odd_part = value - 1;
    let mut twos: u32 = 0;
    while odd_part.is_multiple_of(2) {
        odd_part /= 2;
        twos = twos.saturating_add(1);
    }

    let modulus = u128::from(value);
    let minus_one = modulus - 1;
    'bases: for base in MILLER_RABIN_BASES {
        let mut witness = mod_pow(u128::from(base), odd_part, modulus);
        if witness == 1 || witness == minus_one {
            continue;
        }
        for _ in 1..twos {
            witness = witness * witness % modulus;
            if witness == minus_one {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// `base^exponent mod modulus`, con `modulus < 2⁶⁴` para que los productos
/// intermedios quepan en `u128`.
fn mod_pow(base: u128, exponent: u64, modulus: u128) -> u128 {
    let mut result: u128 = 1;
    let mut factor = base % modulus;
    let mut remaining = exponent;

    while remaining > 0 {
        if remaining % 2 == 1 {
            result = result * factor % modulus;
        }
        factor = factor * factor % modulus;
        remaining /= 2;
    }
    result
}

#[cfg(test)]
mod tests {
    use num_traits::Pow;

    use super::*;

    fn integer(value: i64) -> Integer {
        Integer::from(value)
    }

    fn primes_and_exponents(value: i64) -> Vec<(i64, u32)> {
        factorize(&integer(value))
            .unwrap()
            .factors()
            .iter()
            .map(|(prime, exponent)| (prime.to_i64().unwrap(), *exponent))
            .collect()
    }

    #[test]
    fn twelve_factors_as_the_student_would_write_it() {
        assert_eq!(primes_and_exponents(12), vec![(2, 2), (3, 1)]);
    }

    #[test]
    fn a_negative_number_factors_by_its_magnitude() {
        assert_eq!(primes_and_exponents(-12), primes_and_exponents(12));
    }

    #[test]
    fn zero_and_the_units_have_no_factorization_to_show() {
        assert_eq!(factorize(&integer(0)), None);
        assert_eq!(factorize(&integer(1)), None);
        assert_eq!(factorize(&integer(-1)), None);
    }

    #[test]
    fn a_power_of_two_is_one_factor_with_a_big_exponent() {
        assert_eq!(primes_and_exponents(1_099_511_627_776), vec![(2, 40)]);
    }

    /// El clásico del problema 3 de Project Euler: cuatro factores, ninguno
    /// pequeño, todos por debajo del presupuesto.
    #[test]
    fn a_twelve_digit_number_factors_completely() {
        assert_eq!(
            primes_and_exponents(600_851_475_143),
            vec![(71, 1), (839, 1), (1471, 1), (6857, 1)]
        );
    }

    /// El caso que justifica el tope: los dos factores pasan del límite de la
    /// división por prueba, así que sale marcado en vez de mentir o colgarse.
    #[test]
    fn a_semiprime_beyond_the_budget_reports_what_it_could_not_do() {
        let factorization = factorize(&integer(1_000_003 * 1_000_033)).unwrap();

        assert!(!factorization.is_complete());
        assert_eq!(
            factorization.remaining(),
            Some(&integer(1_000_003 * 1_000_033))
        );
        assert_eq!(factorization.divisor_count(), None);
        assert_eq!(factorization.divisors(), None);
    }

    /// Un primo mayor que el presupuesto de división por prueba: Miller–Rabin lo
    /// resuelve entero en vez de dejarlo como resto.
    #[test]
    fn a_prime_above_the_trial_division_limit_is_still_recognised() {
        let factorization = factorize(&integer(1_000_000_007)).unwrap();

        assert!(factorization.is_complete());
        assert_eq!(primes_and_exponents(1_000_000_007), vec![(1_000_000_007, 1)]);
    }

    #[test]
    fn beyond_the_magnitude_budget_nothing_is_claimed() {
        let huge = Integer::from(2).pow(100_u32);

        assert_eq!(factorize(&huge), None);
        assert_eq!(is_prime(&huge), None);
    }

    #[test]
    fn primality_is_about_the_magnitude_and_excludes_zero_and_the_units() {
        assert_eq!(is_prime(&integer(97)), Some(true));
        assert_eq!(is_prime(&integer(-97)), Some(true));
        assert_eq!(is_prime(&integer(91)), Some(false)); // 7 · 13
        assert_eq!(is_prime(&integer(1)), Some(false));
        assert_eq!(is_prime(&integer(0)), Some(false));
    }

    /// Los pseudoprimos fuertes de base 2 son exactamente lo que una sola base
    /// de Miller–Rabin deja pasar.
    #[test]
    fn strong_pseudoprimes_do_not_fool_the_deterministic_bases() {
        for pseudoprime in [2047_i64, 3_215_031_751, 3_825_123_056_546_413_051] {
            assert_eq!(is_prime(&integer(pseudoprime)), Some(false));
        }
    }

    #[test]
    fn the_divisors_of_twelve_come_out_sorted_and_complete() {
        let divisors: Vec<i64> = factorize(&integer(12))
            .unwrap()
            .divisors()
            .unwrap()
            .iter()
            .map(|divisor| divisor.to_i64().unwrap())
            .collect();

        assert_eq!(divisors, vec![1, 2, 3, 4, 6, 12]);
    }

    /// 963761198400 tiene 6720 divisores: el recuento es el hecho, la lista es
    /// un volcado que nadie lee.
    #[test]
    fn too_many_divisors_reports_the_count_without_the_list() {
        let factorization = factorize(&integer(963_761_198_400)).unwrap();

        assert_eq!(factorization.divisor_count(), Some(6720));
        assert_eq!(factorization.divisors(), None);
    }

    #[test]
    fn parity_does_not_need_the_magnitude_budget() {
        let huge = Integer::from(2).pow(100_u32);

        assert!(is_even(&huge));
        assert!(!is_even(&integer(-7)));
        assert!(is_even(&integer(0)));
    }
}
