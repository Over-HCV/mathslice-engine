//! LaTeX → tokens, con span de bytes en cada uno.

use ms_core::Builtin;
use ms_num::{MAX_BASE, MIN_BASE};
use num_bigint::BigInt;

use crate::error::{ParseError, ParseErrorKind, Span};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Int(BigInt),
    /// Un decimal, con sus dos mitades tal y como se escribieron. **No trae su
    /// valor ya calculado**: el parser lo desazucara a `\frac{n}{10^{k}}`, y
    /// guardar aquí el cociente obligaría a reconstruir el texto para poder
    /// nombrarlo en un mensaje de error.
    Decimal {
        whole: BigInt,
        fraction: String,
    },
    Symbol(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Frac,
    /// `\operatorname{nombre}`, el comando con el que LaTeX escribe una función
    /// con nombre. El nombre viaja resuelto: un nombre que el motor no conoce se
    /// rechaza en el lexer, donde su span sigue siendo el del nombre.
    Function(Builtin),
    /// Separa los argumentos de una llamada. Fuera de una llamada no significa
    /// nada, y por eso el parser la rechaza como átomo.
    Comma,
    /// Un decimal periódico, `0.1\overline{6}`, con sus tres partes tal y como
    /// se escribieron. Las cifras del periodo van como **texto**: `0.\overline{03}`
    /// tiene un cero que un entero se comería, y con él se iría la longitud del
    /// periodo, que es lo que decide qué número es.
    Periodic {
        whole: BigInt,
        nonrepeating: String,
        repeating: String,
    },
    /// Un número escrito en otra base: `1011_{2}`, `ff_{16}`.
    ///
    /// Las cifras van sueltas y en orden, con la base al lado, porque lo que el
    /// parser construye no es el valor sino su **desarrollo posicional** — y para
    /// eso hace falta saber qué cifra va en qué posición.
    InBase {
        digits: Vec<u8>,
        base: u32,
    },
    /// `\%`, postfijo. En LaTeX un `%` sin barra abre un comentario, así que el
    /// porcentaje llega siempre escapado.
    Percent,
    LParen,
    RParen,
    LBrace,
    RBrace,
    /// El `=` de una ecuación.
    Equals,
    /// `\{` y `\}`, las llaves de un **conjunto**.
    ///
    /// Tokens propios y no [`TokenKind::LBrace`], que es la llave de **grupo**
    /// (`x^{2}`, `\frac{1}{2}`). Las dos se escriben casi igual y significan
    /// cosas distintas, que es exactamente la familia de defectos que más ha
    /// costado en este motor: mismo dibujo, dos lecturas. Separarlas aquí hace
    /// que confundirlas sea un error de tipo y no un árbol equivocado.
    LSet,
    RSet,
}

