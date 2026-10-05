//! Errores de parseo, con span sobre el LaTeX original.
//!
//! El span existe para que la UI pueda subrayar el error **donde el usuario lo
//! escribió**. Un "error de sintaxis" sin posición obliga a adivinar.

/// Rango de bytes sobre la cadena LaTeX de entrada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParseErrorKind {
    EmptyInput,
    UnexpectedEnd,
    UnexpectedToken { found: String },
    UnknownCommand { name: String },
    /// `\operatorname{loquesea}` con un nombre que el motor no conoce. Distinto
    /// de `UnknownCommand`: el comando existe y está bien escrito, la función no.
    UnknownFunction { name: String },
    /// Una llamada con un número de argumentos que no es el de esa función. Se
    /// comprueba al parsear: una llamada con otra aridad no denota nada, y
    /// dejarla entrar en la arena convierte un error del usuario en un misterio.
    WrongArity {
        name: &'static str,
        expected: usize,
        found: usize,
    },
    /// Una base que no se puede escribir: menor que 2, mayor que 36, o tan
    /// grande que no cabe. En base 1 no hay notación posicional, y por encima de
    /// 36 se acaban las letras.
    UnsupportedBase { base: String },
    /// Una cifra que en esa base no existe: el `2` de `2_{2}`. No es una
    /// expresión rara, es un error, y calcular otra cosa sería peor.
    DigitOutsideBase { digit: String, base: u32 },
    /// `\overline{}` sin cifras dentro, o con algo que no son cifras. Un periodo
    /// vacío no es un número: `0.\overline{}` no dice qué se repite.
    EmptyPeriod,
    /// Un punto decimal sin cifras detrás. No se completa con un cero: el motor
    /// no inventa cifras que el usuario no escribió, y `0.` puede ser tanto un
    /// dedo resbalado como un `0.\overline{3}` a medio escribir.
    IncompleteDecimal,
    /// La entrada del motor es LaTeX y sólo LaTeX (02-architecture.md §2): `√` y
    /// `∫` son glifos que dibuja la interfaz, no sintaxis de entrada.
    UnicodeMath { text: String },
    /// `x = 2 = 3`. Una cadena de igualdades no es una ecuación con tres lados:
    /// o son dos ecuaciones, o es una afirmación sobre tres cosas que este motor
    /// todavía no sabe representar. Encadenarlas construiría `Equal(Equal(x,2),3)`,
    /// que se escribiría igual y significaría otra cosa.
    ChainedRelation,
    /// `\left` o `\right` sin delimitador detrás. `after` es el comando.
    ExpectedDelimiter { after: String },
    /// Falta el cierre que un grupo ya abierto necesita: el `)` de `(x + 1`, la
    /// llave de `\frac 2 3`.
    ///
    /// Separado de [`ParseErrorKind::ExpectedDelimiter`] porque el campo de aquél
    /// decía «después de» y guardaba **lo que falta**, así que `(x = 2)` contaba
    /// «falta un delimitador después de «)»» — con el `)` delante de las narices
    /// del usuario. Un solo campo con dos significados es cómo un mensaje de
    /// error acaba mintiendo.
    MissingDelimiter { expected: String },
    ArenaFull,
    /// El IR devolvió un fallo que no es «no cabe»: un id de otra arena, o dos
    /// nodos incomparables. Inalcanzable desde el parser, que construye todos
    /// sus ids sobre la misma arena — pero si se alcanzara, decir «la expresión
    /// es demasiado grande» sería contarle al usuario una causa falsa.
    Internal { detail: String },
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Identificador estable para la UI y la telemetría.
    pub fn kind_id(&self) -> &'static str {
        match self.kind {
            ParseErrorKind::EmptyInput => "empty-input",
            ParseErrorKind::UnexpectedEnd => "unexpected-end",
            ParseErrorKind::UnexpectedToken { .. } => "unexpected-token",
            ParseErrorKind::UnknownCommand { .. } => "unknown-command",
            ParseErrorKind::UnknownFunction { .. } => "unknown-function",
            ParseErrorKind::WrongArity { .. } => "wrong-arity",
            ParseErrorKind::UnsupportedBase { .. } => "unsupported-base",
            ParseErrorKind::DigitOutsideBase { .. } => "digit-outside-base",
            ParseErrorKind::EmptyPeriod => "empty-period",
            ParseErrorKind::IncompleteDecimal => "incomplete-decimal",
            ParseErrorKind::UnicodeMath { .. } => "unicode-math",
            ParseErrorKind::ChainedRelation => "chained-relation",
            ParseErrorKind::ExpectedDelimiter { .. } => "expected-delimiter",
            ParseErrorKind::MissingDelimiter { .. } => "missing-delimiter",
            ParseErrorKind::ArenaFull => "arena-full",
            ParseErrorKind::Internal { .. } => "internal",
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.kind {
            ParseErrorKind::EmptyInput => write!(f, "No hay nada que calcular"),
            ParseErrorKind::UnexpectedEnd => write!(f, "La expresión termina antes de tiempo"),
            ParseErrorKind::UnexpectedToken { found } => {
                write!(f, "No esperaba «{found}» aquí")
            }
            ParseErrorKind::UnknownCommand { name } => {
                write!(f, "El comando «\\{name}» todavía no está soportado")
            }
            ParseErrorKind::UnknownFunction { name } => {
                write!(f, "El motor no conoce la función «{name}»")
            }
            ParseErrorKind::WrongArity {
                name,
                expected,
                found,
            } => write!(
                f,
                "{name} toma {expected} argumentos, y aquí tiene {found}"
            ),
            ParseErrorKind::UnsupportedBase { base } => write!(
                f,
                "La base {base} no se puede escribir: van de la 2 a la 36"
            ),
            ParseErrorKind::DigitOutsideBase { digit, base } => {
                write!(f, "La cifra «{digit}» no existe en base {base}")
            }
            ParseErrorKind::EmptyPeriod => {
                write!(f, "Un periodo necesita cifras: escribe qué se repite")
            }
            ParseErrorKind::IncompleteDecimal => {
                write!(f, "Un decimal necesita cifras después del punto")
            }
            ParseErrorKind::UnicodeMath { text } => write!(
                f,
                "«{text}» es un símbolo unicode: escribe el comando LaTeX equivalente"
            ),
            ParseErrorKind::ChainedRelation => write!(
                f,
                "Una ecuación tiene dos lados: escribe un solo «=»"
            ),
            ParseErrorKind::ExpectedDelimiter { after } => {
                write!(f, "Falta un delimitador después de «{after}»")
            }
            ParseErrorKind::MissingDelimiter { expected } => {
                write!(f, "Falta un «{expected}» que cierre lo que se abrió aquí")
            }
            ParseErrorKind::ArenaFull => write!(f, "La expresión es demasiado grande"),
            ParseErrorKind::Internal { detail } => {
                write!(f, "Fallo interno del motor: {detail}")
            }
        }
    }
}

impl core::error::Error for ParseError {}
