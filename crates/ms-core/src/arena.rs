//! La arena: `Vec<Node>` + `ExprId(u32)` + hash-consing.
//!
//! Es exactamente donde SymPy pierde: su perfil está dominado por asignación de
//! objetos y recolección de basura. Aquí no hay recolector porque no hay GC (§2).
//!
//! **Append-only durante la evaluación.** Reescribir es insertar nodos nuevos,
//! nunca mutar los existentes. Eso hace `&Arena` trivialmente `Sync` y habilita
//! `rayon` sin locks cuando llegue el paralelismo (§7).

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::One;
use rustc_hash::FxHashMap;

use crate::node::{ArgSlice, CoreError, ExprId, Head, Node, SymId};

#[derive(Default, Debug)]
pub struct Arena {
    nodes: Vec<Node>,
    /// El índice del hash-consing: estructura igual ⇒ mismo `ExprId`.
    interned: FxHashMap<Node, ExprId>,
    args: Vec<ExprId>,
    /// Las listas de argumentos se internan aparte para que dos aplicaciones
    /// iguales compartan el mismo `ArgSlice`. Sin esto, `Node` no puede servir
    /// de clave del hash-consing: dos `Plus(a, b)` construidos por separado
    /// tendrían rangos distintos y se verían como nodos distintos.
    arg_slices: FxHashMap<Box<[ExprId]>, ArgSlice>,
    symbols: Vec<String>,
    symbol_ids: FxHashMap<String, SymId>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- Lectura -----------------------------------------------------------