impl TokenKind {
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Int(value) => value.to_string(),
            TokenKind::Decimal { whole, fraction } => format!("{whole}.{fraction}"),
            TokenKind::Periodic {
                whole,
                nonrepeating,
                repeating,
            } => format!("{whole}.{nonrepeating}\\overline{{{repeating}}}"),
            TokenKind::InBase { digits, base } => {
                let cifras: String = digits.iter().map(|digit| digit_char(*digit)).collect();
                format!("{cifras}_{{{base}}}")
            }
            TokenKind::Symbol(name) => name.clone(),
            TokenKind::Plus => "+".to_owned(),
            TokenKind::Minus => "-".to_owned(),
            TokenKind::Star => "*".to_owned(),
            TokenKind::Slash => "/".to_owned(),
            TokenKind::Caret => "^".to_owned(),
            TokenKind::Frac => "\\frac".to_owned(),
            TokenKind::Function(builtin) => format!("\\operatorname{{{}}}", builtin.name()),
            TokenKind::Comma => ",".to_owned(),
            TokenKind::Percent => "\\%".to_owned(),
            TokenKind::LParen => "(".to_owned(),
            TokenKind::RParen => ")".to_owned(),
            TokenKind::LBrace => "{".to_owned(),
            TokenKind::RBrace => "}".to_owned(),
            TokenKind::Equals => "=".to_owned(),
            TokenKind::LSet => "\\{".to_owned(),
            TokenKind::RSet => "\\}".to_owned(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut characters = source.char_indices().peekable();

    while let Some((offset, character)) = characters.next() {
        // El span de un carácter multibyte llega hasta su longitud real, no hasta
        // offset + 1: cortar por la mitad un carácter UTF-8 haría estallar el
        // subrayado en la UI.
        let single = Span::new(offset, offset + character.len_utf8());

        if character.is_whitespace() {
            continue;
        }

        if !character.is_ascii() {
            return Err(ParseError::new(
                ParseErrorKind::UnicodeMath {
                    text: character.to_string(),
                },
                single,
            ));
        }

        // Un número en otra base se reconoce **antes** que nada, porque sus
        // cifras pueden ser letras: sin esto, `ff_{16}` serían dos variables
        // llamadas f y un subíndice suelto.
        if let Some(token) = read_in_base(source, offset)? {
            advance_to(&mut characters, token.span.end);
            tokens.push(token);
            continue;
        }

        let kind = match character {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '^' => TokenKind::Caret,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            ',' => TokenKind::Comma,
            '=' => TokenKind::Equals,
            '\\' => {
                let (command, end) = read_while(source, offset + 1, char::is_ascii_alphabetic);
                let command_span = Span::new(offset, end);

                // Los comandos de esta gramática que no se escriben con letras,
                // así que no los encuentra `read_while`.
                if command.is_empty()
                    && let Some(kind) = escaped_symbol(source, offset + 1)
                {
                    let symbol_end = offset + 2;
                    advance_to(&mut characters, symbol_end);
                    tokens.push(Token {
                        kind,
                        span: Span::new(offset, symbol_end),
                    });
                    continue;
                }

                match command.as_str() {
                    "frac" => {
                        advance_to(&mut characters, end);
                        tokens.push(Token {
                            kind: TokenKind::Frac,
                            span: command_span,
                        });
                        continue;
                    }
                    "cdot" | "times" => {
                        advance_to(&mut characters, end);
                        tokens.push(Token {
                            kind: TokenKind::Star,
                            span: command_span,
                        });
                        continue;
                    }
                    // `\div` es la barra de dividir escrita con su glifo. El
                    // árbol no distingue una de otra: las dos acaban en
                    // `Times(a, Power(b, -1))`, porque el IR no tiene división.
                    "div" => {
                        advance_to(&mut characters, end);
                        tokens.push(Token {
                            kind: TokenKind::Slash,
                            span: command_span,
                        });
                        continue;
                    }
                    // `\operatorname{nombre}`: el nombre se resuelve **aquí**,
                    // contra la lista cerrada de `Builtin`. Un nombre que el
                    // motor no conoce se rechaza en el sitio donde se escribió,
                    // en vez de viajar como cadena hasta que alguien más abajo
                    // no sepa qué hacer con él.
                    "operatorname" => {
                        advance_to(&mut characters, end);
                        let (name, span) = read_braced_name(source, end, command_span)?;
                        let Some(builtin) = Builtin::from_name(&name) else {
                            return Err(ParseError::new(
                                ParseErrorKind::UnknownFunction { name },
                                span,
                            ));
                        };
                        advance_to(&mut characters, span.end);
                        tokens.push(Token {
                            kind: TokenKind::Function(builtin),
                            span: Span::new(offset, span.end),
                        });
                        continue;
                    }
                    // `\left(` y `\right)` sólo anuncian el tamaño del
                    // delimitador; para el árbol son un paréntesis normal.
                    "left" | "right" => {
                        advance_to(&mut characters, end);
                        let Some((delimiter_offset, delimiter)) = characters.next() else {
                            return Err(ParseError::new(
                                ParseErrorKind::ExpectedDelimiter { after: command },
                                command_span,
                            ));
                        };
                        let delimiter_span =
                            Span::new(delimiter_offset, delimiter_offset + delimiter.len_utf8());
                        // `\left\{` son dos caracteres: la barra y la llave. Se
                        // consumen los dos aquí, porque volver al bucle con la
                        // barra suelta la leería como un comando vacío.
                        if delimiter == '\\' {
                            let kind = if starts_at(source, delimiter_offset + 1, '{') {
                                TokenKind::LSet
                            } else if starts_at(source, delimiter_offset + 1, '}') {
                                TokenKind::RSet
                            } else {
                                // `\left\%` no es un delimitador. La tabla general
                                // de escapes no vale aquí: aceptaría cualquier
                                // símbolo escapado como si abriera un grupo.
                                return Err(ParseError::new(
                                    ParseErrorKind::ExpectedDelimiter { after: command },
                                    delimiter_span,
                                ));
                            };
                            let end = delimiter_offset + 2;
                            advance_to(&mut characters, end);
                            tokens.push(Token {
                                kind,
                                span: Span::new(delimiter_offset, end),
                            });
                            continue;
                        }

                        let kind = match delimiter {
                            '(' => TokenKind::LParen,
                            ')' => TokenKind::RParen,
                            _ => {
                                return Err(ParseError::new(
                                    ParseErrorKind::ExpectedDelimiter { after: command },
                                    delimiter_span,
                                ));
                            }
                        };
                        tokens.push(Token {
                            kind,
                            span: delimiter_span,
                        });
                        continue;
                    }
                    _ => {
                        return Err(ParseError::new(
                            ParseErrorKind::UnknownCommand { name: command },
                            command_span,
                        ));
                    }
                }
            }
            digit if digit.is_ascii_digit() => {
                let (whole, whole_end) = read_while(source, offset, char::is_ascii_digit);

                if !starts_at(source, whole_end, '.') {
                    advance_to(&mut characters, whole_end);
                    tokens.push(Token {
                        kind: TokenKind::Int(digits_to_integer(&whole, offset, whole_end)?),
                        span: Span::new(offset, whole_end),
                    });
                    continue;
                }

                let (fraction, end) = read_while(source, whole_end + 1, char::is_ascii_digit);

                // Detrás de las cifras puede venir un periodo: `0.1\overline{6}`.
                // Se lee aquí y no como token aparte porque un `\overline`
                // suelto no es nada — necesita la coma y las cifras de delante.
                if let Some((period, period_end)) = read_overline(source, end)? {
                    advance_to(&mut characters, period_end);
                    tokens.push(Token {
                        kind: TokenKind::Periodic {
                            whole: digits_to_integer(&whole, offset, whole_end)?,
                            nonrepeating: fraction,
                            repeating: period,
                        },
                        span: Span::new(offset, period_end),
                    });
                    continue;
                }

                // `0.` a secas no denota nada, y hacerlo pasar por `0` sería
                // inventarse cifras que el usuario no escribió.
                if fraction.is_empty() {
                    return Err(ParseError::new(
                        ParseErrorKind::IncompleteDecimal,
                        Span::new(offset, whole_end + 1),
                    ));
                }

                advance_to(&mut characters, end);
                tokens.push(Token {
                    kind: TokenKind::Decimal {
                        whole: digits_to_integer(&whole, offset, whole_end)?,
                        fraction,
                    },
                    span: Span::new(offset, end),
                });
                continue;
            }
            // Una letra es una variable. En E0 no hay nombres de varias letras:
            // `xy` es el producto de x por y, como en la notación de clase.
            letter if letter.is_ascii_alphabetic() => TokenKind::Symbol(letter.to_string()),
            other => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken {
                        found: other.to_string(),
                    },
                    single,
                ));
            }
        };

        tokens.push(Token { kind, span: single });
    }

    Ok(tokens)
}

