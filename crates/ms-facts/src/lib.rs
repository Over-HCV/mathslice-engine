//! `ms-facts` — lo que el motor sabe de un resultado además de su valor.
//!
//! Un CAS que sólo devuelve `12` ha contestado la pregunta y ha desaprovechado
//! todo lo que sabe: que es par, que es compuesto, que es `2² · 3`, que tiene
//! seis divisores. `facts` es lo que reemplaza al pod `plot` de un CAS comercial
//! (02-architecture.md §3): datos, no una representación. La interfaz decide si
//! con los divisores dibuja una rejilla, una tabla o nada.
//!
//! # Lo que no está aquí, y por qué
//!
//! No hay `\gcd(a, b)` ni ninguna otra consulta. Preguntar por el MCD de dos
//! números exige antes decidir cómo vive una función con nombre en el IR —
//! variantes concretas en `Head`, que el kernel enumera para siempre, o
//! `Head::Fn(SymId)`, que apaga los `match` exhaustivos que hoy protegen. Esa
//! decisión espera a E2, donde el DSL de patrones de `rules!` la obliga con más
//! información encima de la mesa. Mientras tanto, factorizar los dos números da
//! el MCD por la vía de los hechos, sin añadir una sola cabeza.
//!
//! # Cada clase de valor afirma lo suyo
//!
//! Un entero trae hasta cinco clases; una fracción impropia trae su vista mixta;
//! `\frac{3}{4}` no trae ninguna, y eso también es una respuesta: vacío
//! **siempre** habría sido decorado, vacío a veces es información. Nada se
//! afirma por defecto — paridad y primalidad no son propiedades de un racional,
//! y darles una definición inventada es cómo un CAS acaba diciendo algo falso.

use ms_num::{Integer, Periodic, Rational, digits_in_base, factorize, is_even, periodic_decimal};
use num_traits::{One, Pow, Signed, Zero};

/// Un porcentaje es una centésima parte, aquí y en el parser.
const PERCENT_SCALE: i64 = 100;

/// Cuántas posiciones se enumeran en el valor posicional.
///
/// Es un hecho para leer una cifra a la vez, y `2^{100}` tiene 31: enumerarlas
/// no enseña nada y llena la pantalla. El tope es de legibilidad, no de cálculo.
const MAX_PLACE_POSITIONS: usize = 12;

