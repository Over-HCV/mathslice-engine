//! `ms-verify` — comprobar el resultado por un camino que no es el que lo produjo.
//!
//! **El peor fallo de un CAS no es el crash, es la respuesta incorrecta**
//! (01-rust-principles.md §9). El reescritor puede equivocarse en silencio: una
//! regla mal escrita devuelve un valor perfectamente plausible y nada falla. La
//! única defensa es comprobar la respuesta con código distinto del que la
//! calculó, y por eso este crate **no depende de `ms-rules`** — `deny.toml` lo
//! hace cumplir, no un acuerdo entre personas.
//!
//! # Nivel 0 en E1: exacto, no probabilístico
//!
//! `03-lean-verification.md` describe el nivel 0 como muestreo numérico. Eso es
//! lo que aplica **cuando hay variables libres**, que es E2. En E1 no las hay:
//! comprobar que `\frac{7}{12} \div \frac{14}{9} + 2^{40}` vale
//! `\frac{8796093022211}{8}` es evaluar las dos expresiones en ℚ y comparar dos
//! racionales exactos. Determinista, total y sin falsos positivos.
//!
//! # Por qué `Verified<T>` y no un booleano
//!
//! Un booleano se puede ignorar. [`Verified`] no: es la única forma de que un
//! valor llegue al documento de salida, y la única forma de obtener uno es haber
//! pasado por [`check`]. Así la invariante 3 de 02-architecture.md §4 —lo que no
//! se pudo verificar sale **marcado**, no oculto— deja de ser una convención de
//! documento y pasa a ser un error de compilación (§3).

mod dual;

use ms_core::{Arena, Builtin, ExprId, Head, Node, SymId};
use ms_num::{Rational, checked_pow, gcd, round_half_away, round_half_even};
use num_traits::{One, Signed, ToPrimitive, Zero};

/// Qué se pudo afirmar del resultado.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Se comprobó y coincide.
    Verified,
    /// Se comprobó y **no** coincide. Es el caso que justifica el crate entero:
    /// el motor devolvió un valor y la comprobación dice que está mal.
    Failed,
    /// No se pudo comprobar: la expresión no es cerrada, o su magnitud excede lo
    /// que se puede evaluar. No saber no es lo mismo que estar mal.
    Unverified,
}

/// Cómo se comprobó. Sube al documento para que el lector sepa qué vale la
/// afirmación: «evaluado en ℚ» y «muestreado en 30 puntos» no son lo mismo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    /// Evaluación exacta en los racionales, sin variables libres. **Nivel 0.**
    ExactRational,
    /// Cada solución sustituida en la ecuación original, y los dos lados
    /// evaluados en ℚ. **Nivel 1.**
    ///
    /// # Qué afirma, y qué no
    ///
    /// Afirma que **cada solución que se devolvió cumple la ecuación**. Eso es
    /// lo que atrapa el fallo probable —una raíz mal despejada, un signo
    /// cambiado— y lo hace por un camino que no es el que la encontró: aquí no
    /// se factoriza nada, se sustituye y se evalúa.
    ///
    /// **No afirma que no falte ninguna.** Comprobar que están todas exigiría
    /// dividir por los factores hallados y buscar raíces en el cociente, que es
    /// exactamente el código que produjo la respuesta; hacerlo aquí convertiría
    /// la comprobación en un eco. Es preferible sellar menos y que el sello
    /// signifique algo (§9).
    SymbolicSubstitution,
    /// Entrada y salida evaluadas en varios puntos racionales y comparadas.
    /// **Nivel 0**, que es donde `03-lean-verification.md` sitúa el muestreo.
    ///
    /// Es lo que aplica cuando hay una variable libre y por tanto no hay un
    /// número que comparar. Que `\operatorname{expandir}` y
    /// `\operatorname{factorizar}` no cambian el valor es precisamente lo que
    /// esto comprueba, y por un camino que no factoriza nada.
    ///
    /// **No es una demostración**, y por eso es nivel 0 y no 1: dos expresiones
    /// distintas pueden coincidir en los puntos que se prueben. Para un
    /// polinomio de grado `d`, coincidir en `d+1` puntos sí lo demostraría —pero
    /// este crate no sabe que lo que tiene delante es un polinomio, y averiguarlo
    /// sería traerse `ms-alg`, que es medio camino hacia comprobar la respuesta
    /// con el código que la produjo.
    ///
    /// Los puntos son **fijos**, no aleatorios: un sello que no se puede
    /// reproducir no es un sello.
    RationalSampling,
    /// Como [`Method::RationalSampling`], pero la entrada pedía una derivada y
    /// el valor de la entrada en cada punto se obtuvo **derivando por números
    /// duales**: evaluando en `a + \varepsilon` con `\varepsilon^{2} = 0`.
    /// **Nivel 0.**
    ///
    /// # Por qué es un método aparte y no el muestreo de siempre
    ///
    /// Porque afirma algo distinto y hay que poder leerlo en el sello. El
    /// muestreo corriente dice «entrada y salida valen lo mismo»; éste dice «la
    /// derivada que devolvió el motor coincide con la verdadera», y la verdadera
    /// la calculó aritmética dual, que **no deriva**: sale de sumar y multiplicar.
    /// `deny.toml` impide además que este crate vea `ms-diff`, así que no puede
    /// ser un eco de las reglas que produjeron la respuesta.
    ///
    /// Sigue siendo nivel 0: coincidir en doce puntos no es una demostración.
    DualSampling,
}

