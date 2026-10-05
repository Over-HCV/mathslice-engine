//! El tipo de una regla y sus fallos.

use ms_assume::Assumptions;
use ms_core::{Arena, CoreError, ExprId};

/// Lo que devuelve una regla: la expresión reescrita, o `None` si no aplica.
///
/// "No aplica" no es un error. Un CAS pasa la mayor parte del tiempo probando
/// reglas que no encajan; eso es el funcionamiento normal, no un fallo.
pub type RuleResult = Result<Option<ExprId>, RuleError>;

/// Una regla de reescritura. **No se construye a mano**: la genera `rules!`,
/// que es lo que garantiza que ninguna se quede sin metadatos (§5).
pub struct Rule {
    /// Id estable. Sobrevive a los cambios de nombre y de redacción.
    pub id: &'static str,
    /// El título que lee el estudiante.
    pub title: &'static str,
    /// La forma general del paso, en LaTeX.
    pub step_template: &'static str,
    /// Teorema de Lean que respalda la regla, si lo hay
    /// (03-lean-verification.md). Opcional por regla y nunca bloquea una etapa.
    /// Con qué se respalda la corrección de esta regla, si con algo.
    ///
    /// `None` significa que **nadie lo ha mirado**, que es distinto de
    /// [`Soundness::NumericOnly`] —«lo miramos y decidimos que no»—. La
    /// diferencia importa para la métrica del proyecto: una es deuda y la otra
    /// es una decisión.
    pub soundness: Option<Soundness>,
    /// Si es `true`, la reescritura no aparece en la traza. Por defecto es
    /// `false`: lo visible es el default y el silencio se declara (invariante 2).
    pub silent: bool,
    /// El contexto de suposiciones es un argumento **obligatorio**, no un extra.
    /// Casi toda regla de un CAS tiene una condición lateral, y la forma
    /// habitual de dar una respuesta incorrecta es aplicar una regla cuya
    /// condición nadie comprobó. Aquí no se puede escribir una regla sin tener a
    /// mano con qué comprobarla.
    pub apply: fn(&mut Arena, ExprId, &Assumptions) -> RuleResult,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RuleError {
    Core(CoreError),
    /// El reescritor no llegó a forma normal dentro del presupuesto. Se reporta
    /// con lo conseguido hasta ahí; no se finge una respuesta.
    BudgetExhausted { iterations: u32 },
    /// Se volvió a ver una expresión ya vista: el conjunto de reglas cicla.
    /// Barato de detectar porque ids iguales significan expresión ya vista (§2).
    Cycle { rule: &'static str },
    /// La expresión divide entre cero, y por tanto no denota nada. No es un
    /// fallo del reescritor: es la respuesta, y sube hasta la UI como
    /// explicación (§6).
    DivisionByZero,
    /// El resultado exacto sería demasiado grande para calcularlo.
    ///
    /// **No lo cubre `BudgetExhausted`**: el presupuesto cuenta iteraciones y
    /// esto no cicla, calcula. `2^{10^{9}}` se resuelve en una sola reescritura
    /// y deja mil millones de bits en memoria, que mata la pestaña sin haber
    /// gastado una iteración. El tope vive en `ms_num::MAX_EXACT_BITS`.
    MagnitudeLimit,
    /// La ecuación tiene solución, pero **no racional**, y este motor todavía no
    /// sabe escribir un irracional.
    ///
    /// No es «no tiene solución», que sería falso: `x^{2}-2=0` tiene dos, y son
    /// `\pm\sqrt{2}`. Es un límite de la etapa, y decirlo es la diferencia entre
    /// un motor que no llega y uno que se calla. Escribirlas exige `\sqrt` y con
    /// él la torre algebraica, que el roadmap sitúa en E5.
    ///
    /// Es el mismo trato que `\operatorname{enBase}` da a una fracción: un error
    /// que explica el límite, en vez de una respuesta a otra pregunta.
    IrrationalSolutions,
    /// Se pidió una derivada que este motor todavía no sabe hacer.
    ///
    /// Como [`RuleError::IrrationalSolutions`], es un límite de la etapa dicho en
    /// voz alta y no un fallo: `\frac{d}{dx}\left(x^{\frac{1}{2}}\right)` tiene
    /// derivada y es `\frac{1}{2\sqrt{x}}`, que exige `\sqrt` y con él la torre
    /// algebraica de E5.
    ///
    /// La alternativa era devolver la pregunta sin responder, sin error y sin
    /// paso: la forma de fallar en la que nadie se entera de que falló.
    UnsupportedDerivative,
}

/// Con qué respalda una regla su corrección (03-lean-verification.md §2).
///
/// Es un enum y no una cadena porque el compilador tiene que poder distinguir
/// «demostrado» de «no demostrado» sin interpretar texto: la métrica del
/// proyecto es **qué porcentaje de reglas está verificado por máquina**, y una
/// métrica que se lee a ojo no es una métrica.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Soundness {
    /// Un teorema que ya existe en Mathlib.
    Mathlib(&'static str),
    /// Un teorema demostrado en `engine/lean/`.
    ///
    /// Que el nombre exista de verdad lo comprueba `scripts/check-soundness.sh`
    /// y lo corre `pnpm engine:lean`. Sin esa comprobación, una errata en el
    /// nombre afirmaría una demostración inexistente — el fallo exacto que este
    /// campo existe para impedir.
    Local(&'static str),
    /// Sin demostración, **y queda dicho**. Es una respuesta válida para
    /// cualquier regla: Lean nunca bloquea una etapa.
    NumericOnly,
}

impl Soundness {
    /// El nombre del teorema, si lo hay. Es lo que viaja al documento de salida.
    pub fn theorem(self) -> Option<&'static str> {
        match self {
            Soundness::Mathlib(name) | Soundness::Local(name) => Some(name),
            Soundness::NumericOnly => None,
        }
    }

    /// ¿Lo respalda una máquina? Es la métrica del proyecto.
    pub fn is_proved(self) -> bool {
        self.theorem().is_some()
    }
}

impl From<CoreError> for RuleError {
    fn from(error: CoreError) -> Self {
        RuleError::Core(error)
    }
}

impl core::fmt::Display for RuleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RuleError::Core(error) => write!(f, "{error}"),
            RuleError::BudgetExhausted { iterations } => {
                write!(f, "no se alcanzó forma normal en {iterations} pasos")
            }
            RuleError::Cycle { rule } => {
                write!(f, "la regla «{rule}» hace ciclar la reescritura")
            }
            RuleError::DivisionByZero => {
                write!(f, "División entre cero: la expresión no está definida")
            }
            RuleError::MagnitudeLimit => write!(
                f,
                "El resultado exacto sería demasiado grande para calcularlo"
            ),
            RuleError::IrrationalSolutions => write!(
                f,
                "Sus soluciones no son racionales, y todavía no se saben escribir"
            ),
            RuleError::UnsupportedDerivative => write!(
                f,
                "Esta derivada todavía no se sabe hacer: se derivan sumas, productos y potencias de exponente entero"
            ),
        }
    }
}

impl core::error::Error for RuleError {}
