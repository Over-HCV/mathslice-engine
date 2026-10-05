//! El inventario de capacidades, extraído de `engine/docs/`.
//!
//! Los archivos de `docs/` son el árbol de capacidades de referencia.
//! Tienen dos clases de línea y sólo dos:
//!
//! ```text
//! - [ ] Elementary Math
//!   - [ ] Arithmetic: Perform multiple arithmetic operations.
//!     - [ ] Addition & Subtraction: Perform addition and subtraction.
//!         Add numbers:
//!         Subtract numbers:
//! ```
//!
//! Las que empiezan por `- [ ]` son secciones; las que no, y terminan en `:`,
//! son **hojas**, que es el nivel al que se cuenta una capacidad.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// El archivo que **no** cuenta, y por qué.
///
/// `common-core-math.md` no es una categoría de cómputo: es un índice curricular
/// K-12 que re-etiqueta por grado escolar capacidades ya contadas en Arithmetic,
/// Algebra, Geometry y compañía. Contarlas dos veces inflaría el denominador con
/// trabajo que no existe. La decisión y el número están en `docs/main.md`.
const REINDEXED_FILE: &str = "common-core-math";

/// El separador del camino, aquí y en el campo `capability` del corpus.
pub const PATH_SEPARATOR: &str = " > ";

/// Una capacidad hoja, con el camino por el que se llega a ella.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Capability {
    /// `Elementary Math > Arithmetic > Addition & Subtraction > Add numbers`.
    /// Es el id que un caso del corpus escribe en su campo `capability`.
    pub id: String,
    /// El mismo camino por partes, de la categoría a la hoja. Va aparte del id
    /// porque reconstruir el árbol partiendo la cadena obligaría a que ningún
    /// título contuviera nunca el separador, y eso no se puede prometer de un
    /// texto que no escribimos nosotros.
    pub path: Vec<String>,
    /// El archivo del que salió, sin extensión.
    pub file: String,
    /// Si viene del índice curricular, y por tanto no cuenta en el denominador.
    pub reindexed: bool,
}

#[derive(Debug)]
pub enum CapabilityError {
    Read { path: String, message: String },
    NoDocuments { directory: String },
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilityError::Read { path, message } => {
                write!(f, "no se pudo leer «{path}»: {message}")
            }
            CapabilityError::NoDocuments { directory } => {
                write!(f, "no hay documentos de capacidades en «{directory}»")
            }
        }
    }
}

/// El índice que declara en qué orden van las categorías.
const MAIN_DOCUMENT: &str = "main.md";

/// Todas las capacidades hoja de `docs/`, en el orden en que están escritas.
///
/// El orden es el del documento a propósito, dos veces: dentro de un archivo, el
/// de sus líneas; y entre archivos, el del bloque `@import` de `main.md`, que es
/// el del inventario —*Elementary Math* primero, no *Algebra* por empezar por A—.
/// Quien lea la lista de lo que falta la va a leer contra el árbol, no contra un
/// alfabeto.
pub fn from_directory(directory: &Path) -> Result<Vec<Capability>, CapabilityError> {
    let mut paths = Vec::new();
    let entries = std::fs::read_dir(directory).map_err(|error| CapabilityError::Read {
        path: directory.display().to_string(),
        message: error.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| CapabilityError::Read {
            path: directory.display().to_string(),
            message: error.to_string(),
        })?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "md") {
            paths.push(path);
        }
    }
    paths.sort();
    sort_by_index(&mut paths, directory);

    let mut capabilities = Vec::new();
    for path in &paths {
        let contents = std::fs::read_to_string(path).map_err(|error| CapabilityError::Read {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
        let file = path
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        capabilities.extend(from_document(&contents, &file));
    }

    if capabilities.is_empty() {
        return Err(CapabilityError::NoDocuments {
            directory: directory.display().to_string(),
        });
    }
    Ok(capabilities)
}

/// Reordena los archivos según el bloque `@import` de `main.md`.
///
/// Lo que no esté nombrado allí se queda detrás, en orden alfabético: un archivo
/// nuevo aparece en el árbol aunque nadie haya tocado el índice todavía. Si
/// `main.md` no existe o no tiene el bloque, el orden alfabético es el que queda,
/// que es una respuesta razonable y no un fallo.
fn sort_by_index(paths: &mut [PathBuf], directory: &Path) {
    let Ok(index) = std::fs::read_to_string(directory.join(MAIN_DOCUMENT)) else {
        return;
    };

    let order: Vec<String> = index
        .lines()
        .filter_map(|line| line.trim().strip_prefix("@import \""))
        .filter_map(|rest| rest.split('"').next())
        .map(ToOwned::to_owned)
        .collect();

    paths.sort_by_key(|path| {
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        order
            .iter()
            .position(|listed| *listed == name)
            .unwrap_or(usize::MAX)
    });
}

/// Las hojas de un documento, con su camino de secciones.
///
/// La profundidad la da la indentación, que es la que el inventario trae y la
/// única estructura que el archivo tiene. Una línea que no encaja en ninguna de
/// las dos formas —prosa, un título de Markdown, una línea en blanco— se ignora:
/// `main.md` es prosa entera y no aporta ninguna capacidad.
fn from_document(contents: &str, file: &str) -> Vec<Capability> {
    let reindexed = file == REINDEXED_FILE;
    let mut sections: Vec<(usize, String)> = Vec::new();
    let mut capabilities = Vec::new();

    for line in contents.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        if let Some(title) = section_title(trimmed) {
            sections.retain(|(depth, _)| *depth < indent);
            sections.push((indent, title));
            continue;
        }

        let Some(leaf) = leaf_title(trimmed) else {
            continue;
        };
        // Una hoja sin sección por encima sería una capacidad sin sitio en el
        // árbol. No las hay, y si algún día las hay, la que sobra es la línea.
        if sections.is_empty() {
            continue;
        }

        let mut path: Vec<String> = sections
            .iter()
            .map(|(_, section)| section.clone())
            .collect();
        path.push(leaf.to_owned());

        let mut id = String::new();
        for (position, part) in path.iter().enumerate() {
            if position > 0 {
                id.push_str(PATH_SEPARATOR);
            }
            let _ = write!(id, "{part}");
        }

        capabilities.push(Capability {
            id,
            path,
            file: file.to_owned(),
            reindexed,
        });
    }

    capabilities
}

