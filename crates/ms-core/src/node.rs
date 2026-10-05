//! El IR: un sum type, no una jerarquía de clases con visitor.
//!
//! El compilador prueba exhaustividad, así que añadir un `Head` nuevo en el mes
//! 14 hace que el build enumere cada sitio que falta manejar
//! (01-rust-principles.md §1). Por eso no hay ningún `_ =>` aquí.

use num_bigint::BigInt;
use num_rational::BigRational;

/// Índice en la arena. Dos expresiones estructuralmente iguales son el mismo id,
/// así que comparar expresiones es comparar dos enteros (§2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ExprId(pub(crate) u32);

impl ExprId {
    /// Ensanchamiento sin pérdida: `usize` es de 32 bits en wasm32 y de 64 en el host.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// Índice en la tabla de símbolos de la arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct SymId(pub(crate) u32);

/// Rango dentro del vector de argumentos de la arena. Mantiene `Node` pequeño y
/// `Copy`, que es lo que hace barato el hash-consing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ArgSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// Las cabezas del IR. Crece por etapas; la exhaustividad hace ese crecimiento
/// seguro.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Head {
    Plus,
    Times,
    Power,
    /// Una función con nombre: `\operatorname{redondear}(x, 2)`.
    ///
    /// **`Head` crece una sola vez por esto.** Las alternativas eran una variante
    /// por función —y en E6 son cientos dentro de un enum `Copy`— o
    /// `Head::Fn(SymId)`, que no vuelve a tocar el kernel nunca pero apaga la
    /// exhaustividad: con un símbolo cualquiera dentro, añadir una función no
    /// rompe ningún `match`, y una que el verificador no conozca sale sin
    /// verificar y sin que nadie se entere.
    ///
    /// Con un enum cerrado dentro, añadir un [`Builtin`] rompe el `match` del
    /// evaluador exacto, el del escritor de LaTeX y el de la tabla de aridad,
    /// que son los tres sitios donde olvidarse produce una respuesta incorrecta.
    ///
    /// Cuando E3 necesite `f(x)` con `f` sin definir, eso es otra variante
    /// —`UserFn(SymId)`— y otra decisión: una función que el motor no conoce no
    /// se evalúa, y eso hay que decirlo en su sitio, no colarlo aquí.
    Fn(Builtin),
    /// Una ecuación: `x^{2}-5x+6 = 0`. Binaria, y ni plana ni sin orden.
    ///
    /// **No se pela en la frontera como [`Builtin::InBase`].** `InBase` se pela
    /// porque el reescritor no debe verla nunca: no cambia el valor, sólo cómo se
    /// lee. Una ecuación **sí** es lo que se reescribe —`(x-2)(x-3)=0` es un
    /// estado intermedio de la traza, no un envoltorio— y pelarla dejaría los
    /// pasos sin sujeto.
    ///
    /// No es *orderless* aunque `a = b` y `b = a` sean la misma matemática: son
    /// distinta lectura, y aquí manda la lectura. Ordenarlas canónicamente
    /// pondría el `0` delante en `x^{2}-5x+6 = 0`, que no es como se escribe.
    Equal,
    /// El conjunto de soluciones de una ecuación: `Solutions(x=2, x=3)`.
    ///
    /// Se llama así y no `List` porque el atributo tiene que ser obviamente
    /// correcto: las soluciones de una ecuación son un **conjunto**, no una
    /// lista, y por eso es *orderless* —las raíces salen en orden canónico, que
    /// además es ascendente para los números y por tanto se lee bien—. Un `List`
    /// genérico sin orden sería la sorpresa: el día que E3 necesite algo
    /// genuinamente ordenado (los puntos de una interpolación, las filas de una
    /// matriz) entra como otra cabeza y con el atributo contrario, sin pelearse
    /// con ésta.
    ///
    /// No es *one identity*: un conjunto de una solución sigue siendo un
    /// conjunto, y desenvolverlo haría que una raíz doble se leyera como algo de
    /// otra clase que dos raíces simples.
    Solutions,
}

