//! Los fallos de serialización. Ninguno es un panic (§6).

use ms_core::{ExprId, SymId};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SerializeError {
    UnknownExpr(ExprId),
    UnknownSymbol(SymId),
    /// `Power` con aridad distinta de 2. Sólo puede venir de construir el IR a
    /// mano saltándose `Arena::power`.
    MalformedPower {
        arity: usize,
    },
    /// Una llamada a función con una aridad que no es la suya. Escribirla igual
    /// produciría LaTeX que se renderiza y que no significa nada.
    MalformedCall {
        function: &'static str,
        arity: usize,
    },
    /// `Equal` con aridad distinta de 2. Una ecuación tiene dos lados; escribir
    /// «`a = b = c`» o «`= 0`» produciría LaTeX que se renderiza y que el parser
    /// rechazaría al volver a entrar.
    MalformedEquation {
        arity: usize,
    },
}

impl core::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SerializeError::UnknownExpr(id) => write!(f, "expresión desconocida: {id:?}"),
            SerializeError::UnknownSymbol(id) => write!(f, "símbolo desconocido: {id:?}"),
            SerializeError::MalformedPower { arity } => {
                write!(f, "Power espera 2 argumentos, recibió {arity}")
            }
            SerializeError::MalformedCall { function, arity } => {
                write!(f, "{function} no toma {arity} argumentos")
            }
            SerializeError::MalformedEquation { arity } => {
                write!(f, "una ecuación tiene 2 lados, recibió {arity}")
            }
        }
    }
}

impl core::error::Error for SerializeError {}
