//! `ms-cli` — la métrica del proyecto, calculada en vez de estimada.
//!
//! `05-testing.md` §1 dice que el porcentaje de capacidades cubiertas es *la*
//! métrica del proyecto y que sale de cruzar `engine/docs/` con `engine/corpus/`.
//! Hasta ahora ese número estaba escrito a mano en `context/STATUS.md`, y un
//! número escrito a mano no es una métrica: es una impresión con formato.
//!
//! ```sh
//! ms-cli coverage              # el porcentaje y lo que falta por declarar
//! ms-cli capabilities [texto]  # busca capacidades por su nombre
//! ms-cli tree [--check]        # emite engine/tree/, que es lo que lee /motor
//! ```
//!
//! No depende de ningún crate del motor y no entra en el `.wasm`: es una
//! herramienta de escritorio que lee dos directorios de texto.

mod capabilities;
mod corpus;
mod coverage;
mod tree;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// La raíz de `engine/`, resuelta en tiempo de compilación desde el manifiesto
/// de este crate.
///
/// Es una herramienta de desarrollo que se ejecuta con `cargo run` dentro del
/// repositorio, así que la raíz se conoce: `crates/ms-cli` está siempre dos
/// niveles por debajo. Buscarla en tiempo de ejecución sería adivinar desde
/// dónde la lanzaron.
const CRATE_DIRECTORY: &str = env!("CARGO_MANIFEST_DIR");

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let command = arguments.first().map(String::as_str);

    match command {
        Some("coverage") => report_coverage(),
        Some("capabilities") => list_capabilities(arguments.get(1).map(String::as_str)),
        Some("tree") => emit_tree(arguments.iter().any(|argument| argument == "--check")),
        Some(other) => {
            eprintln!("«{other}» no es un comando. Los que hay: coverage, capabilities, tree.");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("Uso: ms-cli coverage | ms-cli capabilities [texto] | ms-cli tree [--check]");
            ExitCode::FAILURE
        }
    }
}

/// Escribe una línea en la salida, y trata el pipe roto como final normal.
///
/// `ms-cli capabilities | head` cierra la salida a la mitad, y el `println!` de
/// la biblioteca estándar entra en pánico cuando eso pasa. La herramienta se va
/// a usar así todo el rato: quien filtra la lista con `head` o con `rg` no ha
/// hecho nada mal, y no puede acabar leyendo un pánico.
fn line(text: &str) -> Result<(), std::io::Error> {
    writeln!(std::io::stdout(), "{text}")
}

/// Igual que [`line`], pero devolviendo el código de salida ya decidido: el pipe
/// roto sale con éxito, cualquier otro fallo de escritura sale con error.
fn write_lines(lines: impl IntoIterator<Item = String>) -> ExitCode {
    for text in lines {
        let Err(error) = line(&text) else {
            continue;
        };
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            return ExitCode::SUCCESS;
        }
        eprintln!("no se pudo escribir la salida: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn engine_root() -> PathBuf {
    Path::new(CRATE_DIRECTORY)
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(CRATE_DIRECTORY))
}

fn report_coverage() -> ExitCode {
    let root = engine_root();

    let inventory = match capabilities::from_directory(&root.join("docs")) {
        Ok(inventory) => inventory,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let measured = match coverage::measure(&inventory, &root.join("corpus")) {
        Ok(measured) => measured,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let reindexed = inventory.len() - measured.total;
    let mut report = vec![
        format!("Inventario: {} capacidades hoja", inventory.len()),
        format!("  de las que {reindexed} son el índice curricular y no cuentan dos veces"),
        format!("Superficie real: {} capacidades", measured.total),
        String::new(),
        format!(
            "Cobertura: {} de {} — {:.2} %",
            measured.covered.len(),
            measured.total,
            measured.percentage()
        ),
        format!(
            "Casos del corpus sin capacidad declarada: {}",
            measured.untagged_cases
        ),
    ];

    if !measured.covered.is_empty() {
        report.push(String::new());
        report.push(String::from("Cubiertas:"));
        report.extend(
            measured
                .covered
                .iter()
                .map(|capability| format!("  {capability}")),
        );
    }

    let written = write_lines(report);
    if measured.unknown.is_empty() {
        return written;
    }

    // Falla a propósito: una capacidad mal escrita en un caso se contaría como
    // cubierta sin estarlo, y una métrica que se puede inflar con una errata no
    // sirve para nada.
    eprintln!();
    eprintln!("Capacidades declaradas en el corpus que no están en el inventario:");
    for unknown in &measured.unknown {
        eprintln!("  {unknown}");
    }
    ExitCode::FAILURE
}

/// El directorio que lee `/motor`, relativo a la raíz de `engine/`.
const TREE_DIRECTORY: &str = "tree";

/// Emite `engine/tree/`, o comprueba que lo versionado sigue siendo lo que sale.
///
/// `--check` no escribe nada y falla si algo cambió. Sin eso, un caso nuevo en el
/// corpus deja la página diciendo que una capacidad no está hecha —o al revés—
/// hasta que alguien se acuerde de regenerar, y una página que miente sobre el
/// progreso es peor que no tenerla.
fn emit_tree(check_only: bool) -> ExitCode {
    let root = engine_root();

    let inventory = match capabilities::from_directory(&root.join("docs")) {
        Ok(inventory) => inventory,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let cases = match corpus::read(&root.join("corpus")) {
        Ok(cases) => cases,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let files = tree::emit(&inventory, &cases);
    let directory = root.join(TREE_DIRECTORY);

    if check_only {
        let stale = tree::stale(&directory, &files);
        if stale.is_empty() {
            return write_lines([format!("engine/{TREE_DIRECTORY}/ está al día ({} archivos)", files.len())]);
        }
        eprintln!("engine/{TREE_DIRECTORY}/ no coincide con docs/ y corpus/. Corre `pnpm engine:tree`.");
        for file in &stale {
            eprintln!("  {file}");
        }
        return ExitCode::FAILURE;
    }

    if let Err(error) = tree::write(&directory, &files) {
        eprintln!("no se pudo escribir engine/{TREE_DIRECTORY}/: {error}");
        return ExitCode::FAILURE;
    }
    write_lines([format!(
        "engine/{TREE_DIRECTORY}/ reescrito: {} archivos",
        files.len()
    )])
}

fn list_capabilities(needle: Option<&str>) -> ExitCode {
    let root = engine_root();
    let inventory = match capabilities::from_directory(&root.join("docs")) {
        Ok(inventory) => inventory,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let needle = needle.unwrap_or_default().to_lowercase();
    let matched: Vec<&str> = inventory
        .iter()
        .filter(|capability| capability.id.to_lowercase().contains(&needle))
        .map(|capability| capability.id.as_str())
        .collect();

    let mut lines: Vec<String> = matched.iter().map(|id| (*id).to_owned()).collect();
    lines.push(String::new());
    lines.push(format!("{} capacidades", matched.len()));

    write_lines(lines)
}
