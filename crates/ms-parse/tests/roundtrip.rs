//! `parse(latex(e)) == e` — la propiedad más importante del proyecto.
//!
//! LaTeX es la única entrada del motor. Si el round-trip falla, todo lo demás es
//! humo: el usuario ve una expresión y el kernel calcula otra (05-testing.md §3).
//!
//! La propiedad se enuncia sobre **expresiones canónicas**, que es lo único que
//! el motor manipula: `canonicalize(parse(latex(e))) == e` para todo `e`
//! canónico. Sin canonicalizar sería falsa por razones sin interés — `Plus(a,b)`
//! y `Plus(b,a)` son la misma expresión y sólo una de las dos se puede escribir.

// Los `allow-*-in-tests` de clippy.toml sólo alcanzan a los módulos
// `#[cfg(test)]`; un test de integración es un crate aparte y no los ve. Los
// lints de §6 protegen el código que se envía al navegador, y esto no se envía:
// aquí un `expect` ES la aserción.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use ms_core::{Arena, Builtin, ExprId, Head};
use ms_parse::parse;
use ms_serialize::to_latex;
use proptest::prelude::*;

/// Un árbol de expresión abstracto, generado antes de tocar la arena.
#[derive(Clone, Debug)]
enum Shape {
    Int(i32),
    Sym(char),
    Plus(Box<Shape>, Box<Shape>),
    Times(Box<Shape>, Box<Shape>),
    Power(Box<Shape>, Box<Shape>),
    /// `Times(a, Power(b, -1))`, la forma en la que vive una fracción.
    Fraction(Box<Shape>, Box<Shape>),
    /// `\operatorname{redondear}(x, n)`. Entra en el generador porque una cabeza
    /// nueva que el escritor sabe escribir y el parser no sabe leer rompería el
    /// round-trip sin que ningún caso lo notara.
    Round(Box<Shape>, i32),
    /// `\frac{d}{dx}\left(f\right)`. Es la cabeza que **no** se escribe con
    /// `\operatorname`, así que es la que más fácil rompe el round-trip: el
    /// escritor emite una forma que el parser sólo reconoce por un caso especial.
    Derivative(Box<Shape>, char),
}

/// Lo que puede ser una consulta entera. `Equal` y `Solutions` sólo viven en la
/// raíz, así que se generan aquí y no dentro de [`Shape`]: meterlas en el
/// generador recursivo produciría formas que el parser rechaza a propósito, y el
/// round-trip fallaría por la razón equivocada.
#[derive(Clone, Debug)]
enum Root {
    Bare(Shape),
    Equation(Shape, Shape),
    /// De una a tres soluciones. Cada una es una ecuación, que es lo que
    /// `ms-solve` mete dentro del conjunto.
    Solutions(Vec<(Shape, Shape)>),
}

/// El generador se queda dentro de la imagen del parser a propósito: no produce
/// `Rat`, porque el parser nunca lo emite (`\frac{5}{6}` entra como
/// `Times(5, Power(6, -1))`), y una propiedad sobre expresiones que la entrada
/// no puede producir no dice nada sobre el round-trip.
fn shape() -> impl Strategy<Value = Shape> {
    let leaf = prop_oneof![
        (-20i32..20).prop_map(Shape::Int),
        prop::sample::select(vec!['x', 'y', 'z', 'a', 'b']).prop_map(Shape::Sym),
    ];

    leaf.prop_recursive(4, 24, 2, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone())
                .prop_map(|(l, r)| Shape::Plus(Box::new(l), Box::new(r))),
            (inner.clone(), inner.clone())
                .prop_map(|(l, r)| Shape::Times(Box::new(l), Box::new(r))),
            // Exponentes pequeños y no negativos: `x^{-1}` es una fracción, y
            // generarlo por los dos caminos sólo probaría el generador.
            (inner.clone(), (0i32..5))
                .prop_map(|(base, e)| Shape::Power(Box::new(base), Box::new(Shape::Int(e)))),
            (inner.clone(), inner.clone())
                .prop_map(|(n, d)| Shape::Fraction(Box::new(n), Box::new(d))),
            // Cifras en los dos sentidos: redondear a centenas también se
            // escribe y se lee.
            (inner.clone(), (-3i32..6)).prop_map(|(x, n)| Shape::Round(Box::new(x), n)),
            // La variable sale del mismo repertorio que las hojas y **nunca es
            // `d`**: `\frac{d}{dd}` no dice cuál de las dos letras es el
            // diferencial, y el parser lo rechaza a propósito.
            (inner, prop::sample::select(vec!['x', 'y', 'z', 'a', 'b']))
                .prop_map(|(f, v)| Shape::Derivative(Box::new(f), v)),
        ]
    })
}

