//! El árbol de capacidades con su cobertura, emitido como JSON por categoría.
//!
//! Es lo que lee `/motor` en el sitio. **Se genera; no se escribe a mano.** El
//! inventario vive en `docs/` y lo hecho vive en `corpus/`; aquí sólo se cruzan,
//! y por eso es imposible que este árbol diga «hecho» de una capacidad que no
//! tiene ningún caso.
//!
//! Un archivo por categoría, como los `.md` y por el mismo motivo: 1709 hojas en
//! un solo JSON no las lee nadie, ni una persona ni una página.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use crate::capabilities::Capability;
use crate::corpus::Case;

/// El índice, que es el archivo que lee la portada.
pub const INDEX_FILE: &str = "index.json";

/// Un nodo del árbol: una sección con hijos, o una hoja con sus casos.
struct Node {
    title: String,
    /// Sólo en las hojas: el id completo, que es lo que declara el corpus.
    id: Option<String>,
    children: Vec<Node>,
    cases: Vec<Case>,
}

impl Node {
    fn section(title: &str) -> Self {
        Node {
            title: title.to_owned(),
            id: None,
            children: Vec::new(),
            cases: Vec::new(),
        }
    }

    fn is_leaf(&self) -> bool {
        self.id.is_some()
    }

    /// Cuántas hojas cuelgan de aquí, y cuántas de ellas tienen caso.
    fn counts(&self) -> (usize, usize) {
        if self.is_leaf() {
            return (1, usize::from(!self.cases.is_empty()));
        }
        self.children
            .iter()
            .fold((0, 0), |(leaves, covered), child| {
                let (child_leaves, child_covered) = child.counts();
                (leaves + child_leaves, covered + child_covered)
            })
    }

    /// **La regla del check**: una hoja está marcada si tiene al menos un caso;
    /// una sección, si lo están todos sus hijos. Una sección sin hojas no está
    /// marcada — no hay nada que haya salido bien en ella.
    fn checked(&self) -> bool {
        let (leaves, covered) = self.counts();
        leaves > 0 && leaves == covered
    }

    fn to_json(&self) -> Value {
        let (leaves, covered) = self.counts();

        if self.is_leaf() {
            return json!({
                "title": self.title,
                "id": self.id,
                "checked": self.checked(),
                "cases": self.cases.iter().map(case_json).collect::<Vec<_>>(),
            });
        }

        json!({
            "title": self.title,
            "leaves": leaves,
            "covered": covered,
            "checked": self.checked(),
            "children": self.children.iter().map(Node::to_json).collect::<Vec<_>>(),
        })
    }
}

fn case_json(case: &Case) -> Value {
    json!({
        "name": case.name,
        "stage": case.stage,
        "latex": case.latex,
        "result_latex": case.result_latex,
        "error": case.error,
    })
}

/// Un archivo a escribir. Se devuelven en vez de escribirse para que
/// `tree --check` pueda compararlos con lo versionado sin tocar el disco.
pub struct Emitted {
    pub file: String,
    pub contents: String,
}