/// Un número en otra base que empiece en `offset`: `1011_{2}`, `ff_{16}`.
///
/// `Ok(None)` es «aquí no hay uno», que es el caso normal. El error se reserva
/// para lo que **sí** es un número en base y está mal: una base imposible o una
/// cifra que en esa base no existe. `2_{2}` no es una expresión rara, es un error
/// del usuario, y decírselo es mejor que calcular otra cosa.
fn read_in_base(source: &str, offset: usize) -> Result<Option<Token>, ParseError> {
    let (text, text_end) = read_while(source, offset, char::is_ascii_alphanumeric);
    if text.is_empty() || !starts_at(source, text_end, '_') {
        return Ok(None);
    }
    if !starts_at(source, text_end + 1, '{') {
        return Err(ParseError::new(
            ParseErrorKind::ExpectedDelimiter {
                after: "_".to_owned(),
            },
            Span::new(text_end, text_end + 1),
        ));
    }

    let (base_digits, base_end) = read_while(source, text_end + 2, char::is_ascii_digit);
    if base_digits.is_empty() || !starts_at(source, base_end, '}') {
        return Err(ParseError::new(
            ParseErrorKind::ExpectedDelimiter {
                after: "_".to_owned(),
            },
            Span::new(text_end, base_end),
        ));
    }
    let span = Span::new(offset, base_end + 1);

    let base: u32 = base_digits.parse().map_err(|_| {
        ParseError::new(
            ParseErrorKind::UnsupportedBase {
                base: base_digits.clone(),
            },
            span,
        )
    })?;
    if !(MIN_BASE..=MAX_BASE).contains(&base) {
        return Err(ParseError::new(
            ParseErrorKind::UnsupportedBase { base: base_digits },
            span,
        ));
    }

    let mut digits = Vec::with_capacity(text.len());
    for digit in text.chars() {
        let Some(value) = digit.to_digit(base) else {
            return Err(ParseError::new(
                ParseErrorKind::DigitOutsideBase {
                    digit: digit.to_string(),
                    base,
                },
                span,
            ));
        };
        digits.push(u8::try_from(value).map_err(|_| {
            ParseError::new(ParseErrorKind::UnsupportedBase { base: base.to_string() }, span)
        })?);
    }

    Ok(Some(Token {
        kind: TokenKind::InBase { digits, base },
        span,
    }))
}

/// La cifra `value` como carácter, para los mensajes de error: `10` es `a`.
fn digit_char(value: u8) -> char {
    char::from_digit(u32::from(value), MAX_BASE).unwrap_or('?')
}

