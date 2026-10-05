//! Del hecho de dominio al hecho de cable.
//!
//! La traducción vive aquí y no en `ms-serialize` por lo mismo que la de
//! `Evidence → Verification`: la frontera es la única capa que ve los dos lados
//! (02-architecture.md §2).

use ms_facts::{Fact, Parity, Primality, Sign, about_rational, in_base};
use ms_num::Rational;
use ms_serialize::{
    DigitPlace, FactDoc, ParityValue, PrimalityValue, PrimePower, SignValue, decimal_latex,
    factorization_latex, in_base_latex, mixed_latex, percent_latex, periodic_latex,
};
use num_traits::Signed;

/// Los hechos del resultado, tal y como los lee el navegador.
///
/// Qué merece decirse lo decide `ms-facts`; aquí sólo se traduce.
pub fn facts(exact: Option<&Rational>, requested_base: Option<u32>) -> Vec<FactDoc> {
    let Some(value) = exact else {
        return Vec::new();
    };

    let mut documented: Vec<FactDoc> = about_rational(value).iter().map(to_document).collect();

    // La base pedida se atiende **después** de lo que el número dice de sí
    // mismo: quien la pidió sigue queriendo saber que el resultado es par. Va la
    // última porque es la respuesta a lo que se preguntó, y es donde se lee.
    if let Some(base) = requested_base {
        documented.extend(in_base(value, base).iter().map(to_document));
    }
    documented
}

/// Las cifras de un periodo, como texto. Van dígito a dígito porque un entero se
/// comería el cero de `\overline{03}`, y con él la longitud del periodo.
fn digits_to_string(digits: &[u8]) -> String {
    digits.iter().map(|digit| char::from(b'0' + digit)).collect()
}

