//! El contrato de salida: un documento **semántico**, no una presentación.
//!
//! Entra LaTeX y sale un resultado normalizado del que se pueden derivar muchas
//! representaciones — gráfica, tabla, diagrama, escena de vídeo, prosa. El motor
//! no produce ninguna (02-architecture.md §1).
//!
//! Espejo en TypeScript: `src/lib/engine/types.ts`. El test `golden_result_doc`
//! ancla esta forma contra `engine/corpus/e0.json`, así que un cambio de forma
//! en Rust rompe el build en vez de desincronizar la UI en silencio.
//!
//! Dos campos del contrato completo **no están todavía**, y cada uno tiene su
//! fecha. Un array siempre vacío no es un contrato, es decorado:
//!
//! - `bindings` de cada paso — E2, con el DSL de patrones de `rules!`. Antes de
//!   que exista un patrón no hay nada que ligar; hoy sería un mapa vacío.
//! - `alternates` — E3, con el resto de `ms-facts`.
//!
//! `facts` entró en E1 fase 3, adelantado respecto al roadmap que lo situaba en
//! E3, para que E3 añada clases en vez de estrenar el campo. Sale vacío cuando
//! el resultado no es un entero, y eso es una respuesta: vacío **siempre** habría
//! sido decorado, vacío a veces es información.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::facts::FactDoc;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ResultDoc {
    pub input: InputDoc,
    pub result: Option<ResultValue>,
    /// Lo que el motor sabe del resultado además de su valor. Vacío cuando no
    /// sabe nada que merezca decirse.
    pub facts: Vec<FactDoc>,
    pub trace: Vec<TraceStep>,
    pub verification: Verification,
    pub error: Option<EngineError>,
    pub timing_ms: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InputDoc {
    pub latex: String,
    /// El árbol estructural. `None` si la entrada ni siquiera se pudo parsear.
    pub expr: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ResultValue {
    pub kind: ResultKind,
    pub expr: Value,
    pub latex: String,
    /// La aproximación decimal, cuando existe. El valor exacto manda siempre;
    /// esto es una comodidad para el lector, no el resultado.
    pub numeric: Option<Numeric>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ResultKind {
    Number,
    Expression,
    /// El conjunto de soluciones de una ecuación: `\left\{x = 2, x = 3\right\}`.
    ///
    /// Variante propia y no `Expression` porque la capa de representación hace
    /// cosas distintas con ella —una lista de soluciones no se lee como una
    /// expresión suelta— y porque un `switch` cerrado por un `never` en
    /// TypeScript se cae al compilar si alguien añade una clase sin decidir cómo
    /// se ve, que es el mismo mecanismo que protege a `FactDoc`.
    Solutions,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Numeric {
    pub approx: f64,
}

/// Un paso de la derivación. **Es el objetivo primario del motor**: un CAS que
/// sólo escupe la respuesta es una calculadora.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TraceStep {
    /// Id estable de la regla, para poder enlazar explicación y telemetría.
    pub rule: String,
    /// Título legible, el que ve el estudiante.
    pub title: String,
    pub before: Value,
    pub before_latex: String,
    pub after: Value,
    pub after_latex: String,
    /// Dónde disparó la regla: los índices de argumento desde la raíz hasta el
    /// subtérmino reescrito. Vacío significa «en la raíz».
    ///
    /// `before` y `after` son fotos del árbol **entero**, y con eso basta
    /// mientras el paso es «sumamos los numeradores». Con derivadas en E3 el paso
    /// es «derivamos *este* subtérmino» y sin el locus la interfaz no puede
    /// señalarlo. Entra ahora porque cambiar el contrato una vez cuesta menos que
    /// cambiarlo tres.
    pub path: Vec<usize>,
    /// Teorema de Lean que respalda la regla, cuando lo hay
    /// (03-lean-verification.md). En E0 todavía no hay ninguno.
    pub soundness: Option<String>,
}

/// Invariante 3 de 02-architecture.md §4: si un resultado no se pudo verificar,
/// **sale marcado, no se oculta**.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Verification {
    pub level: u8,
    pub status: VerificationStatus,
    /// Cómo se comprobó. El nivel solo no basta: «evaluado exactamente en ℚ» y
    /// «muestreado en 30 puntos» son ambos nivel 0 y no valen lo mismo.
    pub method: VerificationMethod,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    Unverified,
    Verified,
    Failed,
}

/// El espejo del método de `ms-verify` en el contrato de salida.
///
/// Es un enum propio y no el de `ms-verify` porque `ms-serialize` no depende de
/// `ms-verify` (02-architecture.md §2): quien traduce entre los dos es la
/// frontera, que es la única que ve ambos lados.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationMethod {
    /// No se comprobó nada, y por tanto no se afirma nada.
    None,
    /// Evaluación exacta en ℚ, sin variables libres.
    ExactRational,
    /// Cada solución sustituida en la ecuación original y los dos lados
    /// evaluados en ℚ.
    ///
    /// Afirma que las soluciones devueltas **cumplen** la ecuación; no que estén
    /// todas. Comprobar lo segundo exigiría el mismo código que las encontró.
    SymbolicSubstitution,
    /// Entrada y salida evaluadas en varios puntos racionales fijos y
    /// comparadas. Es lo que aplica cuando hay una variable libre.
    ///
    /// No es una demostración: dos expresiones distintas pueden coincidir en los
    /// puntos probados. Por eso sigue siendo nivel 0.
    RationalSampling,
    /// Lo mismo, pero el valor de la entrada en cada punto salió de **derivar
    /// evaluando**: números duales, `\varepsilon^{2} = 0`, aritmética exacta en ℚ.
    ///
    /// Es lo que sella una derivada. Afirma que la derivada devuelta coincide con
    /// la verdadera en los puntos probados, calculada por un camino que no
    /// reescribe: el verificador no ve `ms-diff` y no puede repetir su error.
    DualSampling,
}

impl Verification {
    /// Lo que se reporta cuando no hubo comprobación: un documento que falla
    /// antes de tener resultado no puede afirmar nada sobre él.
    pub fn none() -> Self {
        Self {
            level: 0,
            status: VerificationStatus::Unverified,
            method: VerificationMethod::None,
        }
    }
}

/// Un fallo matemático o de entrada, **como dato**. Sube hasta la UI y se
/// muestra como explicación, no como stack trace (§6).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EngineError {
    pub kind: String,
    pub message: String,
    /// Rango de bytes sobre el LaTeX original, para poder subrayar el error
    /// donde el usuario lo escribió.
    pub span: Option<Span>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}
