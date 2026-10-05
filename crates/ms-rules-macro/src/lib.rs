//! El macro `rules!`.
//!
//! Su trabajo no es ahorrar teclas: es hacer **imposible** una regla sin id
//! estable, sin nombre legible, sin plantilla de paso y sin campo de soundness
//! (01-rust-principles.md §5). Escribir una regla a mano se salta la traza y la
//! verificación de un plumazo, así que aquí no se puede escribir a mano.
//!
//! Las reglas se expanden en **tiempo de compilación** a una tabla estática:
//! cero parseo en arranque y cero tabla dinámica, que es lo que mantiene el
//! WASM pequeño (§8).
//!
//! Alcance de E0: el macro exige y genera los metadatos, y el cuerpo de cada
//! regla es Rust. El DSL de patrones (`integrate(u_ * dv_, x_) => …`) llega en
//! E2, cuando el número de reglas justifique la red de discriminación; las
//! reglas aritméticas de E0 calculan mcm y mcd, y eso ningún patrón sintáctico
//! lo expresa.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Expr, ItemFn, Lit, LitBool, LitStr, Meta};

struct Rules {
    rules: Vec<Rule>,
}

struct Rule {
    function: ItemFn,
    id: LitStr,
    title: LitStr,
    step: LitStr,
    soundness: Option<Soundness>,
    silent: bool,
}

/// Con qué respalda una regla su corrección.
///
/// Tres formas, las que enuncia `03-lean-verification.md` §2, y **un enum en vez
/// de una cadena**: el campo antiguo era un literal libre, así que «demostrado
/// en Mathlib», «demostrado aquí» y «sin demostrar, a sabiendas» eran la misma
/// cosa para el compilador. Con tres variantes, la métrica del proyecto —qué
/// porcentaje de reglas está verificado por máquina— se puede calcular sin
/// interpretar texto.
enum Soundness {
    /// `lean("Mathlib.…")`: un teorema que ya existe en Mathlib.
    Mathlib(LitStr),
    /// `lean_local("nombre")`: un teorema demostrado en `engine/lean/`.
    ///
    /// Que exista lo comprueba `scripts/check-soundness.sh`, y no es ceremonia:
    /// un nombre con una errata afirmaría una demostración que no hay, que es
    /// exactamente el fallo que este campo existe para impedir.
    Local(LitStr),
    /// `numeric_only`: sin demostración, y **queda dicho**. Es una respuesta
    /// válida para cualquier regla —Lean nunca bloquea una etapa— y es distinta
    /// de no anotar nada: una dice «lo miramos y decidimos que no», la otra dice
    /// que nadie lo miró.
    NumericOnly,
}

impl Parse for Rules {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut rules = Vec::new();
        while !input.is_empty() {
            rules.push(Rule::from_function(input.parse::<ItemFn>()?)?);
        }
        Ok(Rules { rules })
    }
}

impl Rule {
    fn from_function(mut function: ItemFn) -> syn::Result<Self> {
        let mut id = None;
        let mut title = None;
        let mut step = None;
        let mut soundness = None;
        let mut silent = false;
        let mut kept = Vec::new();

        for attribute in core::mem::take(&mut function.attrs) {
            match attribute_name(&attribute).as_deref() {
                Some("id") => id = Some(string_value(&attribute)?),
                Some("title") => title = Some(string_value(&attribute)?),
                Some("step") => step = Some(string_value(&attribute)?),
                Some("soundness") => soundness = Some(soundness_value(&attribute)?),
                // Invariante 2: toda reescritura emite traza. El silencio es
                // explícito y por eso hay que escribirlo.
                Some("silent") => silent = true,
                Some(_) | None => kept.push(attribute),
            }
        }
        function.attrs = kept;

        let name = function.sig.ident.clone();
        let require = |value: Option<LitStr>, what: &str| {
            value.ok_or_else(|| {
                syn::Error::new_spanned(
                    &name,
                    format!("la regla «{name}» no declara #[{what} = \"…\"]"),
                )
            })
        };

        Ok(Rule {
            id: require(id, "id")?,
            title: require(title, "title")?,
            step: require(step, "step")?,
            soundness,
            silent,
            function,
        })
    }
}

fn attribute_name(attribute: &Attribute) -> Option<String> {
    attribute.path().get_ident().map(ToString::to_string)
}

