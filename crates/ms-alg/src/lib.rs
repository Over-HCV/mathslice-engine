//! `ms-alg` — polinomios y álgebra exacta.
//!
//! Dos mitades con reglas distintas, y la separación es la que importa:
//!
//! - [`poly`] es **genérico en el anillo**. No conoce `Integer`, ni `Rational`,
//!   ni la arena. Es el código que E4 volverá a usar sobre 𝔽ₚ para factorizar
//!   módulo un primo y levantar por Hensel, y por eso no puede estar escrito
//!   contra un tipo concreto (01-rust-principles.md §4).
//! - [`bridge`] es la frontera con el IR, y es concreta a propósito: la arena
//!   guarda `Int` y `Rat`, no un `C` cualquiera.
//!
//! **Este crate es forma canónica, no búsqueda** (02-architecture.md §«La
//! división que importa»). Expandir, agrupar y dividir polinomios son
//! operaciones deterministas y confluentes: el resultado no depende del orden en
//! que se apliquen. Lo que no tiene forma canónica computable es asunto de
//! `ms-simplify` y sus e-grafos, y mezclarlos es el error clásico.

pub mod bridge;
pub mod factor;
pub mod poly;

pub use bridge::{
    Coefficients, Univariate, as_polynomial, as_univariate, factored_to_exprs, to_expr,
};
pub use factor::{Factored, MAX_ROOT_CANDIDATES, factor, integer_gcd, poly_gcd, rational_roots};
pub use poly::Poly;
