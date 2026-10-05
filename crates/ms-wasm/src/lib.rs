//! `ms-wasm` — la frontera con el navegador.
//!
//! Una sola función: entra LaTeX, sale un `ResultDoc` en JSON. Nada de esta capa
//! decide cómo se ve el resultado (02-architecture.md §1).
//!
//! **Aquí no sale ningún fallo hacia fuera.** Un `panic` en WASM mata el Worker
//! y se lleva la sesión del usuario, así que todo error —de sintaxis, de
//! matemáticas o de presupuesto— vuelve dentro del documento, como dato
//! explicable (01-rust-principles.md §6).

mod facts;
mod graph;

use ms_assume::Assumptions;
use ms_core::{Arena, Builtin, ExprId, Head, Node};
use ms_num::{Rational, approx_f64};
use ms_parse::{ParseError, parse};
use ms_rules::{DEFAULT_BUDGET, RULES, RuleError, rational_value, rewrite};
use ms_serialize::{
    EngineError, InputDoc, Numeric, ResultDoc, ResultKind, ResultValue, Span, TraceStep,
    Verification, VerificationMethod, VerificationStatus, to_json, to_latex,
};
use ms_verify::{Evidence, Method, Status};
use num_traits::ToPrimitive as _;
use wasm_bindgen::prelude::wasm_bindgen;

/// Resuelve una expresión LaTeX y devuelve el `ResultDoc` serializado.
///
/// Devuelve siempre un JSON válido: si algo falla, el fallo va **dentro** del
/// documento.
///
/// `timing_ms` sale en 0: quien llama es quien puede medir. En el navegador lo
/// rellena el worker con `performance.now()`, que además es más honesto porque
/// incluye el coste real de la llamada.
#[wasm_bindgen]
pub fn solve(latex: &str) -> String {
    let document = solve_to_document(latex);
    serde_json::to_string(&document).unwrap_or_else(|_| {
        // Inalcanzable con esta forma de documento. La alternativa a este
        // fallback sería un unwrap, y un unwrap aquí es la sesión del usuario.
        String::from(
            r#"{"input":{"latex":"","expr":null},"result":null,"facts":[],"trace":[],"verification":{"level":0,"status":"unverified","method":"none"},"error":{"kind":"serialization","message":"No se pudo serializar el resultado","span":null},"timing_ms":0.0}"#,
        )
    })
}

/// ¿Sabe el motor leer esta expresión? Sólo parsea; no reescribe ni verifica.
///
/// Existe para que la interfaz decida si ofrecer el botón de resolver sin tener
/// que duplicar el parser en TypeScript — y sin pagar una resolución entera por
/// cada línea que el usuario escribe. Una línea que no parsea no es un error que
/// merezca mostrarse: es una línea que todavía no va dirigida al motor.
///
/// Decir que sí **no** promete que resolver vaya a salir bien: `\frac{2}{0}`
/// parsea y luego falla, como debe.
#[wasm_bindgen]
pub fn probe(latex: &str) -> bool {
    can_parse(latex)
}

/// La versión tipada de `probe`, que es la que se prueba.
pub fn can_parse(latex: &str) -> bool {
    parse(&mut Arena::new(), latex).is_ok()
}

/// Las funciones que el motor conoce, con su aridad, en JSON.
///
/// Existe porque la interfaz ofrece una paleta y un autocompletado, y ésos tienen
/// que ofrecer **lo que el motor sabe leer y nada más**. Una lista escrita a mano
/// en TypeScript ofrecería `raiz` o `\sin` —que este motor rechaza— y cada E que
/// añada una función dejaría la paleta corta sin que nada avise. Preguntándolo,
/// la paleta crece cuando crece [`ms_core::Builtin::ALL`] y no puede mentir.
///
/// Sale el nombre y la aridad, que es lo que hace falta para escribir la llamada.
/// **No sale cómo se muestra**: la etiqueta, el idioma y el hueco donde va el
/// cursor son de la capa de representación (02-architecture.md §4). Con el nombre
/// y la aridad, esa capa construye `\operatorname{mcd}(#0, #1)` sola.
#[wasm_bindgen]
pub fn surface() -> String {
    serde_json::to_string(&builtin_surface()).unwrap_or_else(|_| String::from("[]"))
}

