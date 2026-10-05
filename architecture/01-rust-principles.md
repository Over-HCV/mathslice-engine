# Los principios de Rust que guían todo el desarrollo

> No estamos escribiendo C++. Estas capacidades del lenguaje que hacen **posible** este diseño, ordenadas por frecuencia de aparición.
>
> **Cada principio tiene una CONSECUENCIA OBLIGATORIA. No son recomendaciones.** Si un cambio las viola, el cambio está mal — no el principio.

---

## 1. Enums algebraicos + `match` exhaustivo → el IR crece sin romperse

El IR es un *sum type*, no una jerarquía de clases con visitor:

```rust
pub enum Node {
    Int(BigInt),
    Rat(BigRat),
    Sym(SymId),
    Real(Ball),
    App { head: Head, args: ArgSlice },
}
```

El compilador **prueba exhaustividad**. Añadir un head nuevo en el mes 14 hace que el build enumere cada sitio que falta manejar. En Python/JS eso es un `KeyError` en producción; en Java es un visitor que hay que acordarse de actualizar.

Este motor va a crecer durante años hacia ~958 capacidades. La exhaustividad verificada es lo único que hace ese crecimiento seguro.

> ### Consecuencia obligatoria
> **Prohibido el brazo comodín `_ => …` en cualquier `match` sobre `Node` o `Head`.** Se enumera, siempre.
> Lint `wildcard_enum_match_arm` en `deny`.

---

## 2. Ownership → arena con índices, sin GC ni refcount

Nada de `Rc<RefCell<Expr>>`. Arena `Vec<Node>` + `ExprId(u32)` + **hash-consing**: dos expresiones estructuralmente iguales son el mismo id.

De ahí salen tres cosas gratis:

- `a == b` es comparar dos enteros, no recorrer dos árboles.
- Las subexpresiones se comparten sin coste (estructuras persistentes).
- La detección de ciclos en el reescritor es "¿ya vi este id?".

Esto es **exactamente** donde SymPy pierde: su perfil está dominado por asignación de objetos y recolección de basura. Aquí no hay GC porque no hay recolector.

> ### Consecuencia obligatoria
> **Las expresiones nunca se clonan profundamente.** Toda API del kernel toma y devuelve `ExprId`.
> **Reescribir = insertar nodos nuevos, nunca mutar los existentes.**

---

## 3. Newtypes de coste cero y typestate → las invariantes matemáticas viven en el sistema de tipos

**Este es el principio más importante para nuestra corrección.**

Los estados matemáticos son parámetros de tipo, con **cero coste en runtime**:

```rust
struct Canonical(ExprId);            // pasó por canonicalización
struct Poly<C, F: Form>(…);          // Form = Expanded | Factored | Collected
struct Verified<T>(T, Evidence);     // salió de ms-verify

fn factor(p: Poly<Z, Expanded>) -> Poly<Z, Factored>;
fn solve(e: Canonical, ctx: &Assumptions) -> Result<SolutionSet, MathError>;
```

Las cuatro invariantes de [`02-architecture.md`](02-architecture.md) §4 dejan de ser convenciones de documento y pasan a ser **imposibles de violar**:

- No se puede pasar una expresión sin canonicalizar donde se exige canónica.
- No se puede mostrar en UI un resultado que no atravesó `Verified<T>`.
- No se puede disparar una regla que requiere `x > 0` sin un `Assumptions` que lo contenga.

**No compila.** No es un test que puede fallar tarde: es un error de compilación.

Esto ataca de frente el punto débil del §7 de [`00-cas-internals.md`](00-cas-internals.md): las suposiciones son la fuente #1 de respuestas incorrectas en todo CAS, precisamente porque en los demás son opcionales.

> ### Consecuencia obligatoria
> **Toda precondición matemática que escribirías en un comentario va en el tipo.**
> Si aparece un comentario del tipo `// requiere expandido` o `// asume x real`, eso es un bug de diseño, no documentación.

---

## 4. Traits + monomorfización → torre numérica genérica sin coste de abstracción

Un solo código de polinomios corriendo sobre ℤ, ℚ, 𝔽ₚ, ℤ[i], ℚ(α):

