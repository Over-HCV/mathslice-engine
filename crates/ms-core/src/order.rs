//! Orden canónico total sobre expresiones.
//!
//! No se ordena por `ExprId`: el id depende del orden de construcción, así que
//! `x + 2` y `2 + x` darían resultados distintos según cuál se escribiera antes.
//! El orden tiene que ser **estructural** para que dos expresiones iguales
//! canonicalicen al mismo id, que es lo que sostiene el hash-consing.

use core::cmp::Ordering;

use crate::arena::Arena;
use crate::node::{CoreError, ExprId, Node};

impl Arena {
    /// Orden total y estable entre dos expresiones de esta arena.
    pub fn compare(&self, left: ExprId, right: ExprId) -> Result<Ordering, CoreError> {
        if left == right {
            return Ok(Ordering::Equal);
        }

        let left_node = self.get(left).ok_or(CoreError::UnknownExpr(left))?;
        let right_node = self.get(right).ok_or(CoreError::UnknownExpr(right))?;

        let by_rank = left_node.rank().cmp(&right_node.rank());
        if by_rank != Ordering::Equal {
            return Ok(by_rank);
        }

        match (left_node, right_node) {
            (Node::Int(a), Node::Int(b)) => Ok(a.cmp(b)),
            (Node::Rat(a), Node::Rat(b)) => Ok(a.cmp(b)),
            (Node::Sym(a), Node::Sym(b)) => {
                let a_name = self.symbol_name(*a).ok_or(CoreError::UnknownSymbol(*a))?;
                let b_name = self.symbol_name(*b).ok_or(CoreError::UnknownSymbol(*b))?;
                Ok(a_name.cmp(b_name))
            }
            (
                Node::App { head: a_head, args: a_args },
                Node::App { head: b_head, args: b_args },
            ) => {
                let by_head = a_head.cmp(b_head);
                if by_head != Ordering::Equal {
                    return Ok(by_head);
                }
                let a_list = self.args_of(*a_args).ok_or(CoreError::UnknownExpr(left))?;
                let b_list = self.args_of(*b_args).ok_or(CoreError::UnknownExpr(right))?;
                self.compare_lists(a_list, b_list)
            }
            // Rangos iguales con variantes distintas es imposible hoy: `rank` es
            // inyectivo sobre las variantes. Enumerado en vez de `_` (§1), y
            // fallando en vez de devolviendo `Equal`: si una variante nueva
            // colisiona de rango, dos expresiones distintas compararían iguales
            // y el hash-consing las fundiría sin que nada lo delatara.
            (Node::Int(_) | Node::Rat(_) | Node::Sym(_) | Node::App { .. }, _) => {
                Err(CoreError::IncomparableNodes(left, right))
            }
        }
    }

    fn compare_lists(&self, left: &[ExprId], right: &[ExprId]) -> Result<Ordering, CoreError> {
        for (a, b) in left.iter().zip(right.iter()) {
            let ordering = self.compare(*a, *b)?;
            if ordering != Ordering::Equal {
                return Ok(ordering);
            }
        }
        Ok(left.len().cmp(&right.len()))
    }

    /// Ordena en sitio con el orden canónico.
    ///
    /// Es inserción, no `sort_by`: el comparador puede fallar (un id de otra
    /// arena) y `sort_by` no deja propagar ese `Result`. Las listas de
    /// argumentos son de dos o tres elementos; el bench de `ms-rules` vigila
    /// que eso siga siendo cierto.
    pub(crate) fn sort_canonical(&self, items: &mut [ExprId]) -> Result<(), CoreError> {
        for i in 1..items.len() {
            let mut j = i;
            while j > 0 {
                let Some(previous) = items.get(j - 1).copied() else {
                    break;
                };
                let Some(current) = items.get(j).copied() else {
                    break;
                };
                if self.compare(previous, current)? != Ordering::Greater {
                    break;
                }
                items.swap(j - 1, j);
                j -= 1;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_sort_before_symbols_before_applications() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let x = arena.sym("x").unwrap();
        let product = arena.times(&[two, x]).unwrap();

        assert_eq!(arena.compare(two, x).unwrap(), Ordering::Less);
        assert_eq!(arena.compare(x, product).unwrap(), Ordering::Less);
    }

    #[test]
    fn symbols_order_by_name_not_by_construction_order() {
        let mut arena = Arena::new();
        // "z" se construye primero a propósito: si el orden fuera por ExprId,
        // este test pasaría al revés.
        let z = arena.sym("z").unwrap();
        let a = arena.sym("a").unwrap();

        assert_eq!(arena.compare(a, z).unwrap(), Ordering::Less);
    }

    #[test]
    fn integers_order_by_value() {
        let mut arena = Arena::new();
        let ten = arena.int(10).unwrap();
        let two = arena.int(2).unwrap();

        assert_eq!(arena.compare(two, ten).unwrap(), Ordering::Less);
    }

    #[test]
    fn shorter_argument_lists_sort_first_when_prefixes_match() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let y = arena.sym("y").unwrap();
        let short = arena.plus(&[x]).unwrap();
        let long = arena.plus(&[x, y]).unwrap();

        assert_eq!(arena.compare(short, long).unwrap(), Ordering::Less);
    }

    #[test]
    fn sorting_is_stable_and_total() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();
        let a = arena.sym("a").unwrap();

        let mut items = vec![x, two, a];
        arena.sort_canonical(&mut items).unwrap();

        assert_eq!(items, vec![two, a, x]);
    }
}