/// La versión tipada de [`surface`], que es la que se prueba.
pub fn builtin_surface() -> Vec<serde_json::Value> {
    Builtin::ALL
        .into_iter()
        .map(|builtin| {
            serde_json::json!({ "name": builtin.name(), "arity": builtin.arity() })
        })
        .collect()
}

/// La versión tipada, que es la que se prueba.
pub fn solve_to_document(latex: &str) -> ResultDoc {
    let mut arena = Arena::new();

    let parsed = match parse(&mut arena, latex) {
        Ok(parsed) => parsed,
        Err(error) => return failed(latex, None, parse_error(&error)),
    };

    let input_expr = to_json(&arena, parsed).ok();

    // La base de salida se quita del árbol **antes** de reescribir. No es una
    // operación: no cambia el número, cambia cómo se lee. Si entrara al
    // reescritor, o bien ninguna regla la tocaría —y el resultado saldría sin
    // resolver y sin verificar— o bien una regla la desarrollaría, el desarrollo
    // se plegaría otra vez al número y la petición se perdería, ciclando.
    let (parsed, requested_base) = match peel_output_base(&mut arena, parsed) {
        Ok(peeled) => peeled,
        Err(error) => return failed(latex, input_expr, error),
    };

    // La división entre cero la detecta el reescritor, que es el único que ve
    // todos los estados por los que pasa la expresión. Comprobarla aquí, sobre
    // la entrada y una sola vez, dejaba pasar `\frac{2}{1-1}`.
    let rewritten = match rewrite(&mut arena, parsed, RULES, &Assumptions::new(), DEFAULT_BUDGET) {
        Ok(rewritten) => rewritten,
        Err(error) => return failed(latex, input_expr, rule_error(&error)),
    };

    let mut trace = Vec::with_capacity(rewritten.steps.len());
    for step in &rewritten.steps {
        let (Ok(before), Ok(before_latex), Ok(after), Ok(after_latex)) = (
            to_json(&arena, step.before),
            to_latex(&arena, step.before),
            to_json(&arena, step.after),
            to_latex(&arena, step.after),
        ) else {
            continue;
        };
        trace.push(TraceStep {
            rule: step.rule.to_owned(),
            title: step.title.to_owned(),
            before,
            before_latex,
            after,
            after_latex,
            path: step.path.clone(),
            soundness: step.soundness.map(str::to_owned),
        });
    }

    // El valor exacto se calcula una vez y lo comparten el resultado y sus
    // hechos: dos llamadas serían dos fuentes que pueden separarse.
    let exact = rational_value(&arena, rewritten.expression);

    // Una base pedida que no se puede atender **se dice**. Devolver el número en
    // base 10 con la petición callada sería contestar otra pregunta, y quien la
    // hizo no tendría forma de saber que no se le contestó.
    if requested_base.is_some() {
        if let Some(error) = unwritable_in_base(exact.as_ref()) {
            return failed(latex, input_expr, error);
        }
    }

    // Invariante 3: el resultado y su verificación se construyen **juntos**, y no
    // hay forma de escribir uno sin el otro. La comprobación corre sobre la
    // entrada y la salida del reescritor, con código que no es el del reescritor.
    let verified = ms_verify::check(&arena, parsed, rewritten.expression)
        .seal(build_result(&arena, rewritten.expression, exact.as_ref()));
    let (result, evidence) = verified.into_parts();

    ResultDoc {
        input: InputDoc {
            latex: latex.to_owned(),
            expr: input_expr,
        },
        result,
        facts: assemble_facts(&arena, rewritten.expression, exact.as_ref(), requested_base),
        trace,
        verification: verification(evidence),
        error: None,
        timing_ms: 0.0,
    }
}

