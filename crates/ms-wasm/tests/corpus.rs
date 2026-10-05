//! El corpus dorado de E0, y el anclaje de la forma del `ResultDoc`.
//!
//! El corpus es la métrica del proyecto (05-testing.md §1): cada capacidad que
//! entra trae su caso, y todos los de todas las etapas se ejecutan siempre.
//!
//! El test de forma existe porque el contrato tiene dos lados: `ResultDoc` en
//! Rust y `src/lib/engine/types.ts` en TypeScript. Nada obliga al compilador a
//! mantenerlos sincronizados, así que lo hace este test: cambiar un campo en
//! Rust rompe el build, y arreglarlo obliga a mirar el espejo de TypeScript.

// Ver la nota de ms-parse/tests/roundtrip.rs: los `allow-*-in-tests` de
// clippy.toml no alcanzan a los tests de integración, que son crates aparte.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_serialize::VerificationStatus;
use ms_wasm::{can_parse, solve, solve_to_document, surface};
use serde_json::Value;

/// Un archivo por etapa, y todos se ejecutan siempre. Una capacidad de E0 que se
/// rompe al añadir una regla de E1 tiene que fallar aquí, no descubrirse después.
const CORPUS: [&str; 4] = [
    include_str!("../../../corpus/e0.json"),
    include_str!("../../../corpus/e1.json"),
    include_str!("../../../corpus/e2.json"),
    include_str!("../../../corpus/e3.json"),
];

fn corpus_cases() -> Vec<Value> {
    CORPUS
        .iter()
        .flat_map(|stage| {
            let parsed: Value = serde_json::from_str(stage).expect("el corpus es JSON válido");
            parsed
                .get("cases")
                .and_then(Value::as_array)
                .expect("el corpus tiene casos")
                .clone()
        })
        .collect()
}

#[test]
fn every_corpus_case_behaves_as_recorded() {
    for case in corpus_cases() {
        let name = case["name"].as_str().expect("el caso tiene nombre");
        let latex = case["latex"].as_str().expect("el caso tiene latex");
        let document = solve_to_document(latex);

        match case["error"].as_str() {
            Some(expected) => {
                let error = document
                    .error
                    .as_ref()
                    .unwrap_or_else(|| panic!("«{name}» debía fallar y no falló"));
                assert_eq!(error.kind, expected, "«{name}» falló por otra razón");
                assert!(
                    document.result.is_none(),
                    "«{name}» devolvió resultado además del error"
                );
            }
            None => {
                assert_eq!(document.error, None, "«{name}» falló inesperadamente");
                let result = document
                    .result
                    .as_ref()
                    .unwrap_or_else(|| panic!("«{name}» no devolvió resultado"));

                assert_eq!(
                    Some(result.latex.as_str()),
                    case["result_latex"].as_str(),
                    "«{name}» dio otro valor exacto"
                );

                match case["approx"].as_f64() {
                    Some(expected) => {
                        let approx = result
                            .numeric
                            .as_ref()
                            .unwrap_or_else(|| panic!("«{name}» no trae aproximación"))
                            .approx;
                        assert!(
                            (approx - expected).abs() < 1e-12,
                            "«{name}»: {approx} no aproxima a {expected}"
                        );
                    }
                    None => assert!(
                        result.numeric.is_none(),
                        "«{name}» trae una aproximación que no debería"
                    ),
                }
            }
        }

        // Misma regla que `verification`: opcional, y quien lo declara lo fija
        // entero. Se compara el array serializado tal cual porque el orden de
        // los hechos también es contrato — la interfaz los pinta en ese orden.
        if let Some(expected) = case.get("facts") {
            let produced = serde_json::to_value(&document.facts).expect("los hechos serializan");
            assert_eq!(&produced, expected, "«{name}» afirmó otros hechos");
        }

        // Opcional a propósito: un caso que no lo declara no afirma nada sobre su
        // verificación. Los que lo declaran fijan que siga verificándose, que es
        // lo que impide que una regla nueva rompa la comprobación en silencio.
        if let Some(expected) = case["verification"].as_str() {
            let status = match document.verification.status {
                VerificationStatus::Verified => "verified",
                VerificationStatus::Failed => "failed",
                VerificationStatus::Unverified => "unverified",
            };
            assert_eq!(status, expected, "«{name}» verificó de otra manera");
        }

        let steps: Vec<&str> = document.trace.iter().map(|step| step.rule.as_str()).collect();
        let expected: Vec<&str> = case["steps"]
            .as_array()
            .expect("el caso declara pasos")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(steps, expected, "«{name}» siguió otra derivación");
    }
}

