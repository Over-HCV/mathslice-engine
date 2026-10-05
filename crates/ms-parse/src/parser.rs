//! Pratt parser con precedencia y multiplicación implícita.
//!
//! No hay cabeza `Divide` ni `Subtract` en el IR: `a/b` es
//! `Times(a, Power(b, -1))` y `a - b` es `Plus(a, -b)`. Menos cabezas significa
//! menos casos que enumerar en cada `match` durante los próximos años, y es la
//! razón de que `\frac` no necesite nada especial en el kernel.

use ms_core::{Arena, Builtin, CoreError, ExprId, Head, Node};
use num_bigint::BigInt;
use num_traits::Pow;

use crate::error::{ParseError, ParseErrorKind, Span};
use crate::lexer::{Token, TokenKind, tokenize};

// Fuerzas de ligado. La izquierda decide si el operador se lleva lo ya parseado;
// la derecha, cuánto se lleva por delante.
const SUM_LEFT: u8 = 10;
const SUM_RIGHT: u8 = 11;
const PRODUCT_LEFT: u8 = 20;
const PRODUCT_RIGHT: u8 = 21;
/// Más flojo que la potencia a propósito: `-x^2` es `-(x^2)`, no `(-x)^2`.
const UNARY_MINUS: u8 = 25;
const POWER_LEFT: u8 = 30;
/// Menor que `POWER_LEFT` para que `2^3^2` asocie a la derecha.
const POWER_RIGHT: u8 = 29;
/// El porcentaje liga más fuerte que todo lo demás porque es postfijo y sólo
/// afecta al número que lleva pegado: `1 + 20\%` es `1 + \frac{20}{100}`, no
/// `\frac{1 + 20}{100}`.
const PERCENT_LEFT: u8 = 40;

/// `p\%` es `\frac{p}{100}`.
const PERCENT_DENOMINATOR: i64 = 100;

/// La `d` de `\frac{d}{dx}`. Es un símbolo como cualquier otro en todas partes
/// menos ahí, y por eso el operador se reconoce por su forma completa y no por
/// esta letra suelta: `d + 1` sigue siendo una suma.
const DIFFERENTIAL: &str = "d";

/// Los tokens de `{d}{dx}`: `{`, `d`, `}`, `{`, `d`, la variable, `}`.
const DERIVATIVE_OPERATOR_TOKENS: usize = 7;

pub fn parse(arena: &mut Arena, source: &str) -> Result<ExprId, ParseError> {
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Err(ParseError::new(
            ParseErrorKind::EmptyInput,
            Span::new(0, source.len()),
        ));
    }

    let mut parser = Parser {
        tokens,
        position: 0,
        arena,
        source_len: source.len(),
    };
    let expression = parser.parse_root()?;

    if let Some(leftover) = parser.peek() {
        return Err(ParseError::new(
            ParseErrorKind::UnexpectedToken {
                found: leftover.kind.describe(),
            },
            leftover.span,
        ));
    }

    Ok(expression)
}

struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    arena: &'a mut Arena,
    source_len: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned()?;
        self.position += 1;
        Some(token)
    }

    /// Span del final de la entrada, para los errores de "termina antes de tiempo".
    fn end_span(&self) -> Span {
        Span::new(self.source_len, self.source_len)
    }

    /// Lo que puede ser una consulta entera: una expresión, una ecuación, o un
    /// conjunto de soluciones.
    ///
    /// **Los tres sólo se reconocen aquí, en la raíz**, y es una decisión
    /// restrictiva a propósito (Safety §3). `2\left\{x=2\right\}` y
    /// `\frac{x=1}{2}` se renderizarían sin protestar y no denotan nada; dejarlos
    /// entrar en la arena convierte un error del usuario en una expresión que
    /// alguien más abajo tendrá que descifrar. Abrir el permiso más adelante
    /// —para un sistema de ecuaciones, por ejemplo— es fácil; cerrarlo no.
    fn parse_root(&mut self) -> Result<ExprId, ParseError> {
        if let Some(token) = self.peek().cloned()
            && token.kind == TokenKind::LSet
        {
            self.next();
            return self.solution_set(token.span);
        }
        self.parse_relation()
    }

    /// `x^{2}-5x+6 = 0`, o la expresión sola si no hay `=`.
    ///
    /// **El `=` no es un operador del Pratt.** Como operador tendría que asociar
    /// hacia algún lado, y `x = 2 = 3` construiría `Equal(Equal(x,2), 3)`: una
    /// ecuación cuyo lado izquierdo es otra ecuación, que se escribe igual que la
    /// cadena que el usuario tecleó y significa otra cosa. Aquí es una relación
    /// no asociativa, y encadenarla es un error con su span.
    fn parse_relation(&mut self) -> Result<ExprId, ParseError> {
        let left = self.parse_expression(0)?;

        let Some(token) = self.peek().cloned() else {
            return Ok(left);
        };
        if token.kind != TokenKind::Equals {
            return Ok(left);
        }
        self.next();

        let right = self.parse_expression(0)?;

        if let Some(extra) = self.peek()
            && extra.kind == TokenKind::Equals
        {
            return Err(ParseError::new(ParseErrorKind::ChainedRelation, extra.span));
        }

        self.arena
            .app(Head::Equal, &[left, right])
            .map_err(|error| core_error(error, token.span))
    }

    /// `\left\{x = 2, x = 3\right\}`.
    ///
    /// Existe porque el escritor lo emite: una cabeza que se sabe escribir y no
    /// se sabe leer rompe `parse(latex(e)) == e`, que es la propiedad más
    /// importante del proyecto (05-testing.md §3).
    fn solution_set(&mut self, open: Span) -> Result<ExprId, ParseError> {
        let mut solutions = Vec::new();

        loop {
            solutions.push(self.parse_relation()?);

            let Some(token) = self.peek().cloned() else {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEnd,
                    self.end_span(),
                ));
            };

            if token.kind == TokenKind::RSet {
                self.next();
                break;
            }
            if token.kind != TokenKind::Comma {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken {
                        found: token.kind.describe(),
                    },
                    token.span,
                ));
            }
            self.next();
        }

        self.arena
            .app(Head::Solutions, &solutions)
            .map_err(|error| core_error(error, open))
    }

    fn parse_expression(&mut self, minimum_binding_power: u8) -> Result<ExprId, ParseError> {
        let mut left = self.parse_atom()?;

        loop {
            let Some(token) = self.peek().cloned() else {
                break;
            };

            // El porcentaje es postfijo: se lleva lo ya parseado y no pide nada
            // por delante, así que no pasa por `combine`.
            if token.kind == TokenKind::Percent {
                if PERCENT_LEFT < minimum_binding_power {
                    break;
                }
                self.next();
                left = self.percent(left, token.span)?;
                continue;
            }

            let operator = match token.kind {
                TokenKind::Plus => Operator::Add,
                TokenKind::Minus => Operator::Subtract,
                TokenKind::Star => Operator::Multiply,
                TokenKind::Slash => Operator::Divide,
                TokenKind::Caret => Operator::Power,
                // Dos átomos seguidos sin operador: producto implícito.
                TokenKind::Int(_)
                | TokenKind::Decimal { .. }
                | TokenKind::Periodic { .. }
                | TokenKind::InBase { .. }
                | TokenKind::Symbol(_)
                | TokenKind::LParen
                | TokenKind::Frac
                // `2\operatorname{redondear}(x, 1)` es un producto, como `2x`.
                | TokenKind::Function(_) => Operator::ImplicitMultiply,
                // La coma cierra el argumento de una llamada, y sólo la lee
                // `call`. Aquí termina la expresión que se estaba parseando.
                TokenKind::Comma => break,
                // Ya tratado arriba, antes de llegar aquí.
                TokenKind::Percent => break,
                // El `=` cierra el lado de la ecuación que se estaba parseando, y
                // sólo lo lee `parse_relation`. Aquí termina la expresión — y
                // donde no puede haber una ecuación, quien esperaba un cierre
                // fallará señalando el `=`, que es donde está el error.
                TokenKind::Equals => break,
                TokenKind::RParen
                | TokenKind::RBrace
                | TokenKind::LBrace
                | TokenKind::LSet
                | TokenKind::RSet => break,
            };

            if operator.left_binding_power() < minimum_binding_power {
                break;
            }

            // El producto implícito no tiene token propio que consumir.
            if operator != Operator::ImplicitMultiply {
                self.next();
            }

            let right = self.parse_expression(operator.right_binding_power())?;
            left = self.combine(operator, left, right, token.span)?;
        }

        Ok(left)
    }

    fn parse_atom(&mut self) -> Result<ExprId, ParseError> {
        let Some(token) = self.next() else {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedEnd,
                self.end_span(),
            ));
        };

        match token.kind {
            TokenKind::Int(value) => self.intern_int(value, token.span),
            TokenKind::Decimal { whole, fraction } => {
                self.decimal(&whole, &fraction, token.span)
            }
            TokenKind::Symbol(name) => self
                .arena
                .sym(&name)
                .map_err(|error| core_error(error, token.span)),
            TokenKind::Minus => {
                let operand = self.parse_expression(UNARY_MINUS)?;
                self.negate(operand, token.span)
            }
            TokenKind::LParen => {
                let inner = self.parse_expression(0)?;
                self.expect(&TokenKind::RParen, token.span)?;
                Ok(inner)
            }
            TokenKind::LBrace => {
                let inner = self.parse_expression(0)?;
                self.expect(&TokenKind::RBrace, token.span)?;
                Ok(inner)
            }
            TokenKind::Frac => {
                if let Some(variable) = self.derivative_operator() {
                    return self.derivative(variable, token.span);
                }
                let numerator = self.parse_group(token.span)?;
                let denominator = self.parse_group(token.span)?;
                self.divide(numerator, denominator, token.span)
            }
            TokenKind::Function(builtin) => self.call(builtin, token.span),
            TokenKind::Periodic {
                whole,
                nonrepeating,
                repeating,
            } => self.periodic(&whole, &nonrepeating, &repeating, token.span),
            TokenKind::InBase { digits, base } => self.in_base(&digits, base, token.span),
            TokenKind::Plus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Caret
            | TokenKind::Percent
            // Una coma fuera de una llamada no separa nada.
            | TokenKind::Comma
            | TokenKind::RParen
            | TokenKind::RBrace
            // Una ecuación y un conjunto de soluciones sólo viven en la raíz, y
            // los lee `parse_root`. Aquí dentro no son un átomo de nada.
            | TokenKind::Equals
            | TokenKind::LSet
            | TokenKind::RSet => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    found: token.kind.describe(),
                },
                token.span,
            )),
        }
    }

    /// `\operatorname{redondear}\left(x, n\right)`.
    ///
    /// La aridad se comprueba **al parsear**, contra la que declara `Builtin`.
    /// Aceptar una llamada con otro número de argumentos y dejar que una regla
    /// decida más tarde significa tener en la arena expresiones que no denotan
    /// nada, y quien las encuentre no sabrá si es un error del usuario o del
    /// motor.
    fn call(&mut self, builtin: Builtin, span: Span) -> Result<ExprId, ParseError> {
        self.expect(&TokenKind::LParen, span)?;

        let mut args = Vec::with_capacity(builtin.arity());
        loop {
            args.push(self.parse_expression(0)?);
            let Some(token) = self.peek().cloned() else {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEnd,
                    self.end_span(),
                ));
            };
            if token.kind != TokenKind::Comma {
                break;
            }
            self.next();
        }
        self.expect(&TokenKind::RParen, span)?;

        if args.len() != builtin.arity() {
            return Err(ParseError::new(
                ParseErrorKind::WrongArity {
                    name: builtin.name(),
                    expected: builtin.arity(),
                    found: args.len(),
                },
                span,
            ));
        }

        self.arena
            .app(Head::Fn(builtin), &args)
            .map_err(|error| core_error(error, span))
    }

    /// `\frac{d}{dx}` es el operador de derivar, no una fracción.
    ///
    /// Devuelve el nombre de la variable si los tokens que vienen son exactamente
    /// `{d}{d<símbolo>}`, **sin consumir nada**: quien decide es `parse_atom`, y
    /// una fracción de verdad tiene que poder seguir su camino intacta.
    ///
    /// **Por qué no puede parsearse como fracción y arreglarse después:** `d`
    /// partido por `d \cdot x` es `Times(d, Power(Times(d, x), -1))`, y en cuanto
    /// se canonicaliza, la `d` de arriba y la de abajo se cancelan. Lo que queda
    /// es `x^{-1}`: la petición desaparece y el motor se queda con una expresión
    /// que no dice que nadie pidió derivar nada.
    ///
    /// La ambigüedad con una fracción literal existe —`\frac{d}{dx}` también es
    /// «d entre d por x»— y se resuelve a favor de la derivada, que es como lo
    /// lee cualquier libro. Quien quiera la fracción tiene `\frac{d}{d \cdot x}`
    /// y `\frac{d}{x d}`.
    fn derivative_operator(&self) -> Option<String> {
        let kind = |offset: usize| self.tokens.get(self.position + offset).map(|t| &t.kind);

        let is_d = |token: Option<&TokenKind>| {
            matches!(token, Some(TokenKind::Symbol(name)) if name == DIFFERENTIAL)
        };

        if kind(0) != Some(&TokenKind::LBrace) || !is_d(kind(1)) {
            return None;
        }
        if kind(2) != Some(&TokenKind::RBrace) || kind(3) != Some(&TokenKind::LBrace) {
            return None;
        }
        if !is_d(kind(4)) || kind(6) != Some(&TokenKind::RBrace) {
            return None;
        }
        // La variable es un símbolo cualquiera y **no puede ser la `d`**: en
        // `\frac{d}{dd}` no hay forma de saber cuál de las dos es el diferencial
        // y cuál la variable, y elegir una sería adivinar.
        match kind(5) {
            Some(TokenKind::Symbol(name)) if name != DIFFERENTIAL => Some(name.clone()),
            Some(_) | None => None,
        }
    }

    /// `\frac{d}{dx}\left(f\right)` → `derivar(f, x)`.
    ///
    /// El operando se parsea con la fuerza del menos unario: se lleva la potencia
    /// —`\frac{d}{dx}x^{2}` es la derivada de `x^{2}`, no `x` derivada y luego al
    /// cuadrado— pero no el producto ni la suma que vengan detrás, que es como se
    /// lee `\frac{d}{dx}x \cdot y`.
    fn derivative(&mut self, variable: String, span: Span) -> Result<ExprId, ParseError> {
        // Los siete tokens de `{d}{dx}`, ya reconocidos por `derivative_operator`.
        self.position += DERIVATIVE_OPERATOR_TOKENS;

        let operand = self.parse_expression(UNARY_MINUS)?;
        let variable = self
            .arena
            .sym(&variable)
            .map_err(|error| core_error(error, span))?;

        self.arena
            .app(Head::Fn(Builtin::Derivative), &[operand, variable])
            .map_err(|error| core_error(error, span))
    }

    /// Un argumento de `\frac`: `{...}`, o un único átomo si no hay llaves.
    fn parse_group(&mut self, origin: Span) -> Result<ExprId, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedEnd,
                self.end_span(),
            ));
        };
        if token.kind != TokenKind::LBrace {
            return Err(ParseError::new(
                ParseErrorKind::ExpectedDelimiter {
                    after: "frac".to_owned(),
                },
                origin,
            ));
        }
        self.next();
        let inner = self.parse_expression(0)?;
        self.expect(&TokenKind::RBrace, token.span)?;
        Ok(inner)
    }

    fn expect(&mut self, expected: &TokenKind, origin: Span) -> Result<(), ParseError> {
        let Some(token) = self.next() else {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedEnd,
                self.end_span(),
            ));
        };
        if token.kind == *expected {
            return Ok(());
        }
        Err(ParseError::new(
            ParseErrorKind::MissingDelimiter {
                expected: expected.describe(),
            },
            origin,
        ))
    }

    fn combine(
        &mut self,
        operator: Operator,
        left: ExprId,
        right: ExprId,
        span: Span,
    ) -> Result<ExprId, ParseError> {
        match operator {
            Operator::Add => self
                .arena
                .plus(&[left, right])
                .map_err(|error| core_error(error, span)),
            Operator::Subtract => {
                let negated = self.negate(right, span)?;
                self.arena
                    .plus(&[left, negated])
                    .map_err(|error| core_error(error, span))
            }
            Operator::Multiply | Operator::ImplicitMultiply => self
                .arena
                .times(&[left, right])
                .map_err(|error| core_error(error, span)),
            Operator::Divide => self.divide(left, right, span),
            Operator::Power => self
                .arena
                .power(left, right)
                .map_err(|error| core_error(error, span)),
        }
    }

    /// `a / b` es `a · b^(-1)`. El IR no tiene división.
    fn divide(
        &mut self,
        numerator: ExprId,
        denominator: ExprId,
        span: Span,
    ) -> Result<ExprId, ParseError> {
        let minus_one = self.intern_int(BigInt::from(-1), span)?;
        let reciprocal = self
            .arena
            .power(denominator, minus_one)
            .map_err(|error| core_error(error, span))?;
        self.arena
            .times(&[numerator, reciprocal])
            .map_err(|error| core_error(error, span))
    }

    /// `0.75` es `\frac{75}{100}`, y **sin reducir a propósito**: la forma
    /// estructural es la que hace que «Simplificamos la fracción» aparezca como
    /// paso en vez de que el `\frac{3}{4}` salga de la nada.
    ///
    /// Es exactamente el árbol de `\frac{75}{100}` escrito a mano, así que el
    /// decimal no añade ni un nodo ni un camino nuevo al kernel: sólo una forma
    /// más de escribir lo mismo.
    fn decimal(
        &mut self,
        whole: &BigInt,
        fraction: &str,
        span: Span,
    ) -> Result<ExprId, ParseError> {
        // Cifras decimales suficientes para desbordar un u32 son ~4 GB de
        // entrada. Decir «demasiado grande» es la respuesta honesta; un cast
        // silencioso daría un número que no es el que se escribió.
        let scale = u32::try_from(fraction.len())
            .map_err(|_| ParseError::new(ParseErrorKind::ArenaFull, span))?;
        let power: BigInt = BigInt::from(10).pow(scale);
        let fraction = digits_to_integer(fraction, span)?;

        let numerator = self.intern_int(whole * &power + fraction, span)?;
        let denominator = self.intern_int(power, span)?;
        self.divide(numerator, denominator, span)
    }

    /// `0.1\overline{6}` es `\frac{15}{90}`: la receta clásica, tal cual.
    ///
    /// El numerador es «todas las cifras menos las que no se repiten» —`16 - 1`—
    /// y el denominador son tantos nueves como cifras tiene el periodo seguidos
    /// de tantos ceros como cifras hay antes de él. Sale de restar
    /// `1000x - 100x` y es lo que se enseña.
    ///
    /// **La derivación es el desazucarado, y por eso no tiene paso propio**: la
    /// fracción sin reducir `\frac{15}{90}` es el resultado de la receta, y de
    /// ahí «Simplificamos la fracción» la lleva a `\frac{1}{6}` a la vista. Es la
    /// misma decisión que con `\div`, con el decimal y con el porcentaje: una
    /// notación no gana un paso por existir. Un paso titulado exige una regla
    /// que reescriba, y para eso hace falta una cabeza que sobreviva a la
    /// canonicalización — decisión de E2, con `ms-simplify` encima de la mesa.
    fn periodic(
        &mut self,
        whole: &BigInt,
        nonrepeating: &str,
        repeating: &str,
        span: Span,
    ) -> Result<ExprId, ParseError> {
        let leading = u32::try_from(nonrepeating.len())
            .map_err(|_| ParseError::new(ParseErrorKind::ArenaFull, span))?;
        let period = u32::try_from(repeating.len())
            .map_err(|_| ParseError::new(ParseErrorKind::ArenaFull, span))?;

        // `9...90...0`: nueves por las cifras del periodo, ceros por las de
        // delante.
        let nines: BigInt = BigInt::from(10).pow(period) - 1;
        let denominator = nines * BigInt::from(10).pow(leading);

        // `todas las cifras - las que no se repiten`, con la parte entera
        // delante: para `3.1\overline{6}` es `316 - 31`.
        let all_digits = digits_to_integer(&format!("{whole}{nonrepeating}{repeating}"), span)?;
        let without_period = digits_to_integer(&format!("{whole}{nonrepeating}"), span)?;
        let numerator = all_digits - without_period;

        let numerator = self.intern_int(numerator, span)?;
        let denominator = self.intern_int(denominator, span)?;
        self.divide(numerator, denominator, span)
    }

    /// `1011_{2}` es `1 \cdot 2^{3} + 0 \cdot 2^{2} + 1 \cdot 2^{1} + 1 \cdot 2^{0}`.
    ///
    /// El desarrollo posicional **es** la conversión, así que el árbol lo lleva
    /// escrito y los pasos salen de las reglas que ya existen: la potencia y los
    /// plegados. Lo que el estudiante ve es de dónde sale el número, y no un
    /// `11` que aparece de la nada.
    ///
    /// El `0` se queda en su sitio aunque su término valga cero: quitarlo aquí
    /// sería saltarse la posición, que es justo lo que hay que enseñar. Lo quita
    /// después la regla del neutro, y eso ya es aritmética.
    ///
    /// Una sola cifra no se desarrolla: `7_{8}` es `7`, y escribir
    /// `7 \cdot 8^{0}` no explica nada que el `7` no diga.
    fn in_base(&mut self, digits: &[u8], base: u32, span: Span) -> Result<ExprId, ParseError> {
        let Some((last, leading)) = digits.split_last() else {
            return Err(ParseError::new(ParseErrorKind::UnexpectedEnd, span));
        };
        if leading.is_empty() {
            return self.intern_int(BigInt::from(*last), span);
        }

        let base_id = self.intern_int(BigInt::from(base), span)?;
        let mut terms = Vec::with_capacity(digits.len());

        for (position, digit) in digits.iter().enumerate() {
            let exponent = digits.len() - 1 - position;
            let digit = self.intern_int(BigInt::from(*digit), span)?;

            if exponent == 0 {
                terms.push(digit);
                continue;
            }
            let exponent = i64::try_from(exponent)
                .map_err(|_| ParseError::new(ParseErrorKind::ArenaFull, span))?;
            let exponent = self.intern_int(BigInt::from(exponent), span)?;
            let power = self
                .arena
                .power(base_id, exponent)
                .map_err(|error| core_error(error, span))?;
            terms.push(
                self.arena
                    .times(&[digit, power])
                    .map_err(|error| core_error(error, span))?,
            );
        }

        self.arena
            .plus(&terms)
            .map_err(|error| core_error(error, span))
    }

    /// `p\%` es `\frac{p}{100}`, por el mismo camino y por la misma razón que el
    /// decimal: sin plegar, para que la reducción se vea.
    fn percent(&mut self, value: ExprId, span: Span) -> Result<ExprId, ParseError> {
        let hundred = self.intern_int(BigInt::from(PERCENT_DENOMINATOR), span)?;
        self.divide(value, hundred, span)
    }

    /// `-a` es `Plus`-compatible: sobre un literal se pliega el signo, y sobre
    /// cualquier otra cosa se multiplica por -1. Plegar el literal es lo que
    /// hace que `x - 3` vuelva a entrar como `Plus(x, -3)` y el round-trip cierre.
    /// `-operando`.
    ///
    /// El signo se empuja **dentro** siempre que hay un número donde meterlo, en
    /// vez de añadir un factor `-1` por delante. No es cosmética: es lo que hace
    /// que `-5x` sea `Times(-5, x)` y no `Times(-1, 5, x)`, que son dos
    /// expresiones distintas que se escriben igual.
    ///
    /// Sin esto, el escritor y el parser discrepan y `parse(latex(e)) == e` se
    /// cae: `Times(-1, p)` se escribe restando, y al volver a entrar el menos
    /// unario multiplica por `-1` un producto que ya tenía su propio `1` delante,
    /// dejando un factor de más. Lo destapó la propiedad del round-trip con
    /// `0 - 1 \cdot 0^{0}`, no un caso escrito a mano.
    fn negate(&mut self, operand: ExprId, span: Span) -> Result<ExprId, ParseError> {
        match self.arena.get(operand) {
            Some(Node::Int(value)) => {
                let negated = -value.clone();
                return self.intern_int(negated, span);
            }
            Some(Node::Rat(value)) => {
                let negated = -value.clone();
                return self
                    .arena
                    .rat(negated)
                    .map_err(|error| core_error(error, span));
            }
            Some(Node::App { head, .. }) if *head == Head::Times => {
                if let Some(negated) = self.negate_leading_factor(operand, span)? {
                    return Ok(negated);
                }
            }
            Some(Node::Sym(_) | Node::App { .. }) | None => {}
        }

        let minus_one = self.intern_int(BigInt::from(-1), span)?;
        self.arena
            .times(&[minus_one, operand])
            .map_err(|error| core_error(error, span))
    }

    /// Un producto con su primer factor numérico cambiado de signo, o `None` si
    /// no lo tiene.
    ///
    /// Mira el **primer** factor porque es donde el orden canónico deja los
    /// números: `Int` y `Rat` son los rangos 0 y 1. Un producto sin número
    /// delante no tiene dónde meter el signo y vuelve a la vía general.
    ///
    /// Baja por la espina izquierda porque el parser todavía no ha aplanado
    /// nada: `1 \cdot 0 \cdot 0` es `Times(Times(1, 0), 0)`, y el literal que
    /// hay que negar está dos niveles abajo. Aplanar aquí sería canonicalizar a
    /// medias en mitad del parseo.
    fn negate_leading_factor(
        &mut self,
        product: ExprId,
        span: Span,
    ) -> Result<Option<ExprId>, ParseError> {
        let Some(args) = self.arena.app_args(product).map(<[ExprId]>::to_vec) else {
            return Ok(None);
        };
        let Some(first) = args.first().copied() else {
            return Ok(None);
        };
        let negatable = match self.arena.get(first) {
            Some(Node::Int(_) | Node::Rat(_)) => true,
            Some(Node::App { head, .. }) => *head == Head::Times,
            Some(Node::Sym(_)) | None => false,
        };
        if !negatable {
            return Ok(None);
        }

        let negated_first = self.negate(first, span)?;
        // Se construye una lista nueva: la arena comparte estructura, así que
        // tocar la del producto original lo cambiaría también donde ya esté
        // usado (§2, insertar nunca mutar).
        let mut rebuilt = Vec::with_capacity(args.len());
        rebuilt.push(negated_first);
        rebuilt.extend(args.iter().skip(1).copied());

        self.arena
            .times(&rebuilt)
            .map(Some)
            .map_err(|error| core_error(error, span))
    }

    fn intern_int(&mut self, value: BigInt, span: Span) -> Result<ExprId, ParseError> {
        self.arena
            .int(value)
            .map_err(|error| core_error(error, span))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
    Add,
    Subtract,
    Multiply,
    ImplicitMultiply,
    Divide,
    Power,
}