impl Method {
    /// El nivel al que corresponde este método.
    ///
    /// El nivel **se deriva del método** en vez de escribirse: eran dos datos
    /// que decían lo mismo, y un `level: 0` literal junto a un método de nivel 1
    /// es la clase de contradicción que nadie nota hasta que un documento la
    /// enseña.
    fn level(self) -> u8 {
        match self {
            Method::ExactRational | Method::RationalSampling | Method::DualSampling => 0,
            Method::SymbolicSubstitution => 1,
        }
    }
}

/// El certificado. Sus campos son privados: la única forma de obtener uno es
/// [`check`], que es lo que impide fabricar una verificación que nadie hizo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Evidence {
    level: u8,
    status: Status,
    method: Method,
}

impl Evidence {
    pub fn level(self) -> u8 {
        self.level
    }

    pub fn status(self) -> Status {
        self.status
    }

    pub fn method(self) -> Method {
        self.method
    }

    /// Ata un valor a este certificado. Es el único constructor de [`Verified`].
    pub fn seal<T>(self, value: T) -> Verified<T> {
        Verified {
            value,
            evidence: self,
        }
    }
}

/// Un valor que pasó por [`check`], con el certificado de lo que se pudo afirmar.
///
/// Newtype de coste cero: en tiempo de ejecución es el valor y tres bytes de
/// certificado. Lo que aporta es en tiempo de compilación.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Verified<T> {
    value: T,
    evidence: Evidence,
}

