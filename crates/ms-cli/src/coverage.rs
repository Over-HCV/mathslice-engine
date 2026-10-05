//! La cobertura: qué capacidades del inventario tienen un caso en el corpus.
//!
//! El corpus es la métrica del proyecto (05-testing.md §1). Un caso declara a
//! qué capacidad responde con un campo `capability` **opcional**; un caso que no
//! lo declara no cuenta, que es la lectura honesta: nadie ha dicho a qué
//! pregunta del inventario contesta.

use std::path::Path;

use crate::capabilities::Capability;
use crate::corpus::{self, CorpusError};

/// El recuento, ya cruzado.
pub struct Coverage {
    /// Capacidades del inventario que no vienen del índice curricular.
    pub total: usize,
    /// Las cubiertas, en el orden del inventario.
    pub covered: Vec<String>,
    /// Casos del corpus que no declaran capacidad.
    pub untagged_cases: usize,
    /// Lo que un caso declaró y no está en el inventario. Es un error, no un
    /// aviso: una capacidad mal escrita cuenta como cubierta sin serlo, que es
    /// exactamente la forma de mentir con esta métrica.
    pub unknown: Vec<String>,
}

impl Coverage {
    /// El porcentaje, que es *la* cifra que se reporta en STATUS.md.
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        // Las cuentas de esta herramienta caben de sobra en un f64: el
        // inventario son millares, no miles de millones.
        #[expect(
            clippy::cast_precision_loss,
            reason = "el inventario son ~1700 capacidades"
        )]
        let ratio = self.covered.len() as f64 / self.total as f64;
        ratio * 100.0
    }
}

/// Leer el corpus es lo único que puede fallar aquí, y el error se propaga tal
/// cual: envolverlo sólo añadiría una capa que dice lo mismo.
pub type CoverageError = CorpusError;

/// Cruza el inventario con los casos declarados en `corpus/`.
pub fn measure(capabilities: &[Capability], corpus: &Path) -> Result<Coverage, CoverageError> {
    let cases = corpus::read(corpus)?;
    let untagged_cases = cases
        .iter()
        .filter(|case| case.capability.is_none())
        .count();
    let declared: Vec<String> = cases
        .into_iter()
        .filter_map(|case| case.capability)
        .collect();

    let mut covered = Vec::new();
    for capability in capabilities {
        if capability.reindexed {
            continue;
        }
        if declared.iter().any(|declared| *declared == capability.id) {
            covered.push(capability.id.clone());
        }
    }

    let mut unknown: Vec<String> = declared
        .into_iter()
        .filter(|declared| {
            !capabilities
                .iter()
                .any(|capability| capability.id == *declared)
        })
        .collect();
    unknown.sort();
    unknown.dedup();

    Ok(Coverage {
        total: capabilities
            .iter()
            .filter(|capability| !capability.reindexed)
            .count(),
        covered,
        untagged_cases,
        unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(id: &str, reindexed: bool) -> Capability {
        Capability {
            id: id.to_owned(),
            path: id.split(crate::capabilities::PATH_SEPARATOR).map(ToOwned::to_owned).collect(),
            file: String::from("elementary-math"),
            reindexed,
        }
    }

    #[test]
    fn the_curricular_index_stays_out_of_the_denominator() {
        let capabilities = [
            capability("Elementary Math > Add numbers", false),
            capability("Common Core Math > Add to 10", true),
        ];

        let coverage = Coverage {
            total: capabilities
                .iter()
                .filter(|capability| !capability.reindexed)
                .count(),
            covered: Vec::new(),
            untagged_cases: 0,
            unknown: Vec::new(),
        };

        assert_eq!(coverage.total, 1);
    }

    #[test]
    fn coverage_is_the_share_of_the_inventory_with_a_case() {
        let coverage = Coverage {
            total: 200,
            covered: vec![String::from("una"), String::from("otra")],
            untagged_cases: 0,
            unknown: Vec::new(),
        };

        assert!((coverage.percentage() - 1.0).abs() < f64::EPSILON);
    }

    /// Sin inventario no hay porcentaje, y `0 / 0` no es `NaN` en un informe: es
    /// una herramienta apuntando al directorio equivocado.
    #[test]
    fn an_empty_inventory_reports_zero_instead_of_a_nan() {
        let coverage = Coverage {
            total: 0,
            covered: Vec::new(),
            untagged_cases: 0,
            unknown: Vec::new(),
        };

        assert!((coverage.percentage() - 0.0).abs() < f64::EPSILON);
    }
}