/// Las funciones con nombre que el motor conoce.
///
/// Cerrado a propósito, y cada variante llega con su regla y su caso: una
/// función declarada antes de que nada la sepa calcular es una promesa que el
/// documento no cumple.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Builtin {
    /// Redondeo con el empate lejos del cero, el que se enseña en clase.
    Round,
    /// Redondeo con el empate a la cifra par, el que no arrastra sesgo.
    RoundHalfEven,
    /// La base en la que se quiere **leer** el resultado: `enBase(2, 11)`.
    ///
    /// No es una operación: no cambia el número, cambia cómo se escribe. Por eso
    /// ninguna regla la toca —una reescritura devolvería el desarrollo posicional
    /// al número y perdería la petición, o ciclaría intentándolo— y por eso la
    /// frontera la quita del árbol antes de reescribir. Lo que sale de ella es un
    /// hecho, como el decimal o el periódico.
    ///
    /// La base va **primero** para que no se pierda al final de una ecuación
    /// larga: se lee como se dice, «en base 2, esto».
    InBase,
    /// `expandir(p)`: quita los paréntesis de un producto de polinomios.
    ///
    /// **Es una consulta con nombre y no una regla automática**, y ésa es la
    /// decisión. Expandir y factorizar son inversas: una regla que expandiera
    /// `\left(x-2\right)\left(x-3\right)` y otra que factorizara `x^{2}-5x+6`
    /// se devolverían el trabajo la una a la otra hasta agotar el presupuesto.
    ///
    /// Con la petición en la cabeza no hay ciclo posible: la regla la consume, y
    /// lo que queda ya no pide nada. Es también lo que dice el árbol de
    /// capacidades, donde «Expand a polynomial» es un ejemplo de consulta.
    Expand,
    /// `factorizar(p)`: escribe un polinomio como producto de sus factores.
    ///
    /// La otra mitad de la decisión de [`Builtin::Expand`], y por el mismo
    /// motivo. Dentro de una ecuación, factorizar **sí** es un paso automático
    /// —es como se resuelve— y eso lo hace `ms-solve`, sobre `Equal`, donde no
    /// hay nada que lo deshaga.
    Factor,
    /// `mcd(a, b)`: el máximo común divisor, consultable.
    ///
    /// Lo que E1 dejó a E2. Ya no es una decisión de kernel —con
    /// [`Head::Fn`] es una variante y una regla—: lo que faltaba era el azúcar.
    Gcd,
    /// `derivar(f, x)`: la derivada de `f` respecto de `x`.
    ///
    /// Se escribe `\frac{d}{dx}\left(f\right)`, que es como se lee en clase; la
    /// llamada con nombre es la forma canónica del IR y también se acepta al
    /// entrar. Dos escrituras, una sola expresión.
    ///
    /// **Es una petición con nombre, como [`Builtin::Expand`], pero por un
    /// motivo distinto que conviene no confundir.** Expandir y factorizar son
    /// inversas *entre ellas*: si fueran reglas automáticas se devolverían el
    /// trabajo hasta agotar el presupuesto. Derivar **no tiene inversa en este
    /// rule set** —integrar es E4— así que hoy no hay con quién ciclar. Lo que
    /// la petición en la cabeza compra aquí es otra cosa: `2x` no es «la
    /// derivada de algo» y `\frac{d}{dx}` es lo único que dice de qué. Sin la
    /// cabeza, una expresión ya derivada y una sin derivar son indistinguibles,
    /// y el reescritor no sabría cuál de las dos tiene delante.
    ///
    /// Cuando E4 traiga integrar, la pregunta del ciclo llega hecha: las dos son
    /// peticiones y ninguna se dispara sola.
    ///
    /// La variable va **segunda** —al revés que en [`Builtin::InBase`]— porque
    /// aquí sí se lee así: «la derivada de f, respecto de x». En `enBase` la base
    /// va delante para que no se pierda al final de una expresión larga; el
    /// operador `\frac{d}{dx}` ya lleva la suya escrita delante, así que el
    /// argumento no tiene ese problema.
    Derivative,
}

