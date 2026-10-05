//! La forma de cable de los hechos de un resultado.
//!
//! Espejo de `ms-facts`, y **no** su tipo: `ms-serialize` describe la forma del
//! documento y no sabe quién produce los hechos, igual que no sabe quién
//! verifica (02-architecture.md §2). Quien traduce entre los dos es la frontera,
//! que es la única que ve ambos lados.
//!
//! Unión discriminada por `class`, no un `{clase, valor}` uniforme: la interfaz
//! que dibuja los divisores necesita los divisores, no una cadena que tenga que
//! volver a parsear.

use serde::{Deserialize, Serialize};

/// Un hecho, tal y como lo lee TypeScript.
///
/// Los enteros van como cadena, por lo mismo que en `Expr`: son bignum, y como
/// número JSON `JSON.parse` los rompería sobre 2⁵³. Los exponentes y los
/// recuentos no son bignum y van como número.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "class", rename_all = "kebab-case")]
pub enum FactDoc {
    Sign {
        value: SignValue,
    },
    Parity {
        value: ParityValue,
    },
    Primality {
        value: PrimalityValue,
    },
    Factorization {
        factors: Vec<PrimePower>,
        /// Lo que el presupuesto no alcanzó a factorizar. `null` significa que
        /// el producto de los factores **es** el número.
        remaining: Option<String>,
        latex: String,
    },
    Divisors {
        count: u64,
        /// `null` cuando hay demasiados para enumerarlos. El recuento sigue
        /// siendo el hecho; la lista era el volcado.
        values: Option<Vec<String>>,
    },
    /// La vista mixta de una fracción impropia. Las tres partes son magnitudes y
    /// el signo va en `negative`: en una mixta afecta al conjunto.
    Mixed {
        whole: String,
        numerator: String,
        denominator: String,
        negative: bool,
        latex: String,
    },
    /// El valor como porcentaje, cuando `100 \cdot valor` es entero. Aquí el
    /// signo va **dentro** del valor: no hay conjunto del que despegarlo.
    Percent {
        value: String,
        latex: String,
    },
    /// El valor en forma decimal exacta, cuando la tiene. **No lleva campo
    /// `latex` aparte** porque un decimal escrito ya es su propio LaTeX: añadir
    /// una copia idéntica sería invitar a que las dos se separaran.
    Decimal {
        value: String,
    },
    /// El desarrollo decimal periódico, ya escrito: `0.1\overline{6}`. Las tres
    /// partes viajan también sueltas para quien quiera pintar la barra a su
    /// manera —o leerla en voz alta— sin volver a parsear la cadena.
    Periodic {
        whole: String,
        nonrepeating: String,
        repeating: String,
        negative: bool,
        latex: String,
    },
    /// Qué vale cada cifra por su posición. Los nombres de las posiciones
    /// —«decenas», «décimas»— son idioma y no viajan: los pone quien dibuja.
    PlaceValue {
        digits: Vec<DigitPlace>,
        negative: bool,
    },
    /// El valor escrito en la base que se pidió: `11` es `1011_{2}`.
    ///
    /// `digits` son **números**, de la cifra más significativa a la menos, para
    /// quien quiera pintarlas de otra forma —una tabla de posiciones, una a una,
    /// agrupadas de cuatro en cuatro—. `latex` trae ya la notación de siempre,
    /// con las cifras por encima de nueve escritas con letra.
    InBase {
        digits: Vec<u8>,
        base: u32,
        negative: bool,
        latex: String,
    },
    /// Puntos de una expresión con una variable libre, **para que los pinte
    /// quien quiera**.
    ///
    /// El motor es agnóstico del graficador: lo que emite son los datos
    /// elementales —qué variable, y qué vale la expresión en una rejilla de
    /// puntos— y cualquier motor de dibujo los consume. Aquí no hay ejes, ni
    /// rango de vista, ni color: eso es representación, y vive en la capa de TS
    /// (02-architecture.md §1).
    ///
    /// **Es un hecho, no una salida.** La gráfica llega derivada de algo ya
    /// calculado, que es lo que pide el roadmap para E3.
    GraphSamples {
        variable: String,
        points: Vec<GraphPoint>,
    },
}

/// Un punto muestreado, con las dos coordenadas **exactas**.
///
/// Como cociente de enteros —`"3"`, `"-1/2"`— y no como decimal, por lo mismo
/// que los enteros van como cadena: son bignum, y redondear a `f64` aquí sería
/// tomar una decisión de presentación en el sitio donde no se toman. Quien
/// dibuja divide y obtiene su flotante.
///
/// Los puntos donde la expresión no está definida **no aparecen**: un hueco es
/// un hueco, y rellenarlo con un cero sería inventarse un punto de la curva.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GraphPoint {
    pub x: String,
    pub y: String,
}