/// El índice y una categoría por archivo de `docs/`.
///
/// El orden de las categorías es el del inventario —el que tienen los `@import`
/// de `docs/main.md`—, no el alfabético: es el orden de origen y el
/// que la página enseña.
pub fn emit(capabilities: &[Capability], cases: &[Case]) -> Vec<Emitted> {
    let mut by_capability: BTreeMap<&str, Vec<&Case>> = BTreeMap::new();
    for case in cases {
        let Some(capability) = case.capability.as_deref() else {
            continue;
        };
        by_capability.entry(capability).or_default().push(case);
    }

    let mut files = Vec::new();
    let mut categories = Vec::new();
    let (mut total_leaves, mut total_covered) = (0, 0);

    for slug in category_slugs(capabilities) {
        let leaves: Vec<&Capability> = capabilities
            .iter()
            .filter(|capability| capability.file == slug)
            .collect();

        let Some(first) = leaves.first() else {
            continue;
        };
        let reindexed = first.reindexed;
        let title = first.path.first().cloned().unwrap_or_else(|| slug.clone());

        let mut root = Node::section(&title);
        for capability in &leaves {
            let cases = by_capability
                .get(capability.id.as_str())
                .map(|found| found.iter().map(|case| (*case).clone()).collect())
                .unwrap_or_default();
            insert(&mut root, capability, cases);
        }

        let (category_leaves, category_covered) = root.counts();
        // El índice curricular tiene su página y su porcentaje, pero no suma al
        // total: son las mismas capacidades re-etiquetadas por grado escolar.
        if !reindexed {
            total_leaves += category_leaves;
            total_covered += category_covered;
        }

        categories.push(json!({
            "slug": slug,
            "title": title,
            "leaves": category_leaves,
            "covered": category_covered,
            "percent": percentage(category_covered, category_leaves),
            "reindexed": reindexed,
        }));

        files.push(Emitted {
            file: format!("{slug}.json"),
            contents: render(&json!({
                "slug": slug,
                "title": title,
                "leaves": category_leaves,
                "covered": category_covered,
                "percent": percentage(category_covered, category_leaves),
                "reindexed": reindexed,
                "nodes": root.children.iter().map(Node::to_json).collect::<Vec<_>>(),
            })),
        });
    }

    files.push(Emitted {
        file: INDEX_FILE.to_owned(),
        contents: render(&json!({
            "note": "Generado por `ms-cli tree` cruzando engine/docs/ con engine/corpus/. \
                     No se edita a mano: `pnpm engine:tree` lo reescribe.",
            "totals": {
                "leaves": total_leaves,
                "covered": total_covered,
                "percent": percentage(total_covered, total_leaves),
            },
            "categories": categories,
        })),
    });

    files
}

/// Los slugs de categoría en el orden en que aparecen en el inventario.
fn category_slugs(capabilities: &[Capability]) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    for capability in capabilities {
        if !slugs.contains(&capability.file) {
            slugs.push(capability.file.clone());
        }
    }
    slugs
}

/// Cuelga una hoja de su camino, creando las secciones que falten.
///
/// La primera parte del camino es la categoría, que **es** la raíz: si se
/// insertara también como sección, cada categoría se contendría a sí misma.
fn insert(root: &mut Node, capability: &Capability, cases: Vec<Case>) {
    let Some((leaf, sections)) = capability.path.split_last() else {
        return;
    };
    let sections = sections.get(1..).unwrap_or_default();

    let mut current = root;
    for section in sections {
        let existing = current
            .children
            .iter()
            .position(|child| child.title == *section && !child.is_leaf());

        let position = match existing {
            Some(position) => position,
            None => {
                current.children.push(Node::section(section));
                current.children.len() - 1
            }
        };
        let Some(next) = current.children.get_mut(position) else {
            return;
        };
        current = next;
    }

    current.children.push(Node {
        title: leaf.clone(),
        id: Some(capability.id.clone()),
        children: Vec::new(),
        cases,
    });
}

/// El porcentaje con dos decimales, que es como se lee.
fn percentage(covered: usize, leaves: usize) -> f64 {
    if leaves == 0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "el inventario son ~1700 capacidades"
    )]
    let ratio = covered as f64 / leaves as f64;
    (ratio * 10_000.0).round() / 100.0
}

/// JSON indentado y con salto final: es un archivo versionado, y un diff que
/// cambia una línea tiene que verse como una línea.
fn render(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_else(|_| String::from("{}"));
    text.push('\n');
    text
}

/// Escribe lo emitido en `directory`, creándolo si hace falta.
pub fn write(directory: &Path, files: &[Emitted]) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(directory)?;
    for emitted in files {
        std::fs::write(directory.join(&emitted.file), &emitted.contents)?;
    }
    Ok(())
}