/// El signo, como hecho y no como carácter de la salida.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sign {
    Negative,
    Zero,
    Positive,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Parity {
    Even,
    Odd,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Primality {
    Prime,
    Composite,
    /// ±1: ni primo ni compuesto. Son las unidades del anillo, y llamarlas
    /// «no primo» a secas es la confusión que el estudiante trae de casa.
    Unit,
}

/// Un hecho sobre el resultado.
///
/// Enum y no `{clase, valor}`: la interfaz que dibuja los divisores necesita los
/// divisores, no una cadena que tenga que volver a parsear.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Fact {
    Sign(Sign),
    Parity(Parity),
    Primality(Primality),
    /// La factorización de `|n|`. El signo es el hecho [`Fact::Sign`]; `negative`
    /// está aquí sólo para que quien escriba `-2^{2} \cdot 3` no tenga que
    /// cruzar dos hechos para saberlo.
    Factorization {
        factors: Vec<(Integer, u32)>,
        /// Lo que el presupuesto no alcanzó a factorizar. `None` significa que
        /// el producto de los factores es el número.
        remaining: Option<Integer>,
        negative: bool,
    },
    /// Los divisores positivos. `values` es `None` cuando hay demasiados para
    /// enumerarlos: el recuento sigue siendo el hecho, la lista era el volcado.
    Divisors {
        count: u64,
        values: Option<Vec<Integer>>,
    },
    /// La vista mixta de una fracción impropia: `\frac{7}{3}` es `2` y `\frac{1}{3}`.
    ///
    /// Las tres partes son **magnitudes** y el signo va aparte, en `negative`,
    /// porque el signo de una mixta afecta al conjunto: `-2\frac{1}{3}` es
    /// `-\frac{7}{3}`, y no `-2 + \frac{1}{3}`. Es el mismo reparto que en
    /// [`Fact::Factorization`], y por la misma razón: quien la escriba no
    /// debería tener que cruzar dos hechos.
    ///
    /// **Es un hecho y no una forma de entrada.** El motor no acepta mixtas
    /// escritas: la fracción es la forma en la que se opera, y la mixta sólo
    /// existe como lectura de un resultado que ya salió de una fracción.
    Mixed {
        whole: Integer,
        numerator: Integer,
        denominator: Integer,
        negative: bool,
    },
    /// El valor en forma decimal **exacta**, cuando la tiene: `\frac{173}{50}`
    /// es `3.46`.
    ///
    /// Existe porque sin ella el redondeo no se puede leer: pides dos cifras
    /// decimales, el motor contesta `\frac{173}{50}` —que es correcto— y el
    /// `3.46` aparecía sólo como aproximación, con un `≈` que además era falso.
    ///
    /// Un racional tiene decimal finito exactamente cuando su denominador sólo
    /// tiene factores 2 y 5. Los demás son periódicos, y ésos son otro hecho.
    ///
    /// `value = scaled / 10^{scale}`, y quien lo escribe pone la coma: aquí no
    /// hay cadenas de formato, hay un entero y una potencia de diez.
    Decimal {
        scaled: Integer,
        scale: u32,
    },
    /// Qué vale cada cifra por el sitio donde está: `34.5` son un `3` en las
    /// decenas, un `4` en las unidades y un `5` en las décimas.
    ///
    /// Cada par es `(cifra, exponente)`, de la más significativa a la menos, y
    /// el exponente es la potencia de diez de esa posición: `2` para las
    /// centenas, `-1` para las décimas. **Los ceros intermedios entran**: en
    /// `1200` el «0 decenas» es parte de lo que hay que leer, y quitarlo
    /// convertiría el hecho en una lista de cifras sueltas.
    ///
    /// Los nombres —«decenas», «décimas»— **no** están aquí: son idioma, y viven
    /// en la capa que dibuja (invariante 4).
    PlaceValue {
        digits: Vec<(u8, i32)>,
        negative: bool,
    },
    /// El desarrollo decimal periódico, cuando el decimal no es finito:
    /// `\frac{1}{6}` es `0.1\overline{6}`.
    ///
    /// Es el complemento exacto de [`Fact::Decimal`]: un racional no entero tiene
    /// uno o el otro, nunca los dos, porque su denominador o sólo tiene factores
    /// 2 y 5 o no. Que la lista traiga siempre una de las dos lecturas es lo que
    /// hace que `\frac{1}{3}` deje de salir sin nada que decir.
    Periodic(Periodic),
    /// El valor escrito como porcentaje: `\frac{1}{2}` es `50\%`.
    ///
    /// Dos condiciones, y las dos son decisiones:
    ///
    /// - `100 \cdot valor` tiene que ser **entero**. `\frac{1}{3}` no lo es, y
    ///   escribirlo `33.33\%` sería redondear sin que nadie lo pidiera, o
    ///   `\frac{100}{3}\%` sería cambiar una fracción por otra peor.
    /// - El valor **no** puede ser entero. `3` es `300\%`, y es verdad, pero
    ///   nadie escribe `3` para leer eso: de un entero se dicen sus cinco
    ///   clases, que es lo que se le preguntó.
    Percent(Integer),
    /// El valor escrito en otra base: `11` es `1011` en base 2.
    ///
    /// Es el único hecho que responde a una **petición** y no al valor. Los demás
    /// salen de mirar el número; éste sale de que alguien escribiera
    /// `\operatorname{enBase}(2, \ldots)`, y por eso no lo produce
    /// [`about_rational`] sino [`in_base`], que la frontera llama aparte.
    ///
    /// Las cifras van de la más significativa a la menos y son **números**, no
    /// símbolos: que el diez se dibuje `a`, `A` o `(10)` es representación y vive
    /// en la capa que dibuja. El signo va fuera, como en la mixta y en el
    /// periódico.
    InBase {
        digits: Vec<u8>,
        base: u32,
        negative: bool,
    },
}

/// Los hechos de un valor exacto.
///
/// Un `Rational` está siempre reducido, así que denominador 1 **es** la
/// definición de entero. Qué se afirma de cada clase de valor se decide aquí y
/// no en la frontera: es una regla del dominio, no de la traducción.
pub fn about_rational(value: &Rational) -> Vec<Fact> {
    if value.denom().is_one() {
        return about_integer(value.numer());
    }
    about_fraction(value)
}