/// `- [ ] Arithmetic: Perform multiple arithmetic operations.` → `Arithmetic`.
///
/// La descripción que va tras los dos puntos no entra en el id: es prosa descriptiva
/// y cambia sin que cambie la capacidad.
fn section_title(trimmed: &str) -> Option<String> {
    let rest = trimmed
        .strip_prefix("- [ ] ")
        .or_else(|| trimmed.strip_prefix("- [x] "))?;
    let title = rest.split(':').next().unwrap_or(rest).trim();
    Some(title.to_owned())
}

/// `Add numbers:` → `Add numbers`. Devuelve `None` si la línea no es una hoja.
///
/// Una hoja **termina** en `:` y no empieza por `-`, que es como vienen en el
/// inventario. Se quita ese último carácter y ya: los nombres llevan dos puntos
/// dentro —`Add and subtract whole numbers (CCSS.Math.Content.K.OA.A.5):`— y
/// cortar por el primero partiría la capacidad por la mitad.
fn leaf_title(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with('-') || trimmed.starts_with('#') {
        return None;
    }
    let title = trimmed.strip_suffix(':')?;
    if title.is_empty() {
        return None;
    }
    Some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
- [ ] Elementary Math
  - [ ] Arithmetic: Perform multiple arithmetic operations.
    - [ ] Addition & Subtraction: Perform addition and subtraction.
        Add numbers:
        Subtract numbers:
    - [ ] Powers & Roots: Compute powers and roots.
        Compute powers:
";

    #[test]
    fn a_leaf_carries_the_whole_path_that_leads_to_it() {
        let capabilities = from_document(SAMPLE, "elementary-math");

        let ids: Vec<&str> = capabilities.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "Elementary Math > Arithmetic > Addition & Subtraction > Add numbers",
                "Elementary Math > Arithmetic > Addition & Subtraction > Subtract numbers",
                "Elementary Math > Arithmetic > Powers & Roots > Compute powers",
            ]
        );
    }

    /// Salir de una sección hermana no puede dejar su título pegado al camino:
    /// «Powers & Roots» no está dentro de «Addition & Subtraction».
    #[test]
    fn a_sibling_section_replaces_the_previous_one_instead_of_nesting() {
        let capabilities = from_document(SAMPLE, "elementary-math");
        let last = capabilities.last().expect("hay hojas");

        assert!(!last.id.contains("Addition"));
    }

    #[test]
    fn the_curricular_index_is_marked_so_it_does_not_count_twice() {
        let capabilities = from_document(SAMPLE, REINDEXED_FILE);

        assert!(capabilities.iter().all(|capability| capability.reindexed));
    }

    /// Los nombres del Common Core traen dos puntos dentro del paréntesis del
    /// estándar. Cortar por el primero partiría la capacidad por la mitad.
    #[test]
    fn a_leaf_name_may_contain_colons_of_its_own() {
        let document = "\
- [ ] Common Core Math
  - [ ] Addition & Subtraction: Perform addition and subtraction.
      Add and subtract whole numbers (CCSS.Math.Content.K.OA.A.5):
";
        let capabilities = from_document(document, REINDEXED_FILE);

        assert_eq!(capabilities.len(), 1);
        assert!(
            capabilities
                .first()
                .is_some_and(|capability| capability.id.ends_with("(CCSS.Math.Content.K.OA.A.5)"))
        );
    }

    #[test]
    fn prose_contributes_no_capabilities() {
        let capabilities = from_document(
            "Mathematics\n\n## Análisis: qué tomaría replicar el motor\n\nTexto suelto.\n",
            "main",
        );

        assert!(capabilities.is_empty());
    }
}