/// Los archivos cuyo contenido versionado no coincide con lo que sale hoy.
///
/// Un archivo que falta cuenta como distinto: la página estaría leyendo un árbol
/// que ya no existe.
pub fn stale(directory: &Path, files: &[Emitted]) -> Vec<String> {
    files
        .iter()
        .filter(|emitted| {
            std::fs::read_to_string(directory.join(&emitted.file))
                .is_ok_and(|found| found == emitted.contents)
                .eq(&false)
        })
        .map(|emitted| emitted.file.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(path: &[&str], file: &str) -> Capability {
        let path: Vec<String> = path.iter().map(|part| (*part).to_owned()).collect();
        Capability {
            id: path.join(" > "),
            path,
            file: file.to_owned(),
            reindexed: false,
        }
    }

    fn case(capability: Option<&str>) -> Case {
        Case {
            name: String::from("un-caso"),
            stage: String::from("E1"),
            latex: String::from("2+3"),
            result_latex: Some(String::from("5")),
            error: None,
            capability: capability.map(ToOwned::to_owned),
        }
    }

    fn category(files: &[Emitted], name: &str) -> Value {
        let emitted = files
            .iter()
            .find(|emitted| emitted.file == name)
            .expect("el archivo se emitió");
        serde_json::from_str(&emitted.contents).expect("JSON válido")
    }

    /// La regla del check: el padre se marca cuando **todos** sus hijos lo están.
    #[test]
    fn a_section_is_checked_only_when_every_leaf_under_it_is() {
        let capabilities = [
            capability(&["Elementary Math", "Arithmetic", "Add numbers"], "elementary-math"),
            capability(&["Elementary Math", "Arithmetic", "Subtract numbers"], "elementary-math"),
        ];

        let half = emit(&capabilities, &[case(Some("Elementary Math > Arithmetic > Add numbers"))]);
        let section = &category(&half, "elementary-math.json")["nodes"][0];
        assert_eq!(section["checked"], json!(false));
        assert_eq!(section["covered"], json!(1));
        assert_eq!(section["leaves"], json!(2));

        let whole = emit(
            &capabilities,
            &[
                case(Some("Elementary Math > Arithmetic > Add numbers")),
                case(Some("Elementary Math > Arithmetic > Subtract numbers")),
            ],
        );
        assert_eq!(category(&whole, "elementary-math.json")["nodes"][0]["checked"], json!(true));
    }

    /// Una hoja sin caso no está marcada, y llega con la lista de casos vacía:
    /// es lo que la página pinta desactivado.
    #[test]
    fn a_leaf_without_a_case_is_unchecked_and_carries_no_cases() {
        let capabilities = [capability(&["Numbers", "Integers", "Compute properties"], "numbers")];

        let files = emit(&capabilities, &[]);
        let leaf = &category(&files, "numbers.json")["nodes"][0]["children"][0];

        assert_eq!(leaf["checked"], json!(false));
        assert_eq!(leaf["cases"], json!([]));
    }

    /// La categoría es la raíz del archivo, no una sección dentro de él: si se
    /// insertara también como hija, cada categoría se contendría a sí misma.
    #[test]
    fn the_category_is_the_root_and_not_a_node_inside_itself() {
        let capabilities = [capability(&["Numbers", "Integers", "Compute properties"], "numbers")];

        let files = emit(&capabilities, &[]);
        let document = category(&files, "numbers.json");

        assert_eq!(document["title"], json!("Numbers"));
        assert_eq!(document["nodes"][0]["title"], json!("Integers"));
    }

    /// El índice curricular no suma al total, igual que en `ms-cli coverage`.
    #[test]
    fn the_curricular_index_does_not_add_to_the_global_total() {
        let mut reindexed = capability(&["Common Core Math", "Grade 1", "Add to 10"], "common-core-math");
        reindexed.reindexed = true;
        let capabilities = [
            capability(&["Elementary Math", "Arithmetic", "Add numbers"], "elementary-math"),
            reindexed,
        ];

        let files = emit(&capabilities, &[]);
        let index = category(&files, INDEX_FILE);

        assert_eq!(index["totals"]["leaves"], json!(1));
        // Pero su página existe y cuenta sus propias hojas.
        assert_eq!(category(&files, "common-core-math.json")["leaves"], json!(1));
    }

    /// Un caso sin capacidad no cuelga de ninguna hoja. No es un error: los que
    /// fijan un error o un límite no responden a ninguna capacidad.
    #[test]
    fn a_case_without_a_capability_hangs_from_nothing() {
        let capabilities = [capability(&["Numbers", "Integers", "Compute properties"], "numbers")];

        let files = emit(&capabilities, &[case(None)]);

        assert_eq!(category(&files, INDEX_FILE)["totals"]["covered"], json!(0));
    }
}