/// El valor escrito en la base pedida, si se puede escribir en ella.
///
/// Va aparte de [`about_rational`] a propósito: los demás hechos son lo que el
/// motor sabe del número, y éste es lo que alguien pidió. Mezclarlos obligaría a
/// pasarle una base a una función que afirma cosas de un número, y a inventar una
/// base por defecto para las consultas que no piden ninguna.
///
/// Devuelve `None` en los dos casos en que no hay nada honesto que decir:
///
/// - **El valor no es entero.** `\frac{1}{3}` en base 2 es `0.\overline{01}`, un
///   desarrollo periódico en otra base, y eso es otro hecho que todavía no
///   existe. Inventar un redondeo aquí sería contestar otra pregunta.
/// - **La base no se puede escribir** — fuera de `2..=36`. Lo comprueba
///   `ms_num::digits_in_base`, que es donde vive el rango.
pub fn in_base(value: &Rational, base: u32) -> Option<Fact> {
    if !value.is_integer() {
        return None;
    }
    let integer = value.to_integer();
    let digits = digits_in_base(&integer, base)?;

    Some(Fact::InBase {
        digits,
        base,
        negative: integer.is_negative(),
    })
}

/// Lo que se afirma de una fracción que no es entera: su vista mixta, y sólo
/// cuando hay parte entera que sacar.
///
/// `\frac{3}{4}` no la tiene, y decir «0 enteros y tres cuartos» no es un hecho
/// sino ruido con formato. Ni paridad ni primalidad aparecen aquí: no son
/// propiedades de un racional, y afirmarlas exigiría inventarles una definición.
fn about_fraction(value: &Rational) -> Vec<Fact> {
    let mut facts = Vec::new();

    let magnitude = value.abs();
    let whole = magnitude.to_integer();
    if !whole.is_zero() {
        let remainder = magnitude.fract();
        facts.push(Fact::Mixed {
            whole,
            numerator: remainder.numer().clone(),
            denominator: remainder.denom().clone(),
            negative: value.is_negative(),
        });
    }

    if let Some((scaled, scale)) = as_terminating_decimal(value) {
        facts.extend(place_value(&scaled, scale));
        facts.push(Fact::Decimal { scaled, scale });
    } else if let Some(periodic) = periodic_decimal(value) {
        // `else`: las dos lecturas son excluyentes por definición, y un valor con
        // las dos sería una contradicción, no información de más.
        facts.push(Fact::Periodic(periodic));
    }

    if let Some(percentage) = as_percentage(value) {
        facts.push(Fact::Percent(percentage));
    }

    facts
}

/// El valor como `(n, k)` con `valor = n / 10^{k}`, si su decimal es finito.
///
/// Lo es exactamente cuando el denominador —ya reducido— sólo tiene factores 2 y
/// 5, y entonces `k` es el mayor de los dos exponentes: multiplicar por `10^{k}`
/// aporta justo los que faltan. No hace falta factorizar nada: se dividen los
/// dos factores y se cuenta.
fn as_terminating_decimal(value: &Rational) -> Option<(Integer, u32)> {
    let mut denominator = value.denom().clone();
    let twos = strip_factor(&mut denominator, 2);
    let fives = strip_factor(&mut denominator, 5);

    // Lo que quede después de quitar los 2 y los 5 es lo que hace periódico al
    // decimal. Ese caso es otro hecho, y no éste.
    if !denominator.is_one() {
        return None;
    }

    let scale = twos.max(fives);
    let power = Pow::pow(Integer::from(10), scale);
    let scaled = (value * Rational::from(power)).to_integer();
    Some((scaled, scale))
}

/// El valor posicional de `scaled / 10^{scale}`, cifra a cifra.
///
/// El exponente de la primera cifra sale de contar: con `d` cifras y `scale`
/// decimales, la más significativa está en la posición `d - 1 - scale`. Es la
/// misma cuenta que hace quien escribe el número, y por eso se hace una sola vez.
fn place_value(scaled: &Integer, scale: u32) -> Option<Fact> {
    let digits = scaled.magnitude().to_string();
    if digits.len() > MAX_PLACE_POSITIONS {
        return None;
    }
    // Un entero de una cifra no tiene nada que decir de sus posiciones: «3
    // unidades» es el número otra vez. `0.5` sí, porque «5 décimas» es
    // información que la cifra sola no da.
    if digits.len() == 1 && scale == 0 {
        return None;
    }

    let scale = i32::try_from(scale).ok()?;
    let highest = i32::try_from(digits.len()).ok()? - 1 - scale;

    let positions = digits
        .chars()
        .enumerate()
        .map(|(offset, digit)| {
            let digit = u8::try_from(digit.to_digit(10)?).ok()?;
            let exponent = highest - i32::try_from(offset).ok()?;
            Some((digit, exponent))
        })
        .collect::<Option<Vec<_>>>()?;

    Some(Fact::PlaceValue {
        digits: positions,
        negative: scaled.is_negative(),
    })
}