fn root() -> impl Strategy<Value = Root> {
    prop_oneof![
        shape().prop_map(Root::Bare),
        (shape(), shape()).prop_map(|(left, right)| Root::Equation(left, right)),
        prop::collection::vec((shape(), shape()), 1..4).prop_map(Root::Solutions),
    ]
}

fn build_root(arena: &mut Arena, root: &Root) -> ExprId {
    match root {
        Root::Bare(shape) => build(arena, shape),
        Root::Equation(left, right) => {
            let (left, right) = (build(arena, left), build(arena, right));
            arena.app(Head::Equal, &[left, right]).expect("arena")
        }
        Root::Solutions(solutions) => {
            let equations: Vec<ExprId> = solutions
                .iter()
                .map(|(left, right)| {
                    let (left, right) = (build(arena, left), build(arena, right));
                    arena.app(Head::Equal, &[left, right]).expect("arena")
                })
                .collect();
            arena.app(Head::Solutions, &equations).expect("arena")
        }
    }
}

fn build(arena: &mut Arena, shape: &Shape) -> ExprId {
    match shape {
        Shape::Int(value) => arena.int(i64::from(*value)).expect("arena"),
        Shape::Sym(name) => arena.sym(&name.to_string()).expect("arena"),
        Shape::Plus(left, right) => {
            let (left, right) = (build(arena, left), build(arena, right));
            arena.plus(&[left, right]).expect("arena")
        }
        Shape::Times(left, right) => {
            let (left, right) = (build(arena, left), build(arena, right));
            arena.times(&[left, right]).expect("arena")
        }
        Shape::Power(base, exponent) => {
            let (base, exponent) = (build(arena, base), build(arena, exponent));
            arena.power(base, exponent).expect("arena")
        }
        Shape::Fraction(numerator, denominator) => {
            let numerator = build(arena, numerator);
            let denominator = build(arena, denominator);
            let minus_one = arena.int(-1).expect("arena");
            let reciprocal = arena.power(denominator, minus_one).expect("arena");
            arena.times(&[numerator, reciprocal]).expect("arena")
        }
        Shape::Round(value, digits) => {
            let value = build(arena, value);
            let digits = arena.int(i64::from(*digits)).expect("arena");
            arena
                .app(Head::Fn(Builtin::Round), &[value, digits])
                .expect("arena")
        }
        Shape::Derivative(operand, variable) => {
            let operand = build(arena, operand);
            let variable = arena.sym(&variable.to_string()).expect("arena");
            arena
                .app(Head::Fn(Builtin::Derivative), &[operand, variable])
                .expect("arena")
        }
    }
}

proptest! {
    #[test]
    fn parsing_the_rendered_latex_gives_back_the_same_expression(shape in root()) {
        let mut arena = Arena::new();

        let built = build_root(&mut arena, &shape);
        let original = arena.canonicalize(built).expect("canonicalize").id();

        let latex = to_latex(&arena, original).expect("to_latex");

        let reparsed = parse(&mut arena, &latex)
            .unwrap_or_else(|error| panic!("no se pudo reparsear «{latex}»: {error}"));
        let reparsed = arena.canonicalize(reparsed).expect("canonicalize").id();

        prop_assert_eq!(
            original,
            reparsed,
            "«{}» volvió a entrar como «{}»",
            latex,
            to_latex(&arena, reparsed).expect("to_latex")
        );
    }
}
