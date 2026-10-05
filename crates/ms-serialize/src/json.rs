//! Expr → JSON estructural.
//!
//! Es el **formato primario del wire** (02-architecture.md §1): el árbol, no la
//! cadena LaTeX. LaTeX viaja en paralelo para consumo directo, pero un
//! consumidor que quiera razonar sobre la expresión —una escena de vídeo, una
//! tabla, un diagrama— lee esto.
//!
//! Los enteros van como **cadenas**, no como números JSON: son bignum y
//! `JSON.parse` los destrozaría en silencio al pasar de 2^53.

use ms_core::{Arena, ExprId, Head, Node};
use serde_json::{Value, json};

use crate::error::SerializeError;

pub fn to_json(arena: &Arena, id: ExprId) -> Result<Value, SerializeError> {
    let node = arena.get(id).ok_or(SerializeError::UnknownExpr(id))?;

    match node {
        Node::Int(value) => Ok(json!({ "int": value.to_string() })),
        Node::Rat(value) => Ok(json!({
            "rat": [value.numer().to_string(), value.denom().to_string()]
        })),
        Node::Sym(sym) => {
            let name = arena
                .symbol_name(*sym)
                .ok_or(SerializeError::UnknownSymbol(*sym))?;
            Ok(json!({ "sym": name }))
        }
        Node::App { head, args } => {
            let args = arena
                .args_of(*args)
                .ok_or(SerializeError::UnknownExpr(id))?
                .to_vec();
            let mut encoded = Vec::with_capacity(args.len());
            for arg in args {
                encoded.push(to_json(arena, arg)?);
            }
            // El nombre de la función viaja en su **propio** campo y no dentro
            // de `h`: así `h` sigue siendo un conjunto cerrado sobre el que
            // TypeScript puede discriminar, en vez de una cadena libre.
            let mut object = serde_json::Map::new();
            object.insert("h".to_owned(), json!(head_name(*head)));
            if let Some(name) = function_name(*head) {
                object.insert("fn".to_owned(), json!(name));
            }
            object.insert("a".to_owned(), json!(encoded));
            Ok(Value::Object(object))
        }
    }
}

fn head_name(head: Head) -> &'static str {
    match head {
        Head::Plus => "Plus",
        Head::Times => "Times",
        Head::Power => "Power",
        Head::Fn(_) => "Fn",
        Head::Equal => "Equal",
        Head::Solutions => "Solutions",
    }
}

fn function_name(head: Head) -> Option<&'static str> {
    if let Head::Fn(builtin) = head {
        return Some(builtin.name());
    }
    None
}