```rust
trait Coefficient: Add + Mul + Neg + Eq + Clone { fn zero() -> Self; … }
trait Field: Coefficient { fn inv(&self) -> Option<Self>; }

fn gcd<C: Field>(a: &Poly<C>, b: &Poly<C>) -> Poly<C>;
```

La factorización modular **necesita** precisamente esto: el mismo algoritmo corriendo sobre 𝔽ₚ para varios primos, y luego levantado a ℤ por Hensel. Escribirlo dos veces sería duplicar el algoritmo más delicado del crate.

Rust monomorfiza: genera código especializado por tipo, sin dispatch dinámico. Python paga dispatch en **cada operación de coeficiente**. C++ da templates equivalentes pero sin *trait bounds* verificados — los errores salen como muros de texto en la instanciación.

> ### Consecuencia obligatoria
> **Ningún algoritmo algebraico se escribe contra un tipo concreto.**
> Si aparece `BigInt` dentro de `ms-alg`, es un bug: debe ser `C: Coefficient`.

---

## 5. Macros procedurales → las reglas se compilan, no se interpretan

El DSL de reglas se expande **en tiempo de compilación** a código de matching y a entradas de la red de discriminación:

```rust
rules! {
    #[name = "Integración por partes", step = r"\int u\,dv = uv - \int v\,du"]
    #[soundness = lean("MeasureTheory.integral_mul_deriv_eq_deriv_mul")]
    integrate(u_ * dv_, x_) => u_*v_ - integrate(v_ * du_, x_)
        where integrable(dv_, x_);
}
```

Un CAS comercial típico parsea sus reglas en runtime. Nosotros no:

- Cero parseo en arranque.
- Cero tabla dinámica; la red de discriminación es una estructura estática.
- WASM pequeño — las reglas son datos estáticos, no strings.

**Con las ~6700 reglas de Rubi que entran en E4, esta es literalmente la diferencia entre viable e inviable.**

El macro también es lo que garantiza que **cada** regla tenga id estable, nombre legible, plantilla de paso y campo de soundness. Es decir: es lo que hace que los pasos pedagógicos y la verificación existan por construcción en vez de por disciplina.

> ### Consecuencia obligatoria
> **Ninguna regla matemática se escribe como código Rust a mano. Todas pasan por `rules!`.**
> Escribir una regla a mano es saltarse la traza y la verificación de un plumazo.

---

## 6. `Result<T, E>` sin excepciones → los fallos matemáticos son datos

Un CAS falla constantemente de formas **legítimas**: la primitiva no es elemental, hace falta `x > 0`, el problema es indecidible, se agotó el presupuesto de iteraciones. Eso no es una excepción: **es información para el usuario**.

```rust
enum MathError {
    NotElementary { proof: RischCertificate },
    NeedsAssumption(Assumption),
    Undecidable(Reason),
    BudgetExhausted { iterations: u32 },
    Singular { at: ExprId },
}
```

Sube por `?` hasta la UI y se muestra como explicación, no como stack trace. Y el compilador **obliga** a manejarlo: no existe el `catch` olvidado.

> ### Consecuencia obligatoria
> **`panic!`, `unwrap()` y `expect()` prohibidos fuera de tests.** Lint en `deny`.
> Un panic en WASM mata el Worker entero y se lleva la sesión del usuario. Un `MathError` es un mensaje útil.

---

## 7. `Send`/`Sync` verificados → paralelismo real (esto SymPy no puede)

Python tiene GIL: SymPy no puede paralelizar nada. Nosotros sí, y con **garantía de compilación** de que no hay data races.

Dónde paga de verdad:

- Simplificación de subárboles independientes (`rayon`).
- Búsqueda en el e-grafo.
- Factorización modular: varios primos en paralelo.
- Testing diferencial sobre el corpus de 958 casos.

> ### Consecuencia obligatoria
> **La arena es append-only durante la evaluación.** Eso hace `&Arena` trivialmente `Sync` y habilita `rayon` sin locks.
> Cualquier diseño que quiera mutar nodos in-place mata el paralelismo. Se rechaza.

---

## 8. WASM sin runtime ni recolector