/// Divide `value` por `factor` mientras se pueda y devuelve cuántas veces.
fn strip_factor(value: &mut Integer, factor: i64) -> u32 {
    let factor = Integer::from(factor);
    let mut exponent = 0;
    while (&*value % &factor).is_zero() {
        *value /= &factor;
        exponent += 1;
    }
    exponent
}

/// `100 \cdot valor`, si sale entero.
///
/// El signo se queda **dentro**, al contrario que en la mixta: `-50\%` es un
/// número negativo escrito de otra forma, y no hay ninguna parte entera de la
/// que el signo pudiera despegarse.
fn as_percentage(value: &Rational) -> Option<Integer> {
    let scaled = value * Rational::from(Integer::from(PERCENT_SCALE));
    if !scaled.is_integer() {
        return None;
    }
    Some(scaled.to_integer())
}

/// Todo lo que el motor sabe afirmar de un entero.
///
/// La única puerta del crate, y por tanto el único sitio donde vive la tabla de
/// qué clase se emite cuándo:
///
/// | clase | se emite cuando |
/// |---|---|
/// | `sign`, `parity` | siempre — son baratas para cualquier magnitud |
/// | `primality` | `\|n\| = 1`, o `2 ≤ \|n\|` dentro del tope de `ms-num` |
/// | `factorization` | `2 ≤ \|n\|` dentro del tope |
/// | `divisors` | además, la factorización salió completa |
pub fn about_integer(value: &Integer) -> Vec<Fact> {
    let mut facts = vec![Fact::Sign(sign_of(value)), Fact::Parity(parity_of(value))];
    facts.extend(place_value(value, 0));

    if value.magnitude().is_one() {
        facts.push(Fact::Primality(Primality::Unit));
        return facts;
    }

    // El 0 no tiene factorización, y una magnitud fuera del tope no la tiene
    // *que se pueda calcular*. En ninguno de los dos casos se afirma nada más:
    // no saber no es lo mismo que saber que no.
    let Some(factorization) = factorize(value) else {
        return facts;
    };

    // La primalidad se **deriva** de la factorización en vez de preguntarla
    // aparte: dos caminos hasta la misma respuesta son dos caminos que pueden
    // discrepar, y un resultado que se contradice a sí mismo es peor que uno
    // incompleto. Un resto sin factorizar sólo aparece cuando Miller–Rabin ya
    // dijo que es compuesto, así que «incompleta» nunca significa «quizá primo».
    let primality = match factorization.factors() {
        [(_, 1)] if factorization.is_complete() => Primality::Prime,
        _ => Primality::Composite,
    };
    facts.push(Fact::Primality(primality));

    facts.push(Fact::Factorization {
        factors: factorization.factors().to_vec(),
        remaining: factorization.remaining().cloned(),
        negative: value.is_negative(),
    });

    if let Some(count) = factorization.divisor_count() {
        facts.push(Fact::Divisors {
            count,
            values: factorization.divisors(),
        });
    }

    facts
}

fn sign_of(value: &Integer) -> Sign {
    if value.is_negative() {
        return Sign::Negative;
    }
    if value.is_positive() {
        return Sign::Positive;
    }
    Sign::Zero
}

fn parity_of(value: &Integer) -> Parity {
    if is_even(value) {
        return Parity::Even;
    }
    Parity::Odd
}

#[cfg(test)]
mod tests {
    use num_traits::Pow;

    use super::*;

    fn facts(value: i64) -> Vec<Fact> {
        about_integer(&Integer::from(value))
    }

    fn integers(values: &[i64]) -> Vec<Integer> {
        values.iter().copied().map(Integer::from).collect()
    }

    #[test]
    fn a_composite_integer_brings_all_its_classes() {
        assert_eq!(
            facts(12),
            vec![
                Fact::Sign(Sign::Positive),
                Fact::Parity(Parity::Even),
                Fact::PlaceValue {
                    digits: vec![(1, 1), (2, 0)],
                    negative: false,
                },
                Fact::Primality(Primality::Composite),
                Fact::Factorization {
                    factors: vec![(Integer::from(2), 2), (Integer::from(3), 1)],
                    remaining: None,
                    negative: false,
                },
                Fact::Divisors {
                    count: 6,
                    values: Some(integers(&[1, 2, 3, 4, 6, 12])),
                },
            ]
        );
    }