impl Builtin {
    /// Todas, en el orden en que se declararon.
    ///
    /// El enum es cerrado, así que esta lista **es** la superficie de funciones del
    /// motor, y hay un único sitio donde crece. Antes vivía dentro de
    /// [`Builtin::from_name`], que es el peor lugar posible para una lista que
    /// alguien más va a querer: quien añada una variante toca el `match` de
    /// [`Builtin::name`], el de [`Builtin::arity`] y el compilador se lo exige,
    /// pero una lista escrita a mano dentro de una función no se lo exige nadie.
    ///
    /// La cuenta del array sí lo exige: añadir una variante sin ponerla aquí no
    /// compila.
    pub const ALL: [Builtin; 7] = [
        Builtin::Round,
        Builtin::RoundHalfEven,
        Builtin::InBase,
        Builtin::Expand,
        Builtin::Factor,
        Builtin::Gcd,
        Builtin::Derivative,
    ];

    /// El nombre con el que se escribe y se lee: `\operatorname{redondear}`.
    ///
    /// En español, como los títulos de las reglas y por la misma razón: es lo
    /// que el estudiante escribe y lee. Los ids de las reglas no se traducen;
    /// esto sí es idioma.
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Round => "redondear",
            Builtin::RoundHalfEven => "redondearPar",
            Builtin::InBase => "enBase",
            Builtin::Expand => "expandir",
            Builtin::Factor => "factorizar",
            Builtin::Gcd => "mcd",
            Builtin::Derivative => "derivar",
        }
    }

    /// La función que se escribe así, si alguna. Es la única puerta de entrada:
    /// un nombre que no esté aquí no es una función con un error dentro, es una
    /// función que este motor no conoce.
    pub fn from_name(name: &str) -> Option<Self> {
        Builtin::ALL
            .into_iter()
            .find(|builtin| builtin.name() == name)
    }

    /// Cuántos argumentos toma. Una llamada con otra aridad **no es** esta
    /// función, y aceptarla «por si acaso» es cómo se cuela un resultado que
    /// nadie sabe qué significa.
    pub fn arity(self) -> usize {
        match self {
            Builtin::Round
            | Builtin::RoundHalfEven
            | Builtin::InBase
            | Builtin::Gcd
            // La variable no es opcional: `derivar(xy)` no dice respecto de qué,
            // y elegirla por él —«la única que hay», «la primera por orden»— es
            // contestar una pregunta que el usuario no hizo.
            | Builtin::Derivative => 2,
            Builtin::Expand | Builtin::Factor => 1,
        }
    }
}

impl Head {
    /// `Flat`: `a + (b + c)` y `a + b + c` son la misma expresión, así que los
    /// argumentos anidados con la misma cabeza se funden en la canonicalización.
    pub fn is_flat(self) -> bool {
        match self {
            Head::Plus | Head::Times => true,
            // Anidar una función dentro de otra no la aplana: `redondear` de un
            // `redondear` son dos redondeos, y son dos operaciones distintas.
            //
            // Una ecuación dentro de otra no existe —`=` sólo vale en la raíz—, y
            // un conjunto de soluciones dentro de otro sería un sistema, que es
            // otra cosa y todavía no se resuelve.
            Head::Power | Head::Fn(_) | Head::Equal | Head::Solutions => false,
        }
    }

    /// `Orderless`: el orden de los argumentos no significa nada, así que se
    /// ordenan canónicamente. `Power` no lo es — `2^3` no es `3^2`.
    pub fn is_orderless(self) -> bool {
        match self {
            // Un conjunto de soluciones no tiene orden, y ordenarlo canónicamente
            // es lo que hace que las raíces salgan siempre en la misma secuencia
            // —ascendente, porque los números se ordenan por valor— en vez de en
            // el orden en que la factorización las fue encontrando.
            Head::Plus | Head::Times | Head::Solutions => true,
            // En una función los argumentos son posicionales: reordenar
            // `redondear(3.456, 2)` daría «2 redondeado a 3.456 cifras». Y en una
            // ecuación, ver el §doc de `Head::Equal`.
            Head::Power | Head::Fn(_) | Head::Equal => false,
        }
    }

