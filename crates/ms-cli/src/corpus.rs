//! Los casos del corpus, leídos como datos.
//!
//! `engine/corpus/*.json` es el corpus dorado: cada caso es una consulta LaTeX
//! con lo que el motor debe devolver. Aquí se lee **entero** —no sólo el campo
//! `capability`— porque el árbol de `/motor` enseña los casos y la página los
//! ejecuta contra el motor real: sin el LaTeX y sin lo que se espera de él, no
//! habría nada que ejecutar ni contra qué comparar.
//!
//! Quien decide qué significan estos casos es [`crate::coverage`] y
//! [`crate::tree`]; aquí sólo se leen.

use std::path::{Path, PathBuf};

/// Un caso del corpus, con lo que hace falta para volver a ejecutarlo.
#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    /// La etapa del archivo del que salió: `E0`, `E1`.
    pub stage: String,
    pub latex: String,
    /// El valor exacto esperado. `None` en los casos cuya respuesta correcta es
    /// un error.
    pub result_latex: Option<String>,
    /// El `kind` del error esperado, si el caso fija uno.
    pub error: Option<String>,
    /// La hoja de `docs/` a la que responde. Opcional a propósito: un caso que
    /// fija un error o un límite no responde a ninguna capacidad.
    pub capability: Option<String>,
}

#[derive(Debug)]
pub enum CorpusError {
    Read { path: String, message: String },
    Malformed { path: String, message: String },
}

impl std::fmt::Display for CorpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorpusError::Read { path, message } => {
                write!(f, "no se pudo leer «{path}»: {message}")
            }
            CorpusError::Malformed { path, message } => {
                write!(f, "«{path}» no tiene la forma de un corpus: {message}")
            }
        }
    }
}

/// Todos los casos de `corpus/`, en el orden de los archivos y de los casos.
///
/// El orden importa: es el que la página enseña, y el primer caso de cada etapa
/// es su criterio de salida.
pub fn read(directory: &Path) -> Result<Vec<Case>, CorpusError> {
    let mut cases = Vec::new();

    for path in json_files(directory)? {
        let contents = std::fs::read_to_string(&path).map_err(|error| CorpusError::Read {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
        let document: serde_json::Value =
            serde_json::from_str(&contents).map_err(|error| CorpusError::Malformed {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;

        let stage = text(&document, "stage").unwrap_or_default();
        let listed = document
            .get("cases")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| CorpusError::Malformed {
                path: path.display().to_string(),
                message: String::from("falta el array «cases»"),
            })?;

        for case in listed {
            let name = text(case, "name").ok_or_else(|| CorpusError::Malformed {
                path: path.display().to_string(),
                message: String::from("un caso sin nombre"),
            })?;

            cases.push(Case {
                name,
                stage: stage.clone(),
                latex: text(case, "latex").unwrap_or_default(),
                result_latex: text(case, "result_latex"),
                error: text(case, "error"),
                capability: text(case, "capability"),
            });
        }
    }

    Ok(cases)
}

/// Los `.json` de un directorio, ordenados. `e0.json` antes que `e1.json`, que
/// es también el orden en que se construyeron.
fn json_files(directory: &Path) -> Result<Vec<PathBuf>, CorpusError> {
    let entries = std::fs::read_dir(directory).map_err(|error| CorpusError::Read {
        path: directory.display().to_string(),
        message: error.to_string(),
    })?;

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| CorpusError::Read {
            path: directory.display().to_string(),
            message: error.to_string(),
        })?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "json") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Un campo de texto, o `None` si falta o es `null`. Los dos son lo mismo aquí:
/// `"error": null` y no escribir `error` dicen que el caso no espera ninguno.
fn text(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}
