//! Los puntos de una expresión, para que los pinte quien quiera.
//!
//! # Por qué se ensambla aquí y no en `ms-facts`
//!
//! `ms-facts` sabe de **números**: depende sólo de `ms-num` y no ve la arena.
//! Muestrear una expresión exige evaluarla, que es de `ms-verify`, y los ceros y
//! los puntos críticos exigirían además `ms-alg` y `ms-solve`. Meter todo eso
//! bajo `ms-facts` abriría tres aristas nuevas en `deny.toml` para no ganar nada:
//! la frontera ya ve los tres crates, y ensamblar el documento de salida es
//! exactamente su trabajo (02-architecture.md §2).
//!
//! # Qué se emite, y qué no
//!
//! Puntos exactos y el nombre de la variable. **Ni ejes, ni rango de vista, ni
//! color**: eso es representación y vive en TypeScript. El motor no sabe —ni
//! tiene por qué— con qué se va a dibujar.

use ms_core::{Arena, ExprId, Node, SymId};
use ms_num::Rational;
use ms_serialize::{FactDoc, GraphPoint};
use ms_verify::evaluate_bound;
use num_traits::One;

/// De dónde a dónde se muestrea, y con qué paso.
///
/// Una rejilla fija y simétrica alrededor del cero: es la vista que un
/// estudiante espera de una función sin más contexto, y es reproducible, que es
/// lo mismo que se le exige al muestreo del verificador.
const FIRST_HALF_STEP: i64 = -20;
const LAST_HALF_STEP: i64 = 20;
/// El paso es medio: `x` va en `-10, -9.5, …, 10`. Con paso entero, una parábola
/// se ve como una poligonal de once vértices.
const HALF_STEP_DENOMINATOR: i64 = 2;

/// Cuántos puntos hacen falta para que esto sea una curva y no un adorno.
///
/// `\frac{1}{x}` pierde uno —el cero— y sigue siendo perfectamente dibujable;
/// una expresión que sólo se puede evaluar en dos sitios, no.
const MINIMUM_POINTS: usize = 8;

/// Los puntos de `expression`, si es una expresión de una sola variable libre.
///
/// `None` cuando no lo es: un número no tiene gráfica, y con dos variables lo que
/// hay es una superficie, que es otra cosa y otra etapa.
pub fn samples(arena: &Arena, expression: ExprId) -> Option<FactDoc> {
    let variable = single_free_symbol(arena, expression)?;
    let name = arena.symbol_name(variable)?.to_owned();

    let mut points = Vec::new();
    for step in FIRST_HALF_STEP..=LAST_HALF_STEP {
        let x = Rational::new(step.into(), HALF_STEP_DENOMINATOR.into());
        // Un punto donde la expresión no está definida se salta. El hueco es
        // parte de la curva; taparlo con un cero sería inventarse un punto.
        let Some(y) = evaluate_bound(arena, expression, Some((variable, &x))) else {
            continue;
        };
        points.push(GraphPoint {
            x: write_rational(&x),
            y: write_rational(&y),
        });
    }

    if points.len() < MINIMUM_POINTS {
        return None;
    }
    Some(FactDoc::GraphSamples {
        variable: name,
        points,
    })
}

/// `"3"`, `"-1/2"`. Exacto y sin decidir cuántos decimales se enseñan, que es
/// una decisión de quien dibuja.
fn write_rational(value: &Rational) -> String {
    if value.denom().is_one() {
        return value.numer().to_string();
    }
    format!("{}/{}", value.numer(), value.denom())
}

/// La única variable libre, si hay exactamente una.
///
/// Es la misma pregunta que se hace `ms-verify` para decidir si puede muestrear,
/// y está escrita dos veces a propósito: exportarla desde allí ataría la forma
/// del documento de salida a un detalle del verificador, y son dos cosas que
/// cambian por motivos distintos.
fn single_free_symbol(arena: &Arena, id: ExprId) -> Option<SymId> {
    let mut found = None;
    collect_symbol(arena, id, &mut found)?;
    found
}

fn collect_symbol(arena: &Arena, id: ExprId, found: &mut Option<SymId>) -> Option<()> {
    match arena.get(id)? {
        Node::Int(_) | Node::Rat(_) => Some(()),
        Node::Sym(symbol) => match found {
            Some(known) if known != symbol => None,
            Some(_) => Some(()),
            None => {
                *found = Some(*symbol);
                Some(())
            }
        },
        Node::App { args, .. } => {
            for arg in arena.args_of(*args)?.to_vec() {
                collect_symbol(arena, arg, found)?;
            }
            Some(())
        }
    }
}
