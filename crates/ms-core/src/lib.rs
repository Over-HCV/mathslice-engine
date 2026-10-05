//! `ms-core` — el IR de expresiones.
//!
//! Es el contrato que nunca debe cambiar (02-architecture.md §2). No depende de
//! ningún crate del workspace y no sabe nada de matemáticas concretas: sólo
//! representa expresiones, las comparte por hash-consing y las pone en forma
//! canónica.
//!
//! Los tres tipos que importan:
//!
//! - [`Arena`] — dueña de todos los nodos. Append-only durante la evaluación.
//! - [`ExprId`] — un `u32`. Dos expresiones estructuralmente iguales son el mismo id.
//! - [`Canonical`] — prueba en el sistema de tipos de que algo se canonicalizó.

mod arena;
mod canonical;
mod node;
mod order;

pub use arena::Arena;
pub use canonical::Canonical;
pub use node::{ArgSlice, Builtin, CoreError, ExprId, Head, Node, SymId};