fn string_value(attribute: &Attribute) -> syn::Result<LitStr> {
    let Meta::NameValue(pair) = &attribute.meta else {
        return Err(syn::Error::new_spanned(
            attribute,
            "se esperaba `#[clave = \"valor\"]`",
        ));
    };
    let Expr::Lit(literal) = &pair.value else {
        return Err(syn::Error::new_spanned(
            &pair.value,
            "se esperaba una cadena literal",
        ));
    };
    let Lit::Str(text) = &literal.lit else {
        return Err(syn::Error::new_spanned(
            &literal.lit,
            "se esperaba una cadena literal",
        ));
    };
    Ok(text.clone())
}

/// Lee `#[soundness = …]` en sus tres formas.
fn soundness_value(attribute: &Attribute) -> syn::Result<Soundness> {
    let Meta::NameValue(pair) = &attribute.meta else {
        return Err(syn::Error::new_spanned(attribute, EXPECTED));
    };

    // Con `if let` y no con un `match` sobre `syn::Expr`: es un enum ajeno de
    // casi cuarenta variantes, y el lint que prohíbe los comodines existe para
    // que crecer **nuestro** IR rompa el build, no para enumerar la gramática de
    // Rust entera. Un lint que obliga a escribir algo absurdo se acaba apagando,
    // y entonces deja de proteger lo que sí protegía.
    if let Expr::Path(path) = &pair.value
        && path.path.is_ident("numeric_only")
    {
        return Ok(Soundness::NumericOnly);
    }

    let Expr::Call(call) = &pair.value else {
        return Err(syn::Error::new_spanned(&pair.value, EXPECTED));
    };
    let Expr::Path(function) = call.func.as_ref() else {
        return Err(syn::Error::new_spanned(&call.func, EXPECTED));
    };
    let [argument] = call.args.iter().collect::<Vec<_>>()[..] else {
        return Err(syn::Error::new_spanned(
            call,
            "se esperaba exactamente un nombre de teorema",
        ));
    };
    let theorem = literal_string(argument)?;

    if function.path.is_ident("lean") {
        return Ok(Soundness::Mathlib(theorem));
    }
    if function.path.is_ident("lean_local") {
        return Ok(Soundness::Local(theorem));
    }
    Err(syn::Error::new_spanned(
        function,
        "sólo `lean(…)` y `lean_local(…)` respaldan una regla",
    ))
}

/// Lo único que `#[soundness = …]` acepta, dicho una vez.
const EXPECTED: &str = "se esperaba `lean(\"…\")`, `lean_local(\"…\")` o `numeric_only`";

fn literal_string(expression: &Expr) -> syn::Result<LitStr> {
    let Expr::Lit(literal) = expression else {
        return Err(syn::Error::new_spanned(
            expression,
            "se esperaba una cadena literal",
        ));
    };
    let Lit::Str(text) = &literal.lit else {
        return Err(syn::Error::new_spanned(
            &literal.lit,
            "se esperaba una cadena literal",
        ));
    };
    Ok(text.clone())
}

/// Declara un conjunto de reglas y su tabla estática `RULES`.
#[proc_macro]
pub fn rules(input: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<Rules>(input) {
        Ok(parsed) => parsed,
        Err(error) => return error.to_compile_error().into(),
    };

    let functions = parsed.rules.iter().map(|rule| &rule.function);

    let entries = parsed.rules.iter().map(|rule| {
        let name = &rule.function.sig.ident;
        let (id, title, step) = (&rule.id, &rule.title, &rule.step);
        let soundness = match &rule.soundness {
            Some(Soundness::Mathlib(theorem)) => {
                quote!(Some(ms_rules::Soundness::Mathlib(#theorem)))
            }
            Some(Soundness::Local(theorem)) => {
                quote!(Some(ms_rules::Soundness::Local(#theorem)))
            }
            Some(Soundness::NumericOnly) => quote!(Some(ms_rules::Soundness::NumericOnly)),
            None => quote!(None),
        };
        let silent = LitBool::new(rule.silent, name.span());
        quote! {
            ms_rules::Rule {
                id: #id,
                title: #title,
                step_template: #step,
                soundness: #soundness,
                silent: #silent,
                apply: #name,
            }
        }
    });

    quote! {
        #(#functions)*

        /// Las reglas de este módulo, en orden de intento.
        pub static RULES: &[ms_rules::Rule] = &[ #(#entries),* ];
    }
    .into()
}