/// Una cifra y la potencia de diez de su posición: el `4` de `34` es
/// `{digit: 4, exponent: 0}`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigitPlace {
    pub digit: u8,
    pub exponent: i32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SignValue {
    Negative,
    Zero,
    Positive,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ParityValue {
    Even,
    Odd,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PrimalityValue {
    Prime,
    Composite,
    /// ±1: ni primo ni compuesto.
    Unit,
}

/// Un primo con su multiplicidad: el `2^{2}` de `12 = 2^{2} \cdot 3`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PrimePower {
    pub prime: String,
    pub exponent: u32,
}

/// El LaTeX de una factorización.
///
/// Vive aquí y no en la frontera porque es LaTeX, y todo el LaTeX del documento
/// se escribe en este crate. El resto sin factorizar entra como un factor más:
/// quitarlo haría que el producto ya no diera el número, que es la única cosa
/// que una factorización tiene que cumplir.
pub fn factorization_latex(
    factors: &[PrimePower],
    remaining: Option<&str>,
    negative: bool,
) -> String {
    let mut rendered = String::new();
    if negative {
        rendered.push('-');
    }

    let powers: Vec<String> = factors
        .iter()
        .map(|factor| {
            if factor.exponent == 1 {
                return factor.prime.clone();
            }
            format!("{}^{{{}}}", factor.prime, factor.exponent)
        })
        .chain(remaining.map(str::to_owned))
        .collect();

    rendered.push_str(&powers.join(" \\cdot "));
    rendered
}

/// El LaTeX de una vista mixta: `2\frac{1}{3}`, con el signo delante del
/// conjunto.
///
/// Aquí y no en la frontera por lo mismo que [`factorization_latex`]: todo el
/// LaTeX del documento se escribe en este crate. Y **sin `\cdot`** entre la
/// parte entera y la fracción, al contrario que en el escritor de expresiones:
/// allí la yuxtaposición sería un producto y habría que separarla; aquí la
/// yuxtaposición *es* la notación mixta, que es justo lo que se quiere decir.
pub fn mixed_latex(whole: &str, numerator: &str, denominator: &str, negative: bool) -> String {
    let sign = if negative { "-" } else { "" };
    format!("{sign}{whole}\\frac{{{numerator}}}{{{denominator}}}")
}

/// El decimal `scaled / 10^{scale}`, escrito con su coma.
///
/// `scaled` llega como cadena porque es bignum. Los ceros a la izquierda se
/// añaden **aquí**: `5 / 10^{3}` es `0.005`, y quien sólo inserta el punto
/// escribe `5.` o `0.5` según la prisa que tenga.
pub fn decimal_latex(scaled: &str, scale: u32, negative: bool) -> String {
    let sign = if negative { "-" } else { "" };
    if scale == 0 {
        return format!("{sign}{scaled}");
    }

    let places = usize::try_from(scale).unwrap_or(usize::MAX);
    let padded = format!("{scaled:0>width$}", width = places + 1);
    let point = padded.len() - places;
    let (whole, fraction) = padded.split_at(point);
    format!("{sign}{whole}.{fraction}")
}

/// El LaTeX de un decimal periódico: `0.1\overline{6}`.
///
/// `\overline` es la barra de periodo, y es la misma sintaxis que acepta el
/// parser: lo que el motor escribe se puede volver a escribir en la caja de
/// texto y sale el mismo número.
pub fn periodic_latex(
    whole: &str,
    nonrepeating: &str,
    repeating: &str,
    negative: bool,
) -> String {
    let sign = if negative { "-" } else { "" };
    format!("{sign}{whole}.{nonrepeating}\\overline{{{repeating}}}")
}

/// El LaTeX de un porcentaje. El `%` va escapado: sin barra, LaTeX se come el
/// resto de la línea como comentario — el mismo motivo por el que el parser sólo
/// acepta `\%`.
pub fn percent_latex(value: &str) -> String {
    format!("{value}\\%")
}

/// El LaTeX de un número en otra base: `1011_{2}`, `ff_{16}`.
///
/// Es la misma sintaxis que acepta el parser, así que lo que el motor escribe se
/// puede volver a pegar en la caja de texto y sale el mismo número.
///
/// Las cifras por encima de nueve se escriben con letra minúscula, que es la
/// convención de la notación hexadecimal y la que el lexer lee. Una cifra que no
/// tuviera símbolo sería una base fuera de rango, y ésas no llegan hasta aquí:
/// `ms_num::digits_in_base` no las produce.
pub fn in_base_latex(digits: &[u8], base: u32, negative: bool) -> String {
    let sign = if negative { "-" } else { "" };
    let written: String = digits
        .iter()
        .map(|digit| char::from_digit(u32::from(*digit), ms_num::MAX_BASE).unwrap_or('?'))
        .collect();
    format!("{sign}{written}_{{{base}}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn power(prime: &str, exponent: u32) -> PrimePower {
        PrimePower {
            prime: prime.to_owned(),
            exponent,
        }
    }

    #[test]
    fn a_factorization_reads_as_the_student_would_write_it() {
        let factors = vec![power("2", 2), power("3", 1)];

        assert_eq!(factorization_latex(&factors, None, false), "2^{2} \\cdot 3");
    }

    #[test]
    fn a_negative_number_carries_its_sign_in_front() {
        let factors = vec![power("2", 2), power("3", 1)];

        assert_eq!(factorization_latex(&factors, None, true), "-2^{2} \\cdot 3");
    }

    #[test]
    fn a_single_prime_needs_no_product_and_no_exponent() {
        assert_eq!(factorization_latex(&[power("97", 1)], None, false), "97");
    }

    /// El resto entra como un factor más: sin él, el producto ya no daría el
    /// número, que es lo único que una factorización tiene que cumplir.
    #[test]
    fn the_unfactored_remainder_is_still_part_of_the_product() {
        let factors = vec![power("2", 1)];

        assert_eq!(
            factorization_latex(&factors, Some("1000036000099"), false),
            "2 \\cdot 1000036000099"
        );
    }

    #[test]
    fn a_number_with_no_small_factors_is_all_remainder() {
        assert_eq!(
            factorization_latex(&[], Some("1000036000099"), false),
            "1000036000099"
        );
    }

    #[test]
    fn the_class_tag_is_what_typescript_switches_on() {
        let encoded = serde_json::to_string(&FactDoc::Parity {
            value: ParityValue::Even,
        })
        .unwrap();

        assert_eq!(encoded, r#"{"class":"parity","value":"even"}"#);
    }
}