    /// `OneIdentity`: con un solo argumento, la aplicación *es* ese argumento.
    /// `Plus(x)` es `x`. Evita que la reescritura deje envoltorios de un elemento.
    pub fn is_one_identity(self) -> bool {
        match self {
            Head::Plus | Head::Times => true,
            // Una función de un argumento sigue siendo esa función aplicada, no
            // su argumento. Desenvolverla borraría la operación, y en
            // `Solutions` haría que una raíz doble se leyera como otra clase de
            // cosa que dos raíces simples.
            Head::Power | Head::Fn(_) | Head::Equal | Head::Solutions => false,
        }
    }

    /// El elemento neutro, que las reglas de plegado usan para eliminar términos.
    pub fn identity_element(self) -> Option<i8> {
        match self {
            Head::Plus => Some(0),
            Head::Times => Some(1),
            // Ninguna función tiene neutro, y no tenerlo es lo que impide que
            // `identity-element` le quite argumentos por parecerse a un producto.
            // Una ecuación y un conjunto de soluciones, tampoco: quitarle a
            // `x = 0` su cero lo dejaría sin la mitad que lo hace una ecuación.
            Head::Power | Head::Fn(_) | Head::Equal | Head::Solutions => None,
        }
    }
}

/// Un nodo del IR.
///
/// `Rat` sólo aparece **reducido y con denominador distinto de 1** — de eso se
/// encarga `Arena::rat`. Una fracción sin reducir, que es lo que necesitan los
/// pasos pedagógicos intermedios, se representa estructuralmente como
/// `Times(n, Power(d, -1))`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Node {
    Int(BigInt),
    Rat(BigRational),
    Sym(SymId),
    App { head: Head, args: ArgSlice },
}

impl Node {
    /// Rango para el orden canónico: números, luego símbolos, luego aplicaciones.
    /// Es una convención, pero tiene que ser total y estable (ver `order.rs`).
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Node::Int(_) => 0,
            Node::Rat(_) => 1,
            Node::Sym(_) => 2,
            Node::App { .. } => 3,
        }
    }
}

/// Los fallos de la arena. Ninguno es un panic: un panic en WASM mata el Worker
/// y se lleva la sesión del usuario (§6).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoreError {
    /// Un `ExprId` que no viene de esta arena.
    UnknownExpr(ExprId),
    /// Un `SymId` que no viene de esta arena.
    UnknownSymbol(SymId),
    /// Más de 2^32 nodos, argumentos o símbolos. Inalcanzable en la práctica,
    /// pero es la alternativa honesta a un cast que trunca en silencio (§9).
    ArenaFull,
    /// Dos nodos del mismo rango que no son de la misma variante. Hoy es
    /// imposible —`rank` es inyectivo—, y el día que deje de serlo el orden
    /// canónico deja de ser total, `sort_canonical` deja de ordenar y el
    /// hash-consing funde expresiones distintas. Sin crash y sin test que falle,
    /// que es la peor forma de fallar que tiene un CAS (§9).
    IncomparableNodes(ExprId, ExprId),
}

impl core::fmt::Display for CoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CoreError::UnknownExpr(id) => write!(f, "expresión desconocida: {id:?}"),
            CoreError::UnknownSymbol(id) => write!(f, "símbolo desconocido: {id:?}"),
            CoreError::ArenaFull => write!(f, "arena llena"),
            CoreError::IncomparableNodes(left, right) => {
                write!(f, "expresiones incomparables: {left:?} y {right:?}")
            }
        }
    }
}

impl core::error::Error for CoreError {}