impl Operator {
    fn left_binding_power(self) -> u8 {
        match self {
            Operator::Add | Operator::Subtract => SUM_LEFT,
            Operator::Multiply | Operator::ImplicitMultiply | Operator::Divide => PRODUCT_LEFT,
            Operator::Power => POWER_LEFT,
        }
    }

    fn right_binding_power(self) -> u8 {
        match self {
            Operator::Add | Operator::Subtract => SUM_RIGHT,
            Operator::Multiply | Operator::ImplicitMultiply | Operator::Divide => PRODUCT_RIGHT,
            Operator::Power => POWER_RIGHT,
        }
    }
}

/// Las cifras decimales, como entero. El lexer ya garantizó que son dígitos
/// ASCII, así que lo único que puede fallar es que no quepan.
fn digits_to_integer(digits: &str, span: Span) -> Result<BigInt, ParseError> {
    digits
        .parse::<BigInt>()
        .map_err(|_| ParseError::new(ParseErrorKind::ArenaFull, span))
}

fn core_error(error: CoreError, span: Span) -> ParseError {
    match error {
        CoreError::ArenaFull => ParseError::new(ParseErrorKind::ArenaFull, span),
        // El parser construye todos sus ids sobre la misma arena, así que estos
        // no ocurren. Reportarlos como «demasiado grande» sería más cómodo y
        // sería mentira: el usuario leería una causa que no es la suya.
        CoreError::UnknownExpr(_)
        | CoreError::UnknownSymbol(_)
        | CoreError::IncomparableNodes(_, _) => ParseError::new(
            ParseErrorKind::Internal {
                detail: error.to_string(),
            },
            span,
        ),
    }
}