    #[test]
    fn a_prime_says_so_and_lists_its_two_divisors() {
        let facts = facts(97);

        assert!(facts.contains(&Fact::Primality(Primality::Prime)));
        assert!(facts.contains(&Fact::Divisors {
            count: 2,
            values: Some(integers(&[1, 97])),
        }));
    }

    /// El signo es un hecho aparte y la factorización es de la magnitud, pero
    /// `negative` viaja con ella para que escribirla no exija cruzar dos hechos.
    #[test]
    fn a_negative_integer_factors_by_its_magnitude_and_says_it_is_negative() {
        let facts = facts(-12);

        assert!(facts.contains(&Fact::Sign(Sign::Negative)));
        assert!(facts.contains(&Fact::Factorization {
            factors: vec![(Integer::from(2), 2), (Integer::from(3), 1)],
            remaining: None,
            negative: true,
        }));
    }

    #[test]
    fn one_is_a_unit_and_has_nothing_to_factor() {
        assert_eq!(
            facts(1),
            vec![
                Fact::Sign(Sign::Positive),
                Fact::Parity(Parity::Odd),
                Fact::Primality(Primality::Unit),
            ]
        );
        assert!(facts(-1).contains(&Fact::Primality(Primality::Unit)));
    }

    /// Un racional con denominador 1 es un entero y trae los hechos de un
    /// entero. Una fracción no trae ni paridad ni primalidad: no son propiedades
    /// suyas, y darles una definición inventada es cómo se afirma algo falso.
    #[test]
    fn an_integer_rational_reports_what_an_integer_reports() {
        let twelve = Rational::from(Integer::from(12));
        assert_eq!(about_rational(&twelve), facts(12));

        // Un racional no entero trae siempre una de las dos lecturas del
        // decimal, finita o periódica: la lista vacía se quedó para los enteros
        // grandes y los símbolos.
        let third = Rational::new(Integer::from(1), Integer::from(3));
        assert!(matches!(
            about_rational(&third).as_slice(),
            [Fact::Periodic(_)]
        ));
    }

    /// La impropia se parte en parte entera y resto, con las tres partes en
    /// magnitud y el signo aparte.
    #[test]
    fn an_improper_fraction_splits_into_a_whole_part_and_a_remainder() {
        let seven_thirds = Rational::new(Integer::from(7), Integer::from(3));

        assert!(about_rational(&seven_thirds).contains(&Fact::Mixed {
            whole: Integer::from(2),
            numerator: Integer::from(1),
            denominator: Integer::from(3),
            negative: false,
        }));
    }

    /// El signo de una mixta es del conjunto: `-\frac{7}{3}` es `-2\frac{1}{3}`,
    /// que vale `-2 - \frac{1}{3}` y no `-2 + \frac{1}{3}`. Por eso las partes
    /// van en magnitud: un numerador negativo aquí invitaría a sumarlo.
    #[test]
    fn a_negative_improper_fraction_keeps_its_sign_on_the_whole_thing() {
        let minus_seven_thirds = Rational::new(Integer::from(-7), Integer::from(3));

        assert!(about_rational(&minus_seven_thirds).contains(&Fact::Mixed {
            whole: Integer::from(2),
            numerator: Integer::from(1),
            denominator: Integer::from(3),
            negative: true,
        }));
    }

    /// `\frac{1}{2}` no tiene parte entera, y sí las otras dos lecturas.
    #[test]
    fn a_half_is_a_decimal_and_a_percentage() {
        let half = Rational::new(Integer::from(1), Integer::from(2));

        assert_eq!(
            about_rational(&half),
            vec![
                // «5 décimas» dice algo que la cifra sola no dice, así que un
                // decimal de una cifra sí trae posiciones. Un entero de una
                // cifra no.
                Fact::PlaceValue {
                    digits: vec![(5, -1)],
                    negative: false,
                },
                Fact::Decimal {
                    scaled: Integer::from(5),
                    scale: 1,
                },
                Fact::Percent(Integer::from(50)),
            ]
        );
    }