/// Los hechos del resultado: los del número, y los de la curva si es una
/// expresión con una variable.
///
/// Los dos conjuntos son disjuntos por construcción — un resultado o vale un
/// número o tiene una variable libre— así que no hay orden que decidir entre
/// ellos, sólo concatenación.
fn assemble_facts(
    arena: &Arena,
    expression: ExprId,
    exact: Option<&Rational>,
    requested_base: Option<u32>,
) -> Vec<ms_serialize::FactDoc> {
    let mut facts = facts::facts(exact, requested_base);
    if let Some(samples) = graph::samples(arena, expression) {
        facts.push(samples);
    }
    facts
}

/// El error de una petición de base mal hecha.
const OUTPUT_BASE_ERROR: &str = "output-base";

/// Quita el `\operatorname{enBase}(b, x)` de la raíz y devuelve `(x, b)`.
///
/// Una consulta sin envoltorio devuelve `(la consulta, None)` y sale en base 10,
/// que es lo que ha pasado siempre: la base por defecto no es una decisión nueva,
/// es la que ya había.
///
/// **Sólo en la raíz, y a propósito.** `2 + \operatorname{enBase}(2, 11)` no es
/// una petición de salida: es una expresión que mezcla un número con la forma de
/// escribirlo, y no significa nada. Aceptarla la dejaría sin resolver y marcada
/// «sin verificar», que es peor que un error — el estudiante vería un documento a
/// medias sin saber qué pidió mal. Se rechaza con su explicación.
fn peel_output_base(
    arena: &mut Arena,
    expression: ExprId,
) -> Result<(ExprId, Option<u32>), EngineError> {
    let is_call = arena.head_of(expression) == Some(Head::Fn(Builtin::InBase));
    if !is_call {
        if contains_output_base(arena, expression) {
            return Err(output_base_error(
                "La base de salida se pide alrededor de toda la consulta: \
                 \\operatorname{enBase}(2, 1+1), no dentro de ella",
            ));
        }
        return Ok((expression, None));
    }

    let args = arena.app_args(expression).map(<[ExprId]>::to_vec);
    let Some([base, value]) = args.as_deref() else {
        return Err(output_base_error(
            "\\operatorname{enBase} toma la base y la expresión: \\operatorname{enBase}(2, 1+1)",
        ));
    };
    if contains_output_base(arena, *value) {
        return Err(output_base_error(
            "Una base de salida dentro de otra no significa nada: \
             un número se escribe en una base, no en dos",
        ));
    }

    // La base es un literal, no una expresión que haya que resolver: es parte de
    // la pregunta, y una pregunta que hay que calcular antes de entenderla no se
    // puede señalar cuando está mal.
    let Some(Node::Int(base)) = arena.get(*base) else {
        return Err(output_base_error(
            "La base tiene que ser un número entero escrito, no una expresión",
        ));
    };
    let Some(base) = base.to_u32().filter(|base| {
        (ms_num::MIN_BASE..=ms_num::MAX_BASE).contains(base)
    }) else {
        return Err(output_base_error(&format!(
            "Sólo se puede escribir en bases de {} a {}: fuera de ahí no hay cifras con las que escribirlo",
            ms_num::MIN_BASE,
            ms_num::MAX_BASE
        )));
    };

    Ok((*value, Some(base)))
}

/// Por qué el resultado no se puede escribir en la base pedida, si es que no.
///
/// Son los dos casos en los que `ms_facts::in_base` no devuelve nada, y aquí se
/// separan porque al estudiante le hacen falta dos explicaciones distintas: una
/// dice que la consulta no llegó a un número, y la otra que llegó a uno que este
/// motor todavía no sabe escribir así.
fn unwritable_in_base(exact: Option<&Rational>) -> Option<EngineError> {
    let Some(value) = exact else {
        return Some(output_base_error(
            "Para escribir un resultado en otra base hace falta que sea un número, \
             y esta consulta no llega a uno",
        ));
    };
    if value.is_integer() {
        return None;
    }
    Some(output_base_error(
        "Por ahora sólo los enteros se escriben en otra base: una fracción tendría \
         desarrollo periódico en ella, y eso todavía no lo sé escribir",
    ))
}

fn output_base_error(message: &str) -> EngineError {
    EngineError {
        kind: OUTPUT_BASE_ERROR.to_owned(),
        message: message.to_owned(),
        span: None,
    }
}