/// Un `\overline{cifras}` que empiece exactamente en `offset`, y dónde termina.
///
/// `Ok(None)` es «aquí no hay periodo», que es lo normal detrás de un decimal.
/// El error se reserva para un `\overline` que sí está y está mal escrito: sin
/// llaves, vacío o con algo que no son cifras. Callarse eso dejaría el
/// `\overline` como un comando desconocido y el mensaje señalaría a otra cosa.
fn read_overline(source: &str, offset: usize) -> Result<Option<(String, usize)>, ParseError> {
    const COMMAND: &str = "\\overline";

    let Some(rest) = source.get(offset..) else {
        return Ok(None);
    };
    if !rest.starts_with(COMMAND) {
        return Ok(None);
    }

    let open = offset + COMMAND.len();
    let command_span = Span::new(offset, open);
    if !starts_at(source, open, '{') {
        return Err(ParseError::new(
            ParseErrorKind::ExpectedDelimiter {
                after: "overline".to_owned(),
            },
            command_span,
        ));
    }

    let (digits, end) = read_while(source, open + 1, char::is_ascii_digit);
    if digits.is_empty() || !starts_at(source, end, '}') {
        return Err(ParseError::new(
            ParseErrorKind::EmptyPeriod,
            Span::new(offset, end),
        ));
    }
    Ok(Some((digits, end + 1)))
}

/// El `{nombre}` que sigue a un comando, y el span de las llaves incluidas.
///
/// Sólo letras: un nombre de función con dígitos o signos dentro no es un nombre,
/// y aceptarlo aquí obligaría a decidir después qué significa.
fn read_braced_name(
    source: &str,
    open: usize,
    command_span: Span,
) -> Result<(String, Span), ParseError> {
    if !starts_at(source, open, '{') {
        return Err(ParseError::new(
            ParseErrorKind::ExpectedDelimiter {
                after: "operatorname".to_owned(),
            },
            command_span,
        ));
    }

    let (name, end) = read_while(source, open + 1, char::is_ascii_alphabetic);
    if name.is_empty() || !starts_at(source, end, '}') {
        return Err(ParseError::new(
            ParseErrorKind::ExpectedDelimiter {
                after: "operatorname".to_owned(),
            },
            Span::new(open, end),
        ));
    }
    Ok((name, Span::new(open, end + 1)))
}

/// Los comandos de un solo carácter: `\%`, `\{`, `\}`.
///
/// No los encuentra `read_while`, que sólo lee letras, así que se resuelven
/// aparte. `\%` va escapado porque un `%` suelto abre un comentario de LaTeX, y
/// las llaves porque `{` sin barra es la llave de grupo, que es otra cosa.
fn escaped_symbol(source: &str, offset: usize) -> Option<TokenKind> {
    if starts_at(source, offset, '%') {
        return Some(TokenKind::Percent);
    }
    if starts_at(source, offset, '{') {
        return Some(TokenKind::LSet);
    }
    if starts_at(source, offset, '}') {
        return Some(TokenKind::RSet);
    }
    None
}

/// ¿El carácter que hay en `offset` es éste? `false` también cuando `offset` cae
/// al final de la entrada o en mitad de un carácter multibyte, que son las dos
/// formas de que ahí no haya lo que se busca.
fn starts_at(source: &str, offset: usize, character: char) -> bool {
    source
        .get(offset..)
        .is_some_and(|rest| rest.starts_with(character))
}

/// Los dígitos ya leídos, como entero. El único fallo posible es una cadena tan
/// larga que no cabe: `read_while` no deja entrar nada que no sea dígito ASCII.
fn digits_to_integer(digits: &str, start: usize, end: usize) -> Result<BigInt, ParseError> {
    digits
        .parse::<BigInt>()
        .map_err(|_| ParseError::new(ParseErrorKind::ArenaFull, Span::new(start, end)))
}

/// Lee desde `start` mientras el predicado se cumpla. Devuelve el texto y el
/// offset de byte donde paró.
fn read_while(source: &str, start: usize, predicate: fn(&char) -> bool) -> (String, usize) {
    let mut text = String::new();
    let mut end = start;
    let Some(rest) = source.get(start..) else {
        return (text, end);
    };
    for character in rest.chars() {
        if !predicate(&character) {
            break;
        }
        text.push(character);
        end += character.len_utf8();
    }
    (text, end)
}

/// Consume el iterador hasta el offset dado, que ya se leyó con `read_while`.
fn advance_to(
    characters: &mut core::iter::Peekable<core::str::CharIndices<'_>>,
    end: usize,
) {
    while let Some((offset, character)) = characters.peek() {
        if offset + character.len_utf8() > end {
            break;
        }
        characters.next();
    }
}