Rust→WASM no arrastra intérprete, ni GC, ni stdlib pesada. Go mete su runtime y su GC. C++ mete Emscripten. Python mete Pyodide — **~10 MB solo para arrancar**, y eso ya lo pagamos en `src/lib/pyRunner.ts`, así que sabemos exactamente lo que cuesta.

> ### Consecuencia obligatoria
> **Presupuesto de < 3 MB comprimido para E0–E3**, medido en CI con umbral que rompe el build.
> Toda dependencia nueva se justifica contra ese presupuesto.
> `opt-level = "z"` + LTO, allocator pequeño (`talc`/`wee_alloc`), nada de `std::fmt` innecesario, y las reglas como datos estáticos (§5) — nunca como strings a parsear.

---

## 9. `#![forbid(unsafe_code)]` + aritmética chequeada

El peor modo de fallo de un CAS **no es el crash**: es la **respuesta incorrecta**.

Un overflow silencioso en el grado de un polinomio o en un exponente devuelve un resultado plausible y equivocado, y nadie se entera nunca. Un crash, al menos, se ve.

> ### Consecuencia obligatoria
> **`#![forbid(unsafe_code)]` en todos los crates, sin excepción.** Si algún día un benchmark exige `unsafe`, se aísla en un crate propio con justificación escrita.
> **`overflow-checks = true` también en release.**
> Los tipos exactos son bignum por diseño y no desbordan. Donde se usa `u32`/`usize` (grados, aridades, precisión) va `checked_*` explícito.

---

## 10. El testing es del lenguaje, no infraestructura aparte

`proptest`, `cargo-fuzz`, `criterion` y los doctests son parte del mismo build. Sin CI extra, sin otro lenguaje, sin otro runner.

Y hay una coincidencia afortunada: **las leyes matemáticas SON property tests.**

```
d/dx ∫f dx     = f
factor ∘ expand = id
parse(latex(e)) = e
a + b           = b + a   (bajo canonicalización)
```

> ### Consecuencia obligatoria
> **Toda capacidad nueva llega con tres cosas:**
> 1. Su caso en el corpus dorado.
> 2. Al menos una propiedad algebraica en `proptest`.
> 3. Un benchmark de `criterion` si toca el matcher.
>
> El matcher es lo que se degrada silenciosamente. Se mide desde E0, no al final.

---

## 11. Cargo workspace → los límites entre crates son verificables

La regla "`ms-core` no depende de nada" no es un acuerdo entre personas: es `cargo-deny` en CI.

Lo mismo para la frontera de representación ([`02-architecture.md`](02-architecture.md) §4, invariante 4): que ningún crate de Rust conozca Desmos no se sostiene con buena voluntad, se sostiene con un `grep` que rompe el build.

> ### Consecuencia obligatoria
> **Toda frontera arquitectónica de este proyecto tiene un test que la verifica.** Si una regla de diseño no se puede comprobar mecánicamente, se reescribe hasta que se pueda, o se admite que es solo una preferencia.

---

## Tabla de referencia rápida

| # | Principio | Consecuencia obligatoria |
|---|---|---|
| 1 | Enums + match exhaustivo | Sin `_ =>` en `Node`/`Head` |
| 2 | Ownership → arena + hash-consing | Sin clones profundos; API en `ExprId`; append-only |
| 3 | Typestate / newtypes de coste cero | Toda precondición va en el tipo, no en un comentario |
| 4 | Traits + monomorfización | Ningún algoritmo algebraico contra un tipo concreto |
| 5 | Macros procedurales | Ninguna regla escrita a mano; todas por `rules!` |
| 6 | `Result<T, E>` | Sin `panic`/`unwrap`/`expect` fuera de tests |
| 7 | `Send`/`Sync` + rayon | Arena append-only durante evaluación |
| 8 | WASM sin runtime | < 3 MB comprimido, umbral duro en CI |
| 9 | `forbid(unsafe_code)` | Overflow checks también en release |
| 10 | Testing del lenguaje | Cada capacidad: corpus + property + bench |
| 11 | Workspace + cargo-deny | Toda frontera tiene un test que la verifica |
