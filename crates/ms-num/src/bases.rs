//! Escribir un entero en otra base.
//!
//! Devuelve **cifras**, no una cadena. Los símbolos con los que se dibuja cada
//! cifra —`a` para el diez en hexadecimal, o `A`, o `10` entre paréntesis en un
//! libro de texto— son representación, y viven en la capa que dibuja
//! (invariante 4). Aquí sólo está la aritmética.

use num_traits::{Signed, ToPrimitive, Zero};

use crate::Integer;

/// La base más baja que significa algo: en base 1 no hay notación posicional.
pub const MIN_BASE: u32 = 2;

/// La base más alta que se puede escribir: diez cifras y veintiséis letras.
///
/// Es un tope de **notación**, no de aritmética: el algoritmo funciona con
/// cualquier base, pero por encima de 36 no hay con qué escribir las cifras sin
/// inventar símbolos, y un número que no se puede leer no es una respuesta.
pub const MAX_BASE: u32 = 36;

/// Las cifras de `value` en base `base`, de la más significativa a la menos.
///
/// El signo **no** va en las cifras: quien las escriba pone el menos delante,
/// igual que con [`crate::Periodic`] y con la vista mixta. `0` es un solo cero,
/// que es como se escribe, y no una lista vacía.
///
/// Devuelve `None` si la base cae fuera de `MIN_BASE..=MAX_BASE`. Es un dato de
/// entrada que llega del usuario, y una base imposible es una pregunta mal
/// hecha, no un fallo del motor.
pub fn digits_in_base(value: &Integer, base: u32) -> Option<Vec<u8>> {
    if !(MIN_BASE..=MAX_BASE).contains(&base) {
        return None;
    }

    let mut remaining = value.abs();
    if remaining.is_zero() {
        return Some(vec![0]);
    }

    let divisor = Integer::from(base);
    let mut digits = Vec::new();
    while !remaining.is_zero() {
        let digit = (&remaining % &divisor).to_u8()?;
        digits.push(digit);
        remaining /= &divisor;
    }
    digits.reverse();
    Some(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digits(value: i64, base: u32) -> Vec<u8> {
        digits_in_base(&Integer::from(value), base).expect("base admitida")
    }

    #[test]
    fn eleven_in_binary_is_one_zero_one_one() {
        assert_eq!(digits(11, 2), vec![1, 0, 1, 1]);
    }

    #[test]
    fn two_hundred_fifty_five_in_hexadecimal_is_two_fifteens() {
        assert_eq!(digits(255, 16), vec![15, 15]);
    }

    /// El cero se escribe, no se calla. Un bucle que divide hasta agotar el
    /// número devuelve la lista vacía para el cero, y «» no es un número.
    #[test]
    fn zero_is_a_single_zero_digit() {
        assert_eq!(digits(0, 2), vec![0]);
    }

    /// El signo va aparte, como en todos los demás hechos: las cifras de -11 y
    /// las de 11 son las mismas.
    #[test]
    fn the_sign_is_not_one_of_the_digits() {
        assert_eq!(digits(-11, 2), digits(11, 2));
    }

    #[test]
    fn base_ten_gives_back_the_digits_that_were_written() {
        assert_eq!(digits(1204, 10), vec![1, 2, 0, 4]);
    }

    #[test]
    fn a_base_outside_what_can_be_written_has_no_digits() {
        for base in [0, 1, MAX_BASE + 1, 100] {
            assert_eq!(digits_in_base(&Integer::from(255), base), None);
        }
    }

    /// La vuelta: las cifras, leídas por su valor posicional, dan el número.
    #[test]
    fn reading_the_digits_back_gives_the_number() {
        for base in [2_u32, 7, 10, 16, 36] {
            for value in [0_i64, 1, 9, 255, 1_000_000, -4321] {
                let digits = digits(value, base);
                let mut read = Integer::from(0);
                for digit in &digits {
                    read = read * Integer::from(base) + Integer::from(*digit);
                }
                assert_eq!(read, Integer::from(value).abs(), "{value} en base {base}");
            }
        }
    }
}