/// ¿Hay algún `enBase` dentro de este subárbol?
///
/// Sirve para rechazar lo que no es una petición de salida en lugar de dejarlo
/// pasar sin resolver: un `enBase` que sobreviva a la reescritura sale como
/// expresión sin verificar, y eso no explica nada.
fn contains_output_base(arena: &Arena, expression: ExprId) -> bool {
    if arena.head_of(expression) == Some(Head::Fn(Builtin::InBase)) {
        return true;
    }
    let Some(args) = arena.app_args(expression) else {
        return false;
    };
    args.to_vec()
        .iter()
        .any(|argument| contains_output_base(arena, *argument))
}

/// El certificado de `ms-verify`, traducido al contrato de salida.
///
/// Traducir aquí y no en `ms-serialize` es lo que mantiene la dirección de
/// dependencias: `ms-serialize` describe la forma del documento y no sabe quién
/// verifica (02-architecture.md §2).
fn verification(evidence: Evidence) -> Verification {
    Verification {
        level: evidence.level(),
        status: match evidence.status() {
            Status::Verified => VerificationStatus::Verified,
            Status::Failed => VerificationStatus::Failed,
            Status::Unverified => VerificationStatus::Unverified,
        },
        method: match evidence.method() {
            Method::ExactRational => VerificationMethod::ExactRational,
            Method::SymbolicSubstitution => VerificationMethod::SymbolicSubstitution,
            Method::RationalSampling => VerificationMethod::RationalSampling,
            Method::DualSampling => VerificationMethod::DualSampling,
        },
    }
}

/// Qué clase de respuesta es.
///
/// El conjunto de soluciones se reconoce por su cabeza y no por tener un valor
/// exacto: `\left\{x = 2, x = 3\right\}` no evalúa a ningún número, así que sin
/// esta rama saldría como una expresión cualquiera y la interfaz no tendría
/// forma de saber que lo que tiene delante son las soluciones de una ecuación.
fn result_kind(arena: &Arena, expression: ExprId, exact: Option<&Rational>) -> ResultKind {
    if arena.head_of(expression) == Some(ms_core::Head::Solutions) {
        return ResultKind::Solutions;
    }
    if exact.is_some() {
        return ResultKind::Number;
    }
    ResultKind::Expression
}

fn build_result(
    arena: &Arena,
    expression: ExprId,
    exact: Option<&Rational>,
) -> Option<ResultValue> {
    let latex = to_latex(arena, expression).ok()?;
    let expr = to_json(arena, expression).ok()?;

    let numeric = exact.and_then(approx_f64).map(|approx| Numeric { approx });

    Some(ResultValue {
        kind: result_kind(arena, expression, exact),
        expr,
        latex,
        numeric,
    })
}

fn failed(latex: &str, expr: Option<serde_json::Value>, error: EngineError) -> ResultDoc {
    ResultDoc {
        input: InputDoc {
            latex: latex.to_owned(),
            expr,
        },
        result: None,
        // Sin resultado no hay nada de lo que afirmar hechos.
        facts: Vec::new(),
        trace: Vec::new(),
        // Un documento que falla antes de tener resultado no puede afirmar nada
        // sobre él: no hay nada que verificar, y decir «sin verificar» ya es la
        // afirmación honesta.
        verification: Verification::none(),
        error: Some(error),
        timing_ms: 0.0,
    }
}

fn parse_error(error: &ParseError) -> EngineError {
    EngineError {
        kind: error.kind_id().to_owned(),
        message: error.to_string(),
        span: Some(Span {
            start: error.span.start,
            end: error.span.end,
        }),
    }
}