impl<T> Verified<T> {
    pub fn evidence(&self) -> Evidence {
        self.evidence
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn into_parts(self) -> (T, Evidence) {
        (self.value, self.evidence)
    }
}

/// Comprueba que reescribir `input` hasta `output` conservó el valor.
///
/// Es la puerta de entrada al crate y el único constructor de [`Evidence`].
///
/// Los cuatro desenlaces, y por qué cada uno es el honesto:
///
/// | `input` | `output` | Resultado | |
/// |---|---|---|---|
/// | vale `v` | vale `v` | `Verified` | dos caminos, mismo número |
/// | vale `v` | vale `w ≠ v` | `Failed` | el motor se equivocó, y se dice |
/// | vale `v` | no evalúa | `Failed` | había un número y la derivación lo perdió |
/// | no evalúa | cualquiera | `Unverified` | hay símbolos: es E2, con muestreo |
pub fn check(arena: &Arena, input: ExprId, output: ExprId) -> Evidence {
    // Una ecuación resuelta no se comprueba evaluando: no tiene valor, tiene
    // soluciones. Se intenta primero porque el nivel 0 contestaría
    // `Unverified` sobre ella —hay una variable libre— y eso taparía una
    // comprobación que sí se puede hacer.
    if let Some(status) = check_solutions(arena, input, output) {
        return Evidence {
            level: Method::SymbolicSubstitution.level(),
            status,
            method: Method::SymbolicSubstitution,
        };
    }

    let status = match (evaluate(arena, input), evaluate(arena, output)) {
        (Some(before), Some(after)) if before == after => Status::Verified,
        (Some(_), Some(_) | None) => Status::Failed,
        (None, Some(_) | None) => {
            // Sin valor que comparar queda el muestreo, que es lo que aplica
            // cuando hay una variable libre. Si tampoco se puede, se dice que no
            // se sabe.
            if let Some(status) = check_by_sampling(arena, input, output) {
                // El método se elige por lo que hubo que hacer para conocer el
                // valor de la entrada, que es lo que el sello afirma. Con una
                // derivada dentro, ese valor salió de la aritmética dual.
                let method = if contains_derivative(arena, input) {
                    Method::DualSampling
                } else {
                    Method::RationalSampling
                };
                return Evidence {
                    level: method.level(),
                    status,
                    method,
                };
            }
            Status::Unverified
        }
    };

    Evidence {
        level: Method::ExactRational.level(),
        status,
        method: Method::ExactRational,
    }
}

/// Los puntos en los que se muestrea, en orden.
///
/// Fijos y no aleatorios: un sello que sale distinto en cada ejecución no se
/// puede reproducir, y entonces no sirve para nada cuando alguien discute un
/// resultado. Primos y sus negativos porque es poco probable que anulen un
/// denominador, y dos fraccionarios porque una expresión puede coincidir con
/// otra en todos los enteros y no en ℚ.
const SAMPLE_POINTS: [(i64, i64); 12] = [
    (2, 1),
    (3, 1),
    (5, 1),
    (7, 1),
    (-2, 1),
    (-3, 1),
    (-5, 1),
    (11, 1),
    (13, 1),
    (-7, 1),
    (1, 2),
    (-1, 3),
];

/// Cuántos puntos tienen que evaluarse para que el muestreo afirme algo.
///
/// Menos de esto y lo que hay no es evidencia: una expresión que sólo se puede
/// evaluar en dos puntos coincide con demasiadas cosas. Los que fallan son los
/// que anulan un denominador, y saltarlos es correcto —ahí la expresión no
/// denota nada— pero saltarlos casi todos deja de ser una comprobación.
const MINIMUM_SAMPLES: usize = 6;

/// Nivel 0 con una variable libre: evaluar los dos lados en varios puntos.
///
/// `None` cuando no hay exactamente una variable libre. Con ninguna manda la
/// evaluación exacta, y con dos o más habría que recorrer un producto de puntos:
/// es E3, y afirmar algo ligando sólo una sería afirmar de más.
fn check_by_sampling(arena: &Arena, input: ExprId, output: ExprId) -> Option<Status> {
    let symbol = single_free_symbol(arena, input)?;
    // Una salida con otra variable no es una reescritura de esta entrada.
    match single_free_symbol(arena, output) {
        Some(other) if other != symbol => return None,
        Some(_) | None => {}
    }

    let mut agreed = 0_usize;
    for (numerator, denominator) in SAMPLE_POINTS {
        let point = Rational::new(numerator.into(), denominator.into());
        let binding = Some((symbol, &point));

        let (Some(before), Some(after)) = (
            evaluate_bound(arena, input, binding),
            evaluate_bound(arena, output, binding),
        ) else {
            continue;
        };
        if before != after {
            return Some(Status::Failed);
        }
        agreed += 1;
    }

    if agreed < MINIMUM_SAMPLES {
        return None;
    }
    Some(Status::Verified)
}

/// ¿Se pidió derivar en algún sitio de esta expresión?
///
/// Decide qué método se declara en el sello, no si se verifica: lo segundo ya lo
/// dijo el muestreo.
fn contains_derivative(arena: &Arena, id: ExprId) -> bool {
    if arena.head_of(id) == Some(Head::Fn(Builtin::Derivative)) {
        return true;
    }
    match arena.get(id) {
        Some(Node::App { args, .. }) => arena.args_of(*args).is_some_and(|args| {
            args.to_vec()
                .into_iter()
                .any(|arg| contains_derivative(arena, arg))
        }),
        Some(Node::Int(_) | Node::Rat(_) | Node::Sym(_)) | None => false,
    }
}

/// La única variable libre de una expresión, si hay exactamente una.
fn single_free_symbol(arena: &Arena, id: ExprId) -> Option<SymId> {
    let mut found = None;
    collect_symbol(arena, id, &mut found)?;
    found
}

/// Recorre buscando símbolos. `None` en cuanto aparece un segundo distinto.
fn collect_symbol(arena: &Arena, id: ExprId, found: &mut Option<SymId>) -> Option<()> {
    match arena.get(id)? {
        Node::Int(_) | Node::Rat(_) => Some(()),
        Node::Sym(symbol) => match found {
            Some(known) if known != symbol => None,
            Some(_) => Some(()),
            None => {
                *found = Some(*symbol);
                Some(())
            }
        },
        Node::App { args, .. } => {
            for arg in arena.args_of(*args)?.to_vec() {
                collect_symbol(arena, arg, found)?;
            }
            Some(())
        }
    }
}

/// Nivel 1: sustituye cada solución en la ecuación de entrada.
///
/// `None` cuando esto no es «una ecuación que salió resuelta», y entonces el
/// nivel 0 sigue su curso. Es una pregunta y no un intento: sustituir en algo
/// que no es una ecuación no significa nada.
fn check_solutions(arena: &Arena, input: ExprId, output: ExprId) -> Option<Status> {
    let [left, right] = equal_sides(arena, input)?;
    let solutions = solution_set(arena, output)?;

    // Un conjunto vacío no se sella: no hay nada que comprobar, y decir
    // «verificado» de la nada es justo lo que este crate existe para impedir.
    if solutions.is_empty() {
        return None;
    }

    for solution in solutions {
        let [unknown, value] = equal_sides(arena, solution)?;
        let Node::Sym(unknown) = arena.get(unknown)? else {
            return None;
        };
        // El valor de una solución tiene que ser un número cerrado. Si no lo es
        // —`x = y`, `x = \sqrt{2}` el día que existan— sustituirlo no deja una
        // expresión que se pueda evaluar en ℚ, y no saber es la respuesta.
        let value = evaluate(arena, value)?;

        let binding = (*unknown, &value);
        let (Some(left), Some(right)) = (
            evaluate_bound(arena, left, Some(binding)),
            evaluate_bound(arena, right, Some(binding)),
        ) else {
            return None;
        };

        // Una sola solución que no cumple invalida la respuesta entera: el
        // motor devolvió algo que no resuelve la ecuación que le dieron.
        if left != right {
            return Some(Status::Failed);
        }
    }

    Some(Status::Verified)
}

/// Los dos lados de una ecuación, si `id` lo es.
fn equal_sides(arena: &Arena, id: ExprId) -> Option<[ExprId; 2]> {
    let Node::App { head, args } = arena.get(id)? else {
        return None;
    };
    if *head != Head::Equal {
        return None;
    }
    let [left, right] = arena.args_of(*args)? else {
        return None;
    };
    Some([*left, *right])
}

/// Las ecuaciones de un conjunto de soluciones.
fn solution_set(arena: &Arena, id: ExprId) -> Option<Vec<ExprId>> {
    let Node::App { head, args } = arena.get(id)? else {
        return None;
    };
    if *head != Head::Solutions {
        return None;
    }
    Some(arena.args_of(*args)?.to_vec())
}

/// El valor exacto de una expresión cerrada, o `None` si no lo tiene.
///
/// `None` cubre tres cosas distintas a propósito —hay un símbolo, hay una
/// división entre cero, la magnitud excede el tope— porque para quien pregunta
/// las tres significan lo mismo: aquí no hay un número que comparar.
///
/// Se parece a `rational_value` de `ms-rules`, y tiene que parecerse: son dos
/// implementaciones del mismo hecho matemático en crates que no se conocen. Esa
/// duplicación **es** el mecanismo (principio 12); unificarlas destruiría la
/// independencia que hace que la comprobación signifique algo.
pub fn evaluate(arena: &Arena, id: ExprId) -> Option<Rational> {
    evaluate_bound(arena, id, None)
}

/// Como [`evaluate`], con una variable ligada a un valor.
///
/// Es lo que hace posible el nivel 1: sustituir **no construye nada** en la
/// arena, se evalúa leyendo el símbolo como si fuera su valor. Así la
/// comprobación no puede ensuciar el árbol que está comprobando, y sigue siendo
/// una función pura sobre él.
pub fn evaluate_bound(
    arena: &Arena,
    id: ExprId,
    binding: Option<(SymId, &Rational)>,
) -> Option<Rational> {
    let node = arena.get(id)?;

    match node {
        Node::Int(value) => Some(Rational::from(value.clone())),
        Node::Rat(value) => Some(value.clone()),
        Node::Sym(symbol) => match binding {
            Some((bound, value)) if bound == *symbol => Some(value.clone()),
            // Un símbolo sin ligar sigue sin tener valor. Ligar uno no autoriza
            // a inventarse los demás.
            Some(_) | None => None,
        },
        Node::App { head, args } => {
            let args = arena.args_of(*args)?;
            match head {
                Head::Plus => fold(arena, args, binding, Rational::zero(), |sum, term| {
                    Some(sum + term)
                }),
                Head::Times => fold(arena, args, binding, Rational::one(), |product, factor| {
                    Some(product * factor)
                }),
                Head::Fn(builtin) => evaluate_call(arena, *builtin, args, binding),
                Head::Power => {
                    let [base, exponent] = args else {
                        return None;
                    };
                    let base = evaluate_bound(arena, *base, binding)?;
                    let exponent = evaluate_bound(arena, *exponent, binding)?;
                    // Un exponente fraccionario sale de la aritmética exacta en ℚ:
                    // `2^{1/2}` es irracional y su verificación es E5, no ésta.
                    if !exponent.denom().is_one() {
                        return None;
                    }
                    checked_pow(&base, exponent.numer())
                }
                // Una ecuación no tiene valor: tiene soluciones, y comprobarlas
                // es otra clase de verificación —sustituir cada raíz y ver que
                // los dos lados coinciden— que vive en `check_solutions`. Este
                // evaluador contesta lo que sabe: aquí no hay un número que
                // comparar. Devolver el valor de un lado, o su diferencia,
                // afirmaría algo que nadie ha comprobado.
                Head::Equal | Head::Solutions => None,
            }
        }
    }
}

/// El valor exacto de una llamada a función.
///
/// **El redondeo lo hace `ms-num`, no este módulo.** Es la única parte de este
/// crate que no reimplementa lo que comprueba, y es a propósito: el reescritor y
/// el verificador tienen que redondear con el mismo criterio de desempate, y dos
/// copias del `.5` acabarían discrepando en el único caso interesante. Es el
/// mismo reparto que [`checked_pow`], que también usan los dos.
///
/// Lo que sigue estando separado es lo que importa: aquí se **evalúa** el árbol
/// y allí se **reescribe**, `deny.toml` impide que este crate vea `ms-rules`, y
/// una regla de redondeo que se dispare donde no debe sale marcada igual.
fn evaluate_call(
    arena: &Arena,
    builtin: Builtin,
    args: &[ExprId],
    binding: Option<(SymId, &Rational)>,
) -> Option<Rational> {
    // `enBase` no es una operación: pide una notación de salida y la frontera la
    // quita del árbol antes de reescribir, así que aquí no llega nunca. Si algún
    // día llegara —anidada dentro de una expresión, que es un error que la
    // frontera rechaza—, no afirmar nada de ella es la respuesta honesta.
    if builtin == Builtin::InBase {
        return None;
    }

    // **Una derivada sí tiene valor en un punto**, y se calcula sin derivar: se
    // evalúa el operando en números duales y lo que sale en la parte dual es
    // `f'(a)` exacto. Ver `dual.rs`.
    //
    // Vive aquí, dentro del evaluador corriente, y no en una comprobación aparte:
    // así `\frac{d}{dx}\left(x^{2}\right)` y `2 + \frac{d}{dx}\left(x^{2}\right)`
    // se comprueban por el mismo camino, en vez de que el segundo salga sin
    // verificar por estar la derivada un nivel más abajo.
    if builtin == Builtin::Derivative {
        let [operand, variable] = args else {
            return None;
        };
        let Node::Sym(variable) = arena.get(*variable)? else {
            return None;
        };
        // Sin punto no hay dónde evaluar: la derivada de una expresión con
        // variable libre no es un número. Es el mismo «no se sabe» que devuelve
        // un símbolo sin ligar.
        let (bound, point) = binding?;
        if bound != *variable {
            return None;
        }
        return dual::differentiate(arena, *operand, *variable, point);
    }

    // **Expandir y factorizar no cambian el valor**, y de ahí sale su
    // verificación: `expandir(p)` vale exactamente lo que vale `p`, así que
    // comprobar que la expansión es correcta es comprobar que los dos lados
    // valen lo mismo. Con una variable libre no hay valor y sale sin verificar,
    // que es lo que el nivel 1 viene a arreglar.
    if matches!(builtin, Builtin::Expand | Builtin::Factor) {
        let [only] = args else {
            return None;
        };
        return evaluate_bound(arena, *only, binding);
    }

    let [left, right] = args else {
        return None;
    };

    // El MCD se calcula con `ms_num::gcd`, el mismo que usa la regla. Es el
    // reparto de `checked_pow` y del redondeo: lo que se comparte es una
    // primitiva aritmética, no el camino que produjo el resultado. La diferencia
    // importa — el desempate de un redondeo es una convención y dos copias
    // discreparían, pero el máximo común divisor de dos enteros es un hecho.
    if builtin == Builtin::Gcd {
        let (Node::Int(left), Node::Int(right)) = (arena.get(*left)?, arena.get(*right)?) else {
            return None;
        };
        return Some(Rational::from(gcd(left, right).abs()));
    }

    let value = evaluate_bound(arena, *left, binding)?;
    let digits = decimal_places(&evaluate_bound(arena, *right, binding)?)?;

    match builtin {
        Builtin::Round => round_half_away(&value, digits),
        Builtin::RoundHalfEven => round_half_even(&value, digits),
        // Ya se atendieron arriba, antes de mirar los argumentos: los de `enBase`
        // van al revés que los del redondeo y leerlos aquí sería redondear al
        // revés, y los de las demás no son un valor y unas cifras.
        Builtin::InBase
        | Builtin::Expand
        | Builtin::Factor
        | Builtin::Gcd
        | Builtin::Derivative => None,
    }
}

/// El segundo argumento de un redondeo: un número entero de cifras.
///
/// `2.5` cifras no significa nada, y redondear a «unas cuantas» no es una
/// operación. `None` deja el resultado sin verificar, que es lo correcto: la
/// expresión no denota un número.
fn decimal_places(value: &Rational) -> Option<i32> {
    if !value.is_integer() {
        return None;
    }
    value.to_integer().to_i32()
}

fn fold(
    arena: &Arena,
    args: &[ExprId],
    binding: Option<(SymId, &Rational)>,
    initial: Rational,
    combine: fn(Rational, Rational) -> Option<Rational>,
) -> Option<Rational> {
    let mut accumulated = initial;
    for arg in args {
        accumulated = combine(accumulated, evaluate_bound(arena, *arg, binding)?)?;
    }
    Some(accumulated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ms_num::Integer;

    fn rational(numerator: i64, denominator: i64) -> Rational {
        Rational::new(Integer::from(numerator), Integer::from(denominator))
    }

    /// `\frac{1}{2} + \frac{1}{3}` tal y como sale del parser: sin `Node::Rat`,
    /// que sólo existe reducido, sino con la forma estructural.
    fn fraction(arena: &mut Arena, numerator: i64, denominator: i64) -> ExprId {
        let numerator = arena.int(numerator).unwrap();
        let denominator = arena.int(denominator).unwrap();
        let minus_one = arena.int(-1).unwrap();
        let reciprocal = arena.power(denominator, minus_one).unwrap();
        arena.times(&[numerator, reciprocal]).unwrap()
    }

    #[test]
    fn a_structural_fraction_evaluates_to_its_exact_value() {
        let mut arena = Arena::new();
        let two_thirds = fraction(&mut arena, 2, 3);

        assert_eq!(evaluate(&arena, two_thirds), Some(rational(2, 3)));
    }

    #[test]
    fn a_sum_of_fractions_evaluates_without_going_through_a_float() {
        let mut arena = Arena::new();
        let two_thirds = fraction(&mut arena, 2, 3);
        let one_sixth = fraction(&mut arena, 1, 6);
        let sum = arena.plus(&[two_thirds, one_sixth]).unwrap();

        assert_eq!(evaluate(&arena, sum), Some(rational(5, 6)));
    }

    #[test]
    fn powers_are_evaluated_over_bignum() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let forty = arena.int(40).unwrap();
        let power = arena.power(two, forty).unwrap();

        assert_eq!(
            evaluate(&arena, power),
            Some(Rational::from(Integer::from(1_099_511_627_776_i64)))
        );
    }

    #[test]
    fn a_free_symbol_has_no_exact_value() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();
        let sum = arena.plus(&[x, two]).unwrap();

        assert_eq!(evaluate(&arena, sum), None);
    }