/// La superficie de funciones cruza el cable con la misma disciplina que el
/// documento: dos campos, y el espejo de TypeScript (`EngineFunction`) es lo
/// siguiente que hay que tocar si esto falla.
#[test]
fn the_surface_shape_matches_the_typescript_mirror() {
    let functions: Value = serde_json::from_str(&surface()).expect("JSON válido");
    let functions = functions.as_array().expect("es una lista");

    assert!(!functions.is_empty(), "el motor conoce funciones");
    for function in functions {
        assert_eq!(sorted_keys(function), vec!["arity", "name"]);
        assert!(function["name"].is_string());
        assert!(function["arity"].is_u64());
    }
}

/// Espejo exacto de `src/lib/engine/types.ts`. Si esto falla, ese archivo es lo
/// siguiente que hay que actualizar.
#[test]
fn the_wire_shape_matches_the_typescript_mirror() {
    let encoded = solve("\\frac{2}{3}+\\frac{1}{6}");
    let document: Value = serde_json::from_str(&encoded).expect("JSON válido");

    assert_eq!(
        sorted_keys(&document),
        vec![
            "error",
            "facts",
            "input",
            "result",
            "timing_ms",
            "trace",
            "verification"
        ]
    );
    assert_eq!(sorted_keys(&document["input"]), vec!["expr", "latex"]);
    assert_eq!(
        sorted_keys(&document["result"]),
        vec!["expr", "kind", "latex", "numeric"]
    );
    assert_eq!(sorted_keys(&document["result"]["numeric"]), vec!["approx"]);
    assert_eq!(
        sorted_keys(&document["verification"]),
        vec!["level", "method", "status"]
    );
    assert_eq!(
        sorted_keys(&document["trace"][0]),
        vec![
            "after",
            "after_latex",
            "before",
            "before_latex",
            "path",
            "rule",
            "soundness",
            "title"
        ]
    );
}

/// El árbol estructural manda sobre el LaTeX, y sus enteros van como cadena
/// porque son bignum: como número JSON, `JSON.parse` los rompería sobre 2^53.
#[test]
fn integers_cross_the_wire_as_strings() {
    let encoded = solve("\\frac{2}{3}+\\frac{1}{6}");
    let document: Value = serde_json::from_str(&encoded).expect("JSON válido");

    assert_eq!(document["result"]["expr"]["h"], "Times");
    assert_eq!(document["result"]["expr"]["a"][0]["int"], "5");
}

/// Un error del motor tampoco puede romper la forma: la UI lee siempre lo mismo.
#[test]
fn failing_documents_keep_the_same_shape() {
    let encoded = solve("\\frac{2}{0}");
    let document: Value = serde_json::from_str(&encoded).expect("JSON válido");

    assert_eq!(
        sorted_keys(&document),
        vec![
            "error",
            "facts",
            "input",
            "result",
            "timing_ms",
            "trace",
            "verification"
        ]
    );
    assert_eq!(sorted_keys(&document["error"]), vec!["kind", "message", "span"]);
}

/// Cada clase de hecho tiene su propia forma —la interfaz hace `switch` sobre
/// `class`— y ninguna se comprueba sola: el documento de `12` las trae las cinco
/// a la vez, que es la única entrada que las ejercita todas.
#[test]
fn the_fact_shapes_match_the_typescript_mirror() {
    let encoded = solve("12");
    let document: Value = serde_json::from_str(&encoded).expect("JSON válido");
    let facts = document["facts"]
        .as_array()
        .expect("un entero trae hechos")
        .clone();

    let classes: Vec<&str> = facts
        .iter()
        .filter_map(|fact| fact["class"].as_str())
        .collect();
    assert_eq!(
        classes,
        vec![
            "sign",
            "parity",
            "place-value",
            "primality",
            "factorization",
            "divisors"
        ]
    );

    assert_eq!(sorted_keys(&facts[0]), vec!["class", "value"]);
    assert_eq!(
        sorted_keys(&facts[2]),
        vec!["class", "digits", "negative"]
    );
    assert_eq!(
        sorted_keys(&facts[2]["digits"][0]),
        vec!["digit", "exponent"]
    );
    assert_eq!(
        sorted_keys(&facts[4]),
        vec!["class", "factors", "latex", "remaining"]
    );
    assert_eq!(
        sorted_keys(&facts[4]["factors"][0]),
        vec!["exponent", "prime"]
    );
    assert_eq!(sorted_keys(&facts[5]), vec!["class", "count", "values"]);

    // Los primos van como cadena y los exponentes como número: unos son bignum
    // y los otros no.
    assert_eq!(facts[4]["factors"][0]["prime"], "2");
    assert_eq!(facts[4]["factors"][0]["exponent"], 2);
    assert_eq!(facts[5]["values"][5], "12");
    // Las cifras y los exponentes no son bignum y van como número.
    assert_eq!(facts[2]["digits"][0]["digit"], 1);
    assert_eq!(facts[2]["digits"][0]["exponent"], 1);
}