fn rule_error(error: &RuleError) -> EngineError {
    let kind = match error {
        RuleError::Core(_) => "core",
        RuleError::BudgetExhausted { .. } => "budget-exhausted",
        RuleError::Cycle { .. } => "cycle",
        RuleError::DivisionByZero => "division-by-zero",
        RuleError::MagnitudeLimit => "magnitude-limit",
        RuleError::IrrationalSolutions => "irrational-solutions",
        RuleError::UnsupportedDerivative => "unsupported-derivative",
    };
    EngineError {
        kind: kind.to_owned(),
        message: error.to_string(),
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exit_criterion_of_e0() {
        let document = solve_to_document("\\frac{2}{3}+\\frac{1}{6}");

        let result = document.result.expect("hay resultado");
        assert_eq!(result.latex, "\\frac{5}{6}");
        assert_eq!(result.kind, ResultKind::Number);

        let approx = result.numeric.expect("hay aproximación").approx;
        assert!((approx - 0.8333333333333334).abs() < 1e-12);

        let titles: Vec<&str> = document
            .trace
            .iter()
            .map(|step| step.title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec!["Buscamos denominador común", "Sumamos los numeradores"]
        );
        assert_eq!(document.error, None);
    }

    /// El criterio de salida de E1, en la frontera: lo que de verdad recibe el
    /// navegador.
    #[test]
    fn the_exit_criterion_of_e1() {
        let document = solve_to_document("\\frac{7}{12}\\div\\frac{14}{9}+2^{40}");

        let result = document.result.expect("hay resultado");
        assert_eq!(result.latex, "\\frac{8796093022211}{8}");
        assert_eq!(result.kind, ResultKind::Number);

        let approx = result.numeric.expect("hay aproximación").approx;
        assert!((approx - 1_099_511_627_776.375).abs() < 1e-3);

        let titles: Vec<&str> = document
            .trace
            .iter()
            .map(|step| step.title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec![
                "Dividir es multiplicar por el recíproco",
                "Multiplicamos las fracciones",
                "Simplificamos la fracción",
                "Elevamos a la potencia",
                "Buscamos denominador común",
                "Sumamos los numeradores",
            ]
        );

        assert_eq!(document.verification.status, VerificationStatus::Verified);
        assert_eq!(document.error, None);
    }

    /// El presupuesto cuenta iteraciones y esto no cicla: calcula. Sin el tope de
    /// magnitud, esta consulta se lleva por delante la pestaña del usuario.
    #[test]
    fn an_absurd_power_explains_itself_instead_of_exhausting_the_tab() {
        let document = solve_to_document("2^{10000000000}");

        let error = document.error.expect("hay error");
        assert_eq!(error.kind, "magnitude-limit");
        assert!(document.result.is_none());
    }

    #[test]
    fn the_trace_carries_renderable_latex_on_both_sides() {
        let document = solve_to_document("\\frac{2}{3}+\\frac{1}{6}");
        let first = document.trace.first().expect("hay un primer paso");

        assert_eq!(first.before_latex, "\\frac{1}{6} + \\frac{2}{3}");
        assert_eq!(first.after_latex, "\\frac{1}{6} + \\frac{4}{6}");
    }

    #[test]
    fn a_closed_arithmetic_result_comes_back_verified_and_says_how() {
        let document = solve_to_document("1+1");

        assert_eq!(document.verification.status, VerificationStatus::Verified);
        assert_eq!(document.verification.level, 0);
        // Nivel 0 con variables libres es muestreo; sin ellas es evaluación
        // exacta en ℚ, y el documento distingue una cosa de la otra.
        assert_eq!(
            document.verification.method,
            VerificationMethod::ExactRational
        );
    }

    /// Invariante 3: lo que no se pudo comprobar sale **marcado**, no oculto ni
    /// disfrazado de verificado.
    ///
    /// Con dos variables libres no hay muestreo posible —ligar una sola y
    /// afirmar algo sería afirmar de más— así que este documento sigue siendo el
    /// que prueba que «no sé» se dice.
    #[test]
    fn a_result_that_could_not_be_checked_is_marked_instead_of_claimed() {
        let document = solve_to_document("x + y + \\frac{1}{2}");

        assert_eq!(document.verification.status, VerificationStatus::Unverified);
        assert!(document.result.is_some());
    }

    /// Con una sola variable libre sí se comprueba, muestreando, y el método lo
    /// dice: el nivel sigue siendo 0 porque muestrear no demuestra.
    #[test]
    fn a_symbolic_result_with_one_variable_is_checked_by_sampling() {
        let document = solve_to_document("x + \\frac{1}{2}");

        assert_eq!(document.verification.status, VerificationStatus::Verified);
        assert_eq!(document.verification.method, VerificationMethod::RationalSampling);
        assert_eq!(document.verification.level, 0);
    }

    #[test]
    fn a_document_without_a_result_claims_nothing_about_it() {
        let document = solve_to_document("\\frac{2}{0}");

        assert_eq!(document.verification, Verification::none());
    }

    #[test]
    fn a_syntax_error_comes_back_as_data_with_its_span() {
        let document = solve_to_document("\\frac{2}{3} +");

        let error = document.error.expect("hay error");
        assert_eq!(error.kind, "unexpected-end");
        assert!(document.result.is_none());
    }

    #[test]
    fn unicode_math_is_refused_and_pointed_at() {
        let source = "2 + √2";
        let document = solve_to_document(source);

        let error = document.error.expect("hay error");
        assert_eq!(error.kind, "unicode-math");
        let span = error.span.expect("hay span");
        assert_eq!(&source[span.start..span.end], "√");
    }

    #[test]
    fn dividing_by_zero_explains_itself_instead_of_answering() {
        let document = solve_to_document("\\frac{2}{0}");

        let error = document.error.expect("hay error");
        assert_eq!(error.kind, "division-by-zero");
        assert!(document.result.is_none());
    }

    #[test]
    fn a_symbolic_expression_comes_back_without_a_decimal() {
        let document = solve_to_document("x + \\frac{1}{2}");

        let result = document.result.expect("hay resultado");
        assert_eq!(result.kind, ResultKind::Expression);
        assert!(result.numeric.is_none());
    }

    /// La paleta escribe la llamada a partir del nombre y la aridad que salen de
    /// aquí. Si la aridad no fuera la que el parser exige, cada botón de la paleta
    /// insertaría una llamada que el motor rechaza —y sería la interfaz la que
    /// pareciera rota—, así que la promesa se comprueba escribiendo la llamada.
    #[test]
    fn every_function_in_the_surface_can_be_called_as_the_surface_says() {
        let surface = builtin_surface();
        assert_eq!(surface.len(), Builtin::ALL.len());

        for entry in &surface {
            let name = entry["name"].as_str().expect("el nombre es texto");
            let published = entry["arity"].as_u64().expect("la aridad es un número");
            let arity = usize::try_from(published).expect("la aridad cabe en un usize");

            let arguments = vec!["1"; arity].join(", ");
            let call = format!("\\operatorname{{{name}}}({arguments})");
            assert!(can_parse(&call), "la superficie promete «{call}»");

            // Y una llamada con un argumento de más no es esta función: si esto
            // pasara, la aridad publicada no estaría diciendo nada.
            let arguments = vec!["1"; arity + 1].join(", ");
            let wrong = format!("\\operatorname{{{name}}}({arguments})");
            assert!(!can_parse(&wrong), "«{wrong}» no debería parsear");
        }
    }

    /// Los nombres que la interfaz **no** debe ofrecer, escritos para que se vea
    /// que la lista corta es la lista de verdad. `raiz` es el caso que más se
    /// pide y el que más se da por hecho.
    #[test]
    fn names_the_engine_does_not_know_stay_out_of_the_surface() {
        for name in ["raiz", "sin", "cos", "log", "ln", "sqrt", "integrar"] {
            assert!(
                Builtin::from_name(name).is_none(),
                "«{name}» no es una función de este motor"
            );
            assert!(!can_parse(&format!("\\operatorname{{{name}}}(1)")));
        }
    }

    #[test]
    fn the_json_is_always_parseable_even_when_the_input_is_garbage() {
        for source in ["", "\\int", ")(", "\\frac{}{}", "2 +", "√"] {
            let encoded = solve(source);
            let decoded: serde_json::Value =
                serde_json::from_str(&encoded).expect("el JSON siempre es válido");
            assert!(decoded.get("input").is_some(), "falta input en «{source}»");
        }
    }
}
