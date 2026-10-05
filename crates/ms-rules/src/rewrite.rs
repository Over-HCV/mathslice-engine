//! El reescritor: aplica reglas hasta forma normal y va anotando la traza.
//!
//! Estrategia de E0: **barrido lineal, de abajo arriba**. Se prueba cada regla
//! sobre cada subexpresión en post-orden y se aplica la primera que encaje; se
//! repite hasta que ninguna aplique. La red de discriminación de
//! 02-architecture.md §2 llega en E2 — con seis reglas sería adorno, y el bench
//! de `benches/matcher.rs` está precisamente para que se vea cuándo deja de serlo.
//!
//! Dos frenos, porque un reescritor sin ellos cuelga la pestaña del usuario:
//! presupuesto de iteraciones y detección de ciclos por id (§2).

use ms_assume::Assumptions;
use ms_core::{Arena, ExprId, Node};
use rustc_hash::FxHashSet;

use crate::fraction::has_division_by_zero;
use crate::rule::{Rule, RuleError, Soundness};

/// Tope de reescrituras. Generoso para E0 y aun así finito: agotarlo se reporta
/// como `BudgetExhausted`, nunca como un cuelgue.
pub const DEFAULT_BUDGET: u32 = 512;

/// Un paso de la derivación, tal y como sale del reescritor. `ms-serialize` lo
/// convierte en el `TraceStep` del contrato, con su LaTeX.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Step {
    pub rule: &'static str,
    pub title: &'static str,
    pub step_template: &'static str,
    /// El teorema que respalda la regla que dio este paso, si lo hay.
    pub soundness: Option<&'static str>,
    pub before: ExprId,
    pub after: ExprId,
    /// Los índices de argumento desde la raíz hasta el subtérmino donde disparó
    /// la regla. Vacío significa que disparó en la raíz.
    ///
    /// `before` y `after` son fotos del árbol entero, y sin el locus no hay forma
    /// de decir *dónde* pasó lo que pasó. Con una suma de fracciones no se nota;
    /// con «derivamos este subtérmino» en E3, sí.
    pub path: Vec<usize>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Rewritten {
    pub expression: ExprId,
    pub steps: Vec<Step>,
}

/// Reescribe hasta forma normal, canonicalizando entre paso y paso.
///
/// La división entre cero se comprueba en **cada** estado, no sólo en la entrada:
/// `\frac{2}{1-1}` sólo tiene un cero en el denominador después de plegar la
/// resta, y comprobarlo una vez al principio dejaba pasar un resultado
/// indefinido como si fuera una respuesta.
///
/// Cuesta un 5 % en el bench del matcher, medido. Es el precio de no devolver un
/// resultado indefinido como si fuera una respuesta, y a 5 µs por consulta contra
/// los 30 ms que tarda el WASM en arrancar, es un precio que no se nota.
pub fn rewrite(
    arena: &mut Arena,
    expression: ExprId,
    rules: &[Rule],
    assumptions: &Assumptions,
    budget: u32,
) -> Result<Rewritten, RuleError> {
    let mut current = arena.canonicalize(expression)?.id();
    if has_division_by_zero(arena, current) {
        return Err(RuleError::DivisionByZero);
    }
    let mut steps = Vec::new();
    let mut seen = FxHashSet::default();
    seen.insert(current);

    for iteration in 0..budget {
        let Some(applied) = apply_once(arena, current, rules, assumptions)? else {
            return Ok(Rewritten {
                expression: current,
                steps,
            });
        };

        let after = arena.canonicalize(applied.result)?.id();

        // Un paso que destapa un denominador nulo no se cuenta como paso: la
        // derivación termina aquí y lo que sale es la explicación.
        if has_division_by_zero(arena, after) {
            return Err(RuleError::DivisionByZero);
        }

        // Una regla que devuelve la misma expresión es una regla que no hizo
        // nada: contarla como paso llenaría la traza de ruido.
        if after == current {
            return Ok(Rewritten {
                expression: current,
                steps,
            });
        }

        if !seen.insert(after) {
            return Err(RuleError::Cycle {
                rule: applied.rule.id,
            });
        }

        if !applied.rule.silent {
            steps.push(Step {
                rule: applied.rule.id,
                title: applied.rule.title,
                step_template: applied.rule.step_template,
                soundness: applied.rule.soundness.and_then(Soundness::theorem),
                before: current,
                after,
                path: applied.path(),
            });
        }

        current = after;

        if iteration + 1 == budget {
            return Err(RuleError::BudgetExhausted { iterations: budget });
        }
    }

    Ok(Rewritten {
        expression: current,
        steps,
    })
}

struct Applied<'a> {
    rule: &'a Rule,
    result: ExprId,
    /// Se construye al volver de la recursión, así que sale invertido: el índice
    /// más profundo primero. Quien lo publica lo pone del derecho.
    reversed_path: Vec<usize>,
}

impl Applied<'_> {
    /// El locus desde la raíz, que es como se lee.
    ///
    /// Los índices son los del árbol **canónico**, que es el que `rewrite` tiene
    /// en la mano y el que acaba en `Step::before`. Contra el árbol tal y como lo
    /// escribió el usuario no significarían nada: la canonicalización reordena
    /// los argumentos de las cabezas conmutativas.
    fn path(&self) -> Vec<usize> {
        self.reversed_path.iter().rev().copied().collect()
    }
}

/// Primera regla que encaje, en **pre-orden**: el padre antes que los hijos.
///
/// El orden no es indiferente, es lo que evita un ciclo real. Tras poner
/// denominador común, `\frac{1}{6} + \frac{4}{6}` tiene un sumando reducible por
/// su cuenta: de abajo arriba, «simplificar» devolvería `\frac{4}{6}` a
/// `\frac{2}{3}` y desharía el paso anterior, una y otra vez. De arriba abajo,
/// la suma de numeradores colapsa el `Plus` antes de que nadie mire un sumando
/// aislado.
///
/// La detección de ciclos de `rewrite` seguiría atrapándolo, pero como error.
/// Esto lo evita.
fn apply_once<'a>(
    arena: &mut Arena,
    id: ExprId,
    rules: &'a [Rule],
    assumptions: &Assumptions,
) -> Result<Option<Applied<'a>>, RuleError> {
    for rule in rules {
        if let Some(result) = (rule.apply)(arena, id, assumptions)? {
            return Ok(Some(Applied {
                rule,
                result,
                reversed_path: Vec::new(),
            }));
        }
    }

    let children = match arena.get(id) {
        Some(Node::App { args, .. }) => arena
            .args_of(*args)
            .ok_or(ms_core::CoreError::UnknownExpr(id))?
            .to_vec(),
        Some(Node::Int(_) | Node::Rat(_) | Node::Sym(_)) | None => Vec::new(),
    };

    for (position, child) in children.iter().enumerate() {
        let Some(mut applied) = apply_once(arena, *child, rules, assumptions)? else {
            continue;
        };
        // Reconstruye el padre con el hijo reescrito. Insertar, nunca mutar (§2).
        let mut rebuilt = children.clone();
        let Some(slot) = rebuilt.get_mut(position) else {
            continue;
        };
        *slot = applied.result;

        let head = arena
            .head_of(id)
            .ok_or(ms_core::CoreError::UnknownExpr(id))?;
        applied.result = arena.app(head, &rebuilt)?;
        applied.reversed_path.push(position);
        return Ok(Some(applied));
    }

    Ok(None)
}