/// El hecho graficable, que es lo único que E3 le da a la capa de dibujo: qué
/// variable y cuánto vale la expresión en una rejilla de puntos exactos.
///
/// **Ni ejes ni rango de vista**: el motor es agnóstico del graficador, y esa
/// frontera se comprueba aquí — si algún día aparece un campo de color en esta
/// lista, es que se cruzó.
#[test]
fn a_symbolic_result_carries_the_points_that_draw_it() {
    let document = solve_to_document("\\frac{d}{dx}\\left(x^{2}\\right)");
    let facts = serde_json::to_value(&document.facts).expect("los hechos serializan");
    let facts = facts.as_array().expect("array").clone();

    let samples = facts
        .iter()
        .find(|fact| fact["class"] == "graph-samples")
        .expect("una expresión con una variable trae sus puntos");

    assert_eq!(
        sorted_keys(samples),
        vec!["class", "points", "variable"]
    );
    assert_eq!(samples["variable"], "x");
    assert_eq!(sorted_keys(&samples["points"][0]), vec!["x", "y"]);

    // La derivada de x^2 es 2x, así que en x = 3 vale 6. Exacto y como cociente
    // de enteros: aquí no se redondea nada.
    let points = samples["points"].as_array().expect("array").clone();
    let at_three = points
        .iter()
        .find(|point| point["x"] == "3")
        .expect("la rejilla pasa por 3");
    assert_eq!(at_three["y"], "6");
}

/// Un número no tiene curva, y no se inventa una.
#[test]
fn a_numeric_result_has_no_points() {
    let document = solve_to_document("2 + 3");
    let facts = serde_json::to_value(&document.facts).expect("los hechos serializan");

    assert!(
        !facts
            .as_array()
            .expect("array")
            .iter()
            .any(|fact| fact["class"] == "graph-samples")
    );
}

/// El corpus fija qué reglas dispara cada caso, no cómo se escriben sus pasos.
/// Este los mira, porque un paso ilegible es un paso que no enseña nada — y el
/// objetivo primario del motor son los pasos (02-architecture.md).
///
/// Concretamente: el primer paso del criterio de salida escribía
/// `12^{-1}14^{-1}`, donde el `-1` y el `14` se leen pegados como un número que
/// no existe. El separador mira si el factor previo **termina** en número, no si
/// su último carácter es un dígito.
#[test]
fn adjacent_numeric_factors_stay_separated() {
    let document = solve_to_document("\\frac{7}{12}\\div\\frac{14}{9}+2^{40}");
    let first = document.trace.first().expect("el caso deja pasos");

    assert_eq!(
        first.after_latex, "7 \\cdot 9 \\cdot 12^{-1} \\cdot 14^{-1} + 2^{40}",
        "dos factores numéricos quedaron pegados"
    );
}

/// `probe` decide si la interfaz ofrece el botón de resolver, así que lo que
/// importa es dónde traza la raya: parsear no es resolver, y una línea que el
/// motor todavía no entiende no es un error que merezca mostrarse.
#[test]
fn probe_reports_only_whether_the_parser_can_read_it() {
    assert!(can_parse("\\frac{2}{3}+\\frac{1}{6}"));

    // Parsea y luego falla al resolver. Decir que sí aquí es correcto: el error
    // de división entre cero es una respuesta, y merece mostrarse.
    assert!(can_parse("\\frac{2}{0}"));

    // Sin `Head::Equal` no hay ecuaciones que comprobar. El día que las haya,
    // esta línea gana el botón sola, sin tocar la interfaz.
    assert!(!can_parse("e^{i\\pi}+1=0"));

    assert!(!can_parse(""));
}

fn sorted_keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("se esperaba un objeto")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}