    /// Devuelve `Option` en vez de indexar porque un id de otra arena es un
    /// fallo posible, y los fallos son datos, no panics (§6).
    pub fn get(&self, id: ExprId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    pub fn args_of(&self, slice: ArgSlice) -> Option<&[ExprId]> {
        let start = slice.start as usize;
        let end = start.checked_add(slice.len as usize)?;
        self.args.get(start..end)
    }

    /// Los argumentos de `id`, si es una aplicación.
    pub fn app_args(&self, id: ExprId) -> Option<&[ExprId]> {
        match self.get(id)? {
            Node::App { args, .. } => self.args_of(*args),
            Node::Int(_) | Node::Rat(_) | Node::Sym(_) => None,
        }
    }

    pub fn head_of(&self, id: ExprId) -> Option<Head> {
        match self.get(id)? {
            Node::App { head, .. } => Some(*head),
            Node::Int(_) | Node::Rat(_) | Node::Sym(_) => None,
        }
    }

    pub fn symbol_name(&self, sym: SymId) -> Option<&str> {
        self.symbols.get(sym.0 as usize).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    // ---- Construcción ------------------------------------------------------

    /// Inserta el nodo, o devuelve el id existente si ya se había visto.
    pub fn intern(&mut self, node: Node) -> Result<ExprId, CoreError> {
        if let Some(&id) = self.interned.get(&node) {
            return Ok(id);
        }
        let index = u32::try_from(self.nodes.len()).map_err(|_| CoreError::ArenaFull)?;
        let id = ExprId(index);
        self.nodes.push(node.clone());
        self.interned.insert(node, id);
        Ok(id)
    }

    pub fn int(&mut self, value: impl Into<BigInt>) -> Result<ExprId, CoreError> {
        self.intern(Node::Int(value.into()))
    }

    /// Un racional **siempre reducido**. Con denominador 1 se degrada a `Int`,
    /// para que no existan dos representaciones del mismo número exacto.
    pub fn rat(&mut self, value: BigRational) -> Result<ExprId, CoreError> {
        let reduced = value.reduced();
        if reduced.denom().is_one() {
            return self.intern(Node::Int(reduced.numer().clone()));
        }
        self.intern(Node::Rat(reduced))
    }

    pub fn sym(&mut self, name: &str) -> Result<ExprId, CoreError> {
        let sym = match self.symbol_ids.get(name) {
            Some(&existing) => existing,
            None => {
                let index = u32::try_from(self.symbols.len()).map_err(|_| CoreError::ArenaFull)?;
                let fresh = SymId(index);
                self.symbols.push(name.to_owned());
                self.symbol_ids.insert(name.to_owned(), fresh);
                fresh
            }
        };
        self.intern(Node::Sym(sym))
    }

    /// El símbolo que ya tiene este id, como expresión.
    ///
    /// Existe porque quien manipula un polinomio sólo tiene el `SymId` de su
    /// variable —no su nombre— y volver a escribirla exigiría buscar el texto
    /// para que `sym` lo volviera a resolver al mismo id.
    ///
    /// **Comprueba que el símbolo sea de esta arena.** Un `SymId` es un `u32` sin
    /// procedencia: internar uno de otra arena metería aquí un nodo que nombra un
    /// símbolo que no existe, y el fallo aparecería mucho después, al escribirlo,
    /// en forma de expresión sin nombre.
    pub fn sym_id(&mut self, sym: SymId) -> Result<ExprId, CoreError> {
        if self.symbol_name(sym).is_none() {
            return Err(CoreError::UnknownSymbol(sym));
        }
        self.intern(Node::Sym(sym))
    }

    /// Construye una aplicación **tal cual**, sin canonicalizar. Aplanar y
    /// ordenar es trabajo de `canonicalize`, que es un paso explícito y con
    /// tipo propio (`Canonical`) para que no se pueda olvidar.
    pub fn app(&mut self, head: Head, args: &[ExprId]) -> Result<ExprId, CoreError> {
        let slice = self.intern_args(args)?;
        self.intern(Node::App { head, args: slice })
    }

    pub fn plus(&mut self, args: &[ExprId]) -> Result<ExprId, CoreError> {
        self.app(Head::Plus, args)
    }

    pub fn times(&mut self, args: &[ExprId]) -> Result<ExprId, CoreError> {
        self.app(Head::Times, args)
    }

    pub fn power(&mut self, base: ExprId, exponent: ExprId) -> Result<ExprId, CoreError> {
        self.app(Head::Power, &[base, exponent])
    }

    fn intern_args(&mut self, args: &[ExprId]) -> Result<ArgSlice, CoreError> {
        if let Some(&slice) = self.arg_slices.get(args) {
            return Ok(slice);
        }
        let start = u32::try_from(self.args.len()).map_err(|_| CoreError::ArenaFull)?;
        let len = u32::try_from(args.len()).map_err(|_| CoreError::ArenaFull)?;
        self.args.extend_from_slice(args);
        let slice = ArgSlice { start, len };
        self.arg_slices.insert(args.into(), slice);
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structurally_equal_expressions_share_one_id() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();

        let left = arena.plus(&[x, two]).unwrap();
        let right = arena.plus(&[x, two]).unwrap();

        // Comparar expresiones es comparar dos enteros, no recorrer dos árboles.
        assert_eq!(left, right);
    }

    #[test]
    fn distinct_expressions_get_distinct_ids() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();

        let sum = arena.plus(&[x, two]).unwrap();
        let product = arena.times(&[x, two]).unwrap();

        assert_ne!(sum, product);
    }

    #[test]
    fn argument_order_is_preserved_until_canonicalization() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let two = arena.int(2).unwrap();

        // `app` no ordena: `Orderless` se aplica en canonicalize, no aquí.
        assert_ne!(arena.plus(&[x, two]).unwrap(), arena.plus(&[two, x]).unwrap());
    }

    #[test]
    fn rational_with_denominator_one_degrades_to_integer() {
        let mut arena = Arena::new();
        let from_rational = arena
            .rat(BigRational::new(BigInt::from(6), BigInt::from(3)))
            .unwrap();
        let from_integer = arena.int(2).unwrap();

        assert_eq!(from_rational, from_integer);
    }

    #[test]
    fn rationals_are_stored_reduced() {
        let mut arena = Arena::new();
        let four_sixths = arena
            .rat(BigRational::new(BigInt::from(4), BigInt::from(6)))
            .unwrap();
        let two_thirds = arena
            .rat(BigRational::new(BigInt::from(2), BigInt::from(3)))
            .unwrap();

        assert_eq!(four_sixths, two_thirds);
    }

    #[test]
    fn the_same_symbol_name_interns_once() {
        let mut arena = Arena::new();
        assert_eq!(arena.sym("x").unwrap(), arena.sym("x").unwrap());
        assert_ne!(arena.sym("x").unwrap(), arena.sym("y").unwrap());
    }

    #[test]
    fn unknown_ids_are_reported_not_panicked() {
        let arena = Arena::new();
        assert!(arena.get(ExprId(99)).is_none());
    }
}
