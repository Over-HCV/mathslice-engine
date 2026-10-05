//! `ms-assume` — lo que se supone de los símbolos, y las preguntas que las
//! reglas tienen derecho a hacer antes de dispararse.
//!
//! Existe por una razón concreta: casi toda regla de un CAS tiene una condición
//! lateral, y la forma habitual de que un CAS dé una respuesta incorrecta es
//! aplicar una regla cuya condición nadie comprobó. `\frac{a}{b} \div
//! \frac{c}{d}` sólo es `\frac{ad}{bc}` **si `c \neq 0`**; `\sqrt{x^2}` sólo es
//! `x` si `x \geq 0`. Que el contexto sea un argumento obligatorio de
//! `Rule::apply` convierte esa comprobación en algo que el compilador exige, no
//! en algo que hay que acordarse de escribir (01-rust-principles.md §5).
//!
//! **Sólo afirma lo que sabe.** Todas las consultas distinguen «sé que no» de
//! «no sé», y una regla que necesita certeza no puede dispararse con un «no sé».
//! Es la política restrictiva de Safety §3: abrir el permiso después es fácil.
//!
//! Lo que todavía no está y llega con su tarea, para no dejar API sin usar: la
//! base numérica (E1 4.4), los intervalos y los dominios de definición (E2).

use ms_core::{Arena, ExprId, Head, Node, SymId};
use num_traits::Zero;
use rustc_hash::FxHashMap;

/// El conjunto donde vive un símbolo. Crece hacia arriba: los complejos y la
/// torre algebraica son E5, y añadirlos aquí antes sería prometer lo que el
/// motor no sabe comprobar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Domain {
    Integer,
    Rational,
    Real,
}

/// El signo de un símbolo, cuando se conoce. `Zero` es un signo y no la ausencia
/// de uno: saber que algo es cero es tan útil como saber que es positivo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sign {
    Negative,
    Zero,
    Positive,
}

/// Lo que se supone de un símbolo concreto. `None` es «no se sabe», y no se
/// rellena con un valor por defecto: un valor por defecto inventado es
/// exactamente cómo un CAS acaba afirmando cosas falsas.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SymbolAssumption {
    pub domain: Option<Domain>,
    pub sign: Option<Sign>,
}

/// El contexto con el que corre una reescritura.
///
/// Vacío significa «no se supone nada», que es el estado correcto para una
/// consulta escrita en una caja de texto: el estudiante no ha declarado nada
/// sobre sus símbolos y el motor no puede inventárselo.
#[derive(Clone, Debug, Default)]
pub struct Assumptions {
    symbols: FxHashMap<SymId, SymbolAssumption>,
}