fn to_document(fact: &Fact) -> FactDoc {
    match fact {
        Fact::Sign(sign) => FactDoc::Sign {
            value: match sign {
                Sign::Negative => SignValue::Negative,
                Sign::Zero => SignValue::Zero,
                Sign::Positive => SignValue::Positive,
            },
        },
        Fact::Parity(parity) => FactDoc::Parity {
            value: match parity {
                Parity::Even => ParityValue::Even,
                Parity::Odd => ParityValue::Odd,
            },
        },
        Fact::Primality(primality) => FactDoc::Primality {
            value: match primality {
                Primality::Prime => PrimalityValue::Prime,
                Primality::Composite => PrimalityValue::Composite,
                Primality::Unit => PrimalityValue::Unit,
            },
        },
        Fact::Factorization {
            factors,
            remaining,
            negative,
        } => {
            let factors: Vec<PrimePower> = factors
                .iter()
                .map(|(prime, exponent)| PrimePower {
                    prime: prime.to_string(),
                    exponent: *exponent,
                })
                .collect();
            let remaining = remaining.as_ref().map(ToString::to_string);

            FactDoc::Factorization {
                latex: factorization_latex(&factors, remaining.as_deref(), *negative),
                factors,
                remaining,
            }
        }
        Fact::Divisors { count, values } => FactDoc::Divisors {
            count: *count,
            values: values
                .as_ref()
                .map(|divisors| divisors.iter().map(ToString::to_string).collect()),
        },
        Fact::Mixed {
            whole,
            numerator,
            denominator,
            negative,
        } => {
            let (whole, numerator, denominator) = (
                whole.to_string(),
                numerator.to_string(),
                denominator.to_string(),
            );

            FactDoc::Mixed {
                latex: mixed_latex(&whole, &numerator, &denominator, *negative),
                whole,
                numerator,
                denominator,
                negative: *negative,
            }
        }
        // La magnitud y el signo viajan separados hasta el escritor: quien
        // inserta la coma cuenta cifras, y un `-` colado entre ellas la
        // desplaza una posición.
        Fact::Decimal { scaled, scale } => FactDoc::Decimal {
            value: decimal_latex(
                &scaled.magnitude().to_string(),
                *scale,
                scaled.is_negative(),
            ),
        },
        Fact::Periodic(periodic) => {
            let whole = periodic.whole.to_string();
            let nonrepeating = digits_to_string(&periodic.nonrepeating);
            let repeating = digits_to_string(&periodic.repeating);

            FactDoc::Periodic {
                latex: periodic_latex(&whole, &nonrepeating, &repeating, periodic.negative),
                whole,
                nonrepeating,
                repeating,
                negative: periodic.negative,
            }
        }
        Fact::PlaceValue { digits, negative } => FactDoc::PlaceValue {
            digits: digits
                .iter()
                .map(|(digit, exponent)| DigitPlace {
                    digit: *digit,
                    exponent: *exponent,
                })
                .collect(),
            negative: *negative,
        },
        Fact::InBase {
            digits,
            base,
            negative,
        } => FactDoc::InBase {
            latex: in_base_latex(digits, *base, *negative),
            digits: digits.clone(),
            base: *base,
            negative: *negative,
        },
        Fact::Percent(percentage) => {
            let value = percentage.to_string();

            FactDoc::Percent {
                latex: percent_latex(&value),
                value,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ms_num::Integer;

    use super::*;

    fn facts_of(value: i64) -> Vec<FactDoc> {
        facts(Some(&Rational::from(Integer::from(value))), None)
    }

    #[test]
    fn a_composite_integer_crosses_the_wire_with_all_its_classes() {
        let documented = facts_of(12);

        // Signo, paridad, posiciones, primalidad, factorización y divisores.
        assert_eq!(documented.len(), 6);
        assert_eq!(
            documented.last(),
            Some(&FactDoc::Divisors {
                count: 6,
                values: Some(
                    ["1", "2", "3", "4", "6", "12"]
                        .iter()
                        .map(ToString::to_string)
                        .collect()
                ),
            })
        );
    }

    #[test]
    fn the_factorization_arrives_already_written_in_latex() {
        let documented = facts_of(-12);

        assert!(documented.iter().any(|fact| matches!(
            fact,
            FactDoc::Factorization { latex, .. } if latex == "-2^{2} \\cdot 3"
        )));
    }

    /// Una fracción propia con decimal finito cruza el cable con él, y con nada
    /// más: no hay parte entera que sacar, y `100 \cdot \frac{3}{8}` no es
    /// entero, así que tampoco hay porcentaje exacto.
    #[test]
    fn a_proper_fraction_crosses_the_wire_with_its_exact_decimal() {
        let three_eighths = Rational::new(Integer::from(3), Integer::from(8));

        assert!(facts(Some(&three_eighths), None).contains(&FactDoc::Decimal {
            value: "0.375".to_owned()
        }));
    }

    /// Un periódico no tiene decimal finito, así que cruza el cable con su
    /// desarrollo y su barra ya escrita. Sin resultado no hay hechos: no es un
    /// fallo, es que no hay nada que decir.
    #[test]
    fn a_repeating_fraction_crosses_the_wire_with_its_period() {
        let sixth = Rational::new(Integer::from(1), Integer::from(6));

        assert_eq!(
            facts(Some(&sixth), None),
            vec![FactDoc::Periodic {
                whole: "0".to_owned(),
                nonrepeating: "1".to_owned(),
                repeating: "6".to_owned(),
                negative: false,
                latex: "0.1\\overline{6}".to_owned(),
            }]
        );
        assert_eq!(facts(None, None), Vec::new());
    }

    /// Una impropia cruza el cable con su vista mixta y el LaTeX ya escrito, que
    /// es lo que la interfaz necesita para no volver a hacer la división.
    #[test]
    fn an_improper_fraction_crosses_the_wire_as_a_mixed_number() {
        let seven_thirds = Rational::new(Integer::from(-7), Integer::from(3));

        assert!(facts(Some(&seven_thirds), None).contains(&FactDoc::Mixed {
            whole: "2".to_owned(),
            numerator: "1".to_owned(),
            denominator: "3".to_owned(),
            negative: true,
            // El signo va delante del conjunto: -2\frac{1}{3} es -\frac{7}{3},
            // y no -2 + \frac{1}{3}.
            latex: "-2\\frac{1}{3}".to_owned(),
        }));
    }
}