    /// `\frac{5}{2}` tiene las tres lecturas, y las tres son verdad.
    #[test]
    fn an_improper_fraction_can_have_every_reading() {
        let five_halves = Rational::new(Integer::from(5), Integer::from(2));

        assert_eq!(
            about_rational(&five_halves),
            vec![
                Fact::Mixed {
                    whole: Integer::from(2),
                    numerator: Integer::from(1),
                    denominator: Integer::from(2),
                    negative: false,
                },
                Fact::PlaceValue {
                    digits: vec![(2, 0), (5, -1)],
                    negative: false,
                },
                Fact::Decimal {
                    scaled: Integer::from(25),
                    scale: 1,
                },
                Fact::Percent(Integer::from(250)),
            ]
        );
    }

    /// El decimal aparece exactamente cuando el denominador sólo tiene 2 y 5.
    /// `\frac{1}{8}` lo tiene y vale `0.125`; `\frac{1}{6}` no, porque el 3
    /// sobrevive y el decimal es periódico.
    #[test]
    fn a_decimal_appears_only_when_the_denominator_is_two_and_five() {
        let eighth = Rational::new(Integer::from(1), Integer::from(8));
        let sixth = Rational::new(Integer::from(1), Integer::from(6));

        assert!(about_rational(&eighth).contains(&Fact::Decimal {
            scaled: Integer::from(125),
            scale: 3,
        }));
        assert!(
            !about_rational(&sixth)
                .iter()
                .any(|fact| matches!(fact, Fact::Decimal { .. }))
        );
    }

    /// `\frac{1}{3}` no se escribe como porcentaje exacto, y `33.33\%` sería
    /// redondear sin que nadie lo haya pedido. Lo que sí trae es su periódico:
    /// no afirmar nada era la respuesta hasta que hubo algo exacto que decir.
    #[test]
    fn a_third_has_no_exact_percentage_but_it_does_have_a_period() {
        let third = Rational::new(Integer::from(1), Integer::from(3));
        let facts = about_rational(&third);

        assert!(
            !facts
                .iter()
                .any(|fact| matches!(fact, Fact::Percent(_) | Fact::Decimal { .. }))
        );
        assert!(matches!(
            facts.as_slice(),
            [Fact::Periodic(periodic)] if periodic.repeating == vec![3]
        ));
    }

    /// El signo de un porcentaje va dentro del número: no hay conjunto del que
    /// despegarlo, al contrario que en la mixta.
    #[test]
    fn a_negative_value_keeps_its_sign_inside_the_percentage() {
        let minus_half = Rational::new(Integer::from(-1), Integer::from(2));

        assert!(about_rational(&minus_half).contains(&Fact::Percent(Integer::from(-50))));
    }

    /// Una fracción cuya parte entera es exactamente 1 sí la tiene: `\frac{4}{3}`
    /// es `1\frac{1}{3}`. El borde está en el 0, no en el 1.
    #[test]
    fn the_boundary_is_a_zero_whole_part_not_a_small_one() {
        let four_thirds = Rational::new(Integer::from(4), Integer::from(3));

        assert!(about_rational(&four_thirds).iter().any(|fact| matches!(
            fact,
            Fact::Mixed { whole, .. } if whole.is_one()
        )));
    }

    /// Los divisores de 0 son todos los enteros y su factorización no existe.
    /// Signo y paridad sí, y son todo lo que se afirma.
    #[test]
    fn zero_claims_only_what_is_true_of_it() {
        assert_eq!(
            facts(0),
            vec![Fact::Sign(Sign::Zero), Fact::Parity(Parity::Even)]
        );
    }

    /// Fuera del tope de magnitud, el motor no se calla el signo ni la paridad:
    /// son baratos. Se calla lo que no puede calcular.
    #[test]
    fn beyond_the_factoring_budget_only_the_cheap_facts_survive() {
        let huge = Integer::from(2).pow(100_u32);

        assert_eq!(
            about_integer(&huge),
            vec![Fact::Sign(Sign::Positive), Fact::Parity(Parity::Even)]
        );
    }

    /// Una factorización incompleta no emite divisores: contarlos sobre factores
    /// parciales daría un número menor que el real.
    #[test]
    fn an_incomplete_factorization_reports_its_remainder_and_no_divisors() {
        let semiprime = 1_000_003_i64 * 1_000_033;
        let facts = facts(semiprime);

        assert!(facts.contains(&Fact::Primality(Primality::Composite)));
        assert!(facts.contains(&Fact::Factorization {
            factors: Vec::new(),
            remaining: Some(Integer::from(semiprime)),
            negative: false,
        }));
        assert!(
            !facts
                .iter()
                .any(|fact| matches!(fact, Fact::Divisors { .. }))
        );
    }
}