impl Assumptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declara lo que se sabe de un símbolo. Sustituye lo anterior: dos
    /// suposiciones sobre el mismo símbolo no se mezclan solas, porque mezclarlas
    /// mal produce un contexto contradictorio del que se deduce cualquier cosa.
    pub fn assume(&mut self, symbol: SymId, assumption: SymbolAssumption) {
        self.symbols.insert(symbol, assumption);
    }

    pub fn of(&self, symbol: SymId) -> SymbolAssumption {
        self.symbols.get(&symbol).copied().unwrap_or_default()
    }

    /// ¿Se **sabe** que esta expresión no es cero?
    ///
    /// `false` significa «no consta», no «es cero». Es la pregunta que hace la
    /// regla de la división antes de invertir un denominador, y por eso responde
    /// que no ante la duda: dividir entre algo que podría ser cero es la clase de
    /// respuesta incorrecta que este motor no debe dar (§9).
    ///
    /// En E1 la certeza viene de dos sitios: un literal distinto de cero, o un
    /// símbolo declarado con signo `Negative` o `Positive`. Los productos y
    /// potencias se deciden por sus partes; las sumas no, porque `x + y` puede
    /// anularse y sumar dos cosas no nulas no da nada.
    pub fn is_known_nonzero(&self, arena: &Arena, id: ExprId) -> bool {
        let Some(node) = arena.get(id) else {
            return false;
        };

        match node {
            Node::Int(value) => !value.is_zero(),
            Node::Rat(value) => !value.is_zero(),
            Node::Sym(symbol) => matches!(
                self.of(*symbol).sign,
                Some(Sign::Negative | Sign::Positive)
            ),
            Node::App { head, args } => {
                let Some(args) = arena.args_of(*args) else {
                    return false;
                };
                match head {
                    // Un producto es no nulo exactamente cuando lo son todos sus
                    // factores, y una potencia cuando lo es su base: `0^{n}` es
                    // el único cero que puede salir de elevar.
                    Head::Times => args.iter().all(|arg| self.is_known_nonzero(arena, *arg)),
                    Head::Power => args
                        .first()
                        .is_some_and(|base| self.is_known_nonzero(arena, *base)),
                    // `x + y` con ambos no nulos puede ser cero. No saberlo es la
                    // respuesta correcta.
                    Head::Plus => false,
                    // Y de una función, menos: `redondear(x, 0)` es cero para
                    // todo `x` menor que un medio, con `x` no nulo. Política
                    // restrictiva (Safety §3): abrir el permiso después es fácil.
                    //
                    // Una ecuación y un conjunto de soluciones no son números, así
                    // que preguntar si son cero no significa nada. Contestar que
                    // no se sabe es lo único honesto: contestar que sí dejaría a
                    // una regla dividir entre una ecuación.
                    Head::Fn(_) | Head::Equal | Head::Solutions => false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_context_knows_nothing_about_a_symbol() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();

        assert!(!Assumptions::new().is_known_nonzero(&arena, x));
    }

    #[test]
    fn a_symbol_declared_positive_is_known_to_be_nonzero() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let Some(Node::Sym(symbol)) = arena.get(x) else {
            panic!("x es un símbolo");
        };
        let symbol = *symbol;

        let mut assumptions = Assumptions::new();
        assumptions.assume(
            symbol,
            SymbolAssumption {
                domain: Some(Domain::Real),
                sign: Some(Sign::Positive),
            },
        );

        assert!(assumptions.is_known_nonzero(&arena, x));
    }

    #[test]
    fn a_symbol_declared_zero_is_not_reported_as_nonzero() {
        let mut arena = Arena::new();
        let x = arena.sym("x").unwrap();
        let Some(Node::Sym(symbol)) = arena.get(x) else {
            panic!("x es un símbolo");
        };
        let symbol = *symbol;

        let mut assumptions = Assumptions::new();
        assumptions.assume(
            symbol,
            SymbolAssumption {
                domain: None,
                sign: Some(Sign::Zero),
            },
        );

        assert!(!assumptions.is_known_nonzero(&arena, x));
    }

    #[test]
    fn literals_answer_without_any_declaration() {
        let mut arena = Arena::new();
        let zero = arena.int(0).unwrap();
        let three = arena.int(3).unwrap();
        let assumptions = Assumptions::new();

        assert!(!assumptions.is_known_nonzero(&arena, zero));
        assert!(assumptions.is_known_nonzero(&arena, three));
    }

    #[test]
    fn a_product_is_nonzero_only_when_every_factor_is() {
        let mut arena = Arena::new();
        let two = arena.int(2).unwrap();
        let x = arena.sym("x").unwrap();
        let known = arena.times(&[two, two]).unwrap();
        let unknown = arena.times(&[two, x]).unwrap();
        let assumptions = Assumptions::new();

        assert!(assumptions.is_known_nonzero(&arena, known));
        assert!(!assumptions.is_known_nonzero(&arena, unknown));
    }

    #[test]
    fn a_sum_of_two_nonzero_terms_is_not_claimed_to_be_nonzero() {
        // 1 + (-1) es cero, así que de dos sumandos no nulos no se deduce nada.
        let mut arena = Arena::new();
        let one = arena.int(1).unwrap();
        let minus_one = arena.int(-1).unwrap();
        let sum = arena.plus(&[one, minus_one]).unwrap();

        assert!(!Assumptions::new().is_known_nonzero(&arena, sum));
    }

    #[test]
    fn a_reciprocal_of_a_nonzero_base_is_nonzero() {
        let mut arena = Arena::new();
        let three = arena.int(3).unwrap();
        let minus_one = arena.int(-1).unwrap();
        let reciprocal = arena.power(three, minus_one).unwrap();

        assert!(Assumptions::new().is_known_nonzero(&arena, reciprocal));
    }
}