    #[test]
    fn dividing_by_zero_has_no_value_instead_of_a_wrong_one() {
        let mut arena = Arena::new();
        let over_zero = fraction(&mut arena, 2, 0);

        assert_eq!(evaluate(&arena, over_zero), None);
    }

    #[test]
    fn a_fractional_exponent_is_left_to_the_algebraic_tower() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let half = fraction(&mut arena, 1, 2);
        let root = arena.power(two, half).unwrap();

        assert_eq!(evaluate(&arena, root), None);
    }

    #[test]
    fn a_rewrite_that_preserves_the_value_verifies() {
        let mut arena = Arena::new();
        let sum = {
            let two_thirds = fraction(&mut arena, 2, 3);
            let one_sixth = fraction(&mut arena, 1, 6);
            arena.plus(&[two_thirds, one_sixth]).unwrap()
        };
        let result = fraction(&mut arena, 5, 6);

        let evidence = check(&arena, sum, result);

        assert_eq!(evidence.status(), Status::Verified);
        assert_eq!(evidence.level(), 0);
        assert_eq!(evidence.method(), Method::ExactRational);
    }

    /// El caso que justifica el crate: el motor devuelve un número plausible y
    /// equivocado, y nada más en el sistema se daría cuenta.
    #[test]
    fn a_rewrite_that_changes_the_value_is_reported_as_failed() {
        let mut arena = Arena::new();
        let sum = {
            let two_thirds = fraction(&mut arena, 2, 3);
            let one_sixth = fraction(&mut arena, 1, 6);
            arena.plus(&[two_thirds, one_sixth]).unwrap()
        };
        let wrong = fraction(&mut arena, 5, 7);

        assert_eq!(check(&arena, sum, wrong).status(), Status::Failed);
    }

    #[test]
    fn losing_the_value_along_the_way_is_a_failure_not_an_absence() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let x = arena.sym("x").unwrap();

        assert_eq!(check(&arena, two, x).status(), Status::Failed);
    }

    /// Con una variable libre ya no se contesta «no sé»: se muestrea. El sello
    /// sigue siendo nivel 0, y el método dice por qué.
    #[test]
    fn a_symbolic_input_is_checked_by_sampling() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();

        let evidence = check(&arena, x, x);
        assert_eq!(evidence.status(), Status::Verified);
        assert_eq!(evidence.method(), Method::RationalSampling);
        assert_eq!(evidence.level(), 0, "muestrear no demuestra");
    }

    /// `\frac{d}{dx}\left(x^{3}\right)` contra `3x^{2}`: la derivada del motor y
    /// la que sale de la aritmética dual coinciden en los doce puntos.
    #[test]
    fn a_derivative_is_checked_with_dual_numbers() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let three = arena.int(3).unwrap();
        let two = arena.int(2).unwrap();

        let cube = arena.power(x, three).unwrap();
        let request = arena
            .app(Head::Fn(Builtin::Derivative), &[cube, x])
            .unwrap();

        let squared = arena.power(x, two).unwrap();
        let answer = arena.times(&[three, squared]).unwrap();

        let evidence = check(&arena, request, answer);
        assert_eq!(evidence.status(), Status::Verified);
        assert_eq!(evidence.method(), Method::DualSampling);
        assert_eq!(evidence.level(), 0, "muestrear no demuestra");
    }

    /// **Y una derivada incorrecta se caza.** Es lo único que hace que el sello
    /// valga algo: `3x^{2}` frente a `2x^{2}`, el error de un exponente mal
    /// bajado, que es exactamente lo que una segunda implementación simbólica
    /// podría repetir y la aritmética dual no.
    #[test]
    fn a_wrong_derivative_is_caught() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let three = arena.int(3).unwrap();
        let two = arena.int(2).unwrap();

        let cube = arena.power(x, three).unwrap();
        let request = arena
            .app(Head::Fn(Builtin::Derivative), &[cube, x])
            .unwrap();

        let squared = arena.power(x, two).unwrap();
        let wrong = arena.times(&[two, squared]).unwrap();

        assert_eq!(check(&arena, request, wrong).status(), Status::Failed);
    }

    /// La regla del producto sale sola de que `\varepsilon^{2} = 0`, así que
    /// también se comprueba sin haberla escrito en ningún sitio.
    #[test]
    fn the_product_rule_falls_out_of_the_dual_arithmetic() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let one = arena.int(1).unwrap();
        let two = arena.int(2).unwrap();

        // x(x + 1), cuya derivada es 2x + 1.
        let shifted = arena.plus(&[x, one]).unwrap();
        let product = arena.times(&[x, shifted]).unwrap();
        let request = arena
            .app(Head::Fn(Builtin::Derivative), &[product, x])
            .unwrap();

        let doubled = arena.times(&[two, x]).unwrap();
        let answer = arena.plus(&[doubled, one]).unwrap();

        assert_eq!(check(&arena, request, answer).status(), Status::Verified);
    }

    /// Y una reescritura simbólica **incorrecta** se caza: es lo único que hace
    /// que el muestreo valga algo.
    #[test]
    fn a_wrong_symbolic_rewrite_is_caught_by_sampling() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();
        let doubled = arena.times(&[two, x]).unwrap();

        assert_eq!(check(&arena, x, doubled).status(), Status::Failed);
    }

    /// Dos variables libres no se muestrean: ligar sólo una y afirmar algo sería
    /// afirmar de más.
    #[test]
    fn two_free_variables_are_not_sampled() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let y = arena.sym("y").unwrap();
        let sum = arena.plus(&[x, y]).unwrap();

        assert_eq!(check(&arena, sum, sum).status(), Status::Unverified);
    }

    #[test]
    fn a_value_only_reaches_the_outside_through_its_evidence() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();

        let sealed = check(&arena, two, two).seal("resultado");

        assert_eq!(sealed.evidence().status(), Status::Verified);
        assert_eq!(*sealed.value(), "resultado");
    }
}
