//! Línea base del matcher, tomada en E0 a propósito.
//!
//! El matcher es lo que se degrada **en silencio** conforme crece el rule set:
//! pasar de 5 reglas a las ~6700 de Rubi sin medir es cómo se muere el
//! rendimiento sin que nadie sepa cuándo (01-rust-principles.md §10).
//!
//! Una medida tomada hoy, con barrido lineal, es la que dirá si la red de
//! discriminación de E2 sirve de algo. Tomada al final no vale para nada: no
//! habría con qué compararla.

// Un bench tampoco es código de producción: no se envía al navegador y un fallo
// aquí es un bench roto, no una sesión perdida.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use criterion::{Criterion, criterion_group, criterion_main};
use ms_assume::Assumptions;
use ms_core::{Arena, ExprId};
use ms_rules::{DEFAULT_BUDGET, RULES, rewrite};
use std::hint::black_box;

fn fraction(arena: &mut Arena, numerator: i64, denominator: i64) -> ExprId {
    let numerator = arena.int(numerator).expect("arena");
    let denominator = arena.int(denominator).expect("arena");
    let minus_one = arena.int(-1).expect("arena");
    let reciprocal = arena.power(denominator, minus_one).expect("arena");
    arena.times(&[numerator, reciprocal]).expect("arena")
}

/// Suma de fracciones con denominadores primos entre sí, que es el caso caro:
/// el mcm crece y ninguna reducción intermedia lo abarata.
fn coprime_sum(arena: &mut Arena, terms: usize) -> ExprId {
    let denominators = [3_i64, 5, 7, 11, 13, 17, 19, 23];
    let summands: Vec<ExprId> = (0..terms)
        .map(|index| {
            let denominator = denominators[index % denominators.len()];
            fraction(arena, 1, denominator)
        })
        .collect();
    arena.plus(&summands).expect("arena")
}

fn matcher(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("rewrite");
    // Contexto vacío: es el de una consulta escrita en la caja de texto, y es lo
    // que mide la línea base contra la que se compararán las etapas siguientes.
    let assumptions = Assumptions::new();

    group.bench_function("exit-criterion", |bencher| {
        bencher.iter(|| {
            let mut arena = Arena::new();
            let two_thirds = fraction(&mut arena, 2, 3);
            let one_sixth = fraction(&mut arena, 1, 6);
            let sum = arena.plus(&[two_thirds, one_sixth]).expect("arena");
            let outcome = rewrite(&mut arena, black_box(sum), RULES, &assumptions, DEFAULT_BUDGET);
            black_box(outcome).expect("rewrite");
        });
    });

    for terms in [4_usize, 8] {
        group.bench_function(format!("coprime-sum/{terms}"), |bencher| {
            bencher.iter(|| {
                let mut arena = Arena::new();
                let sum = coprime_sum(&mut arena, terms);
                let outcome = rewrite(&mut arena, black_box(sum), RULES, &assumptions, DEFAULT_BUDGET);
                black_box(outcome).expect("rewrite");
            });
        });
    }

    group.finish();
    sweep(criterion);
}

/// El coste del barrido lineal por regla, medido **dentro de la misma sesión**.
///
/// Las cifras de una etapa contra las de la anterior no se pueden restar: entre
/// la fase 3 y la 4, sin tocar una línea, las mismas reglas pasaron de 5,25 µs a
/// 5,64 µs, y criterion lo anunció como una regresión del 7 %. Era el portátil, y
/// la deriva entre sesiones es del mismo orden que el coste de una regla.
///
/// Esto lo esquiva: una expresión **ya en forma normal** —ninguna regla encaja,
/// con ningún tamaño de rule set— barrida con 3, 6, 9, 12, 16 y todas las reglas. El trabajo es
/// idéntico salvo por la longitud del barrido, así que la pendiente entre las
/// medidas es el coste de una regla, y todas salen del mismo minuto
/// en la misma máquina.
///
/// Es la medida con la que se decide la red de discriminación: si el barrido
/// crece lineal y el punto de partida es de microsegundos, con 30 reglas sigue
/// sin justificarse.
fn sweep(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("sweep");
    let assumptions = Assumptions::new();

    for rules in [3_usize, 6, 9, 12, 16, RULES.len()] {
        let subset = &RULES[..rules];
        group.bench_function(format!("normal-form/{rules}-rules"), |bencher| {
            bencher.iter(|| {
                let mut arena = Arena::new();
                let x = arena.sym("x").expect("arena");
                let y = arena.sym("y").expect("arena");
                let sum = arena.plus(&[x, y]).expect("arena");
                let outcome =
                    rewrite(&mut arena, black_box(sum), subset, &assumptions, DEFAULT_BUDGET);
                black_box(outcome).expect("rewrite");
            });
        });
    }

    group.finish();
}

criterion_group!(benches, matcher);
criterion_main!(benches);
