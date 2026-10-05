//! Canonicalización y el typestate que la certifica.
//!
//! `Canonical` es un newtype de coste cero (§3): no se puede pasar una expresión
//! sin canonicalizar donde se exige canónica, y no por convención de documento
//! sino porque **no compila**.
//!
//! Canonicalizar es sólo estructura: aplanar, ordenar y quitar envoltorios de un
//! elemento. **No pliega aritmética.** Plegar `2 + 3` a `5` es trabajo de una
//! regla, y toda regla emite traza (invariante 2). Si la canonicalización
//! calculara por su cuenta, los pasos pedagógicos tendrían agujeros.

use crate::arena::Arena;
use crate::node::{CoreError, ExprId, Node};

/// Una expresión que pasó por `Arena::canonicalize`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Canonical(ExprId);

impl Canonical {
    pub fn id(self) -> ExprId {
        self.0
    }
}

impl Arena {
    pub fn canonicalize(&mut self, id: ExprId) -> Result<Canonical, CoreError> {
        let canonical = self.canonicalize_inner(id)?;
        Ok(Canonical(canonical))
    }

    fn canonicalize_inner(&mut self, id: ExprId) -> Result<ExprId, CoreError> {
        let node = self.get(id).ok_or(CoreError::UnknownExpr(id))?;
        let (head, args) = match node {
            Node::Int(_) | Node::Rat(_) | Node::Sym(_) => return Ok(id),
            Node::App { head, args } => (*head, *args),
        };

        // Copiado antes de recurrir: la recursión necesita `&mut self` y el
        // slice de argumentos toma prestada la arena.
        let original = self
            .args_of(args)
            .ok_or(CoreError::UnknownExpr(id))?
            .to_vec();

        let mut canonical_args = Vec::with_capacity(original.len());
        for arg in original {
            let arg = self.canonicalize_inner(arg)?;
            // Flat: `a + (b + c)` y `a + b + c` son la misma expresión.
            if head.is_flat() && self.head_of(arg) == Some(head) {
                let nested = self
                    .app_args(arg)
                    .ok_or(CoreError::UnknownExpr(arg))?
                    .to_vec();
                canonical_args.extend(nested);
                continue;
            }
            canonical_args.push(arg);
        }

        if head.is_orderless() {
            self.sort_canonical(&mut canonical_args)?;
        }

        // OneIdentity: `Plus(x)` es `x`. Evita que la reescritura deje
        // envoltorios de un solo argumento por todo el árbol.
        if head.is_one_identity()
            && let [only] = canonical_args.as_slice()
        {
            return Ok(*only);
        }

        self.app(head, &canonical_args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orderless_heads_reach_the_same_id_from_either_argument_order() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();

        let left = arena.plus(&[x, two]).unwrap();
        let right = arena.plus(&[two, x]).unwrap();

        assert_eq!(
            arena.canonicalize(left).unwrap(),
            arena.canonicalize(right).unwrap()
        );
    }

    #[test]
    fn power_is_not_orderless() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let three = arena.int(3).unwrap();

        let two_cubed = arena.power(two, three).unwrap();
        let three_squared = arena.power(three, two).unwrap();

        assert_ne!(
            arena.canonicalize(two_cubed).unwrap(),
            arena.canonicalize(three_squared).unwrap()
        );
    }

    #[test]
    fn flat_heads_splice_nested_arguments() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let y = arena.sym("y").unwrap();
        let z = arena.sym("z").unwrap();

        let nested = arena.plus(&[y, z]).unwrap();
        let outer = arena.plus(&[x, nested]).unwrap();
        let flat = arena.plus(&[x, y, z]).unwrap();

        assert_eq!(
            arena.canonicalize(outer).unwrap(),
            arena.canonicalize(flat).unwrap()
        );
    }

    #[test]
    fn one_identity_unwraps_single_argument_applications() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let wrapped = arena.plus(&[x]).unwrap();

        assert_eq!(arena.canonicalize(wrapped).unwrap().id(), x);
    }

    #[test]
    fn canonicalization_is_idempotent() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();
        let nested = arena.plus(&[two, x]).unwrap();
        let expression = arena.times(&[nested, x]).unwrap();

        let once = arena.canonicalize(expression).unwrap();
        let twice = arena.canonicalize(once.id()).unwrap();

        assert_eq!(once, twice);
    }

    #[test]
    fn canonicalization_does_not_fold_arithmetic() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let three = arena.int(3).unwrap();
        let sum = arena.plus(&[two, three]).unwrap();

        let canonical = arena.canonicalize(sum).unwrap();
        let five = arena.int(5).unwrap();

        // Plegar 2 + 3 a 5 es trabajo de una regla, y toda regla emite traza.
        assert_ne!(canonical.id(), five);
    }
}
