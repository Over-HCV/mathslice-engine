# Roadmap: etapas E0 → E∞

> El estado actual **no vive aquí** — vive en [`../context/STATUS.md`](../context/STATUS.md). Este documento define las etapas; aquel dice en cuál estamos.

## Principio de etapas

Cada etapa es una **rebanada vertical**: parser → kernel → resultado → UI, funcionando end-to-end.

**Nada de construir capas horizontales completas.** No se construye "todo el parser", luego "todo el evaluador", luego "toda la UI". Se construye lo mínimo de cada uno que hace funcionar un caso real, y se ensancha.

## La superficie total

El inventario de capacidades ([`../docs/main.md`](../docs/main.md)) tiene **1709 capacidades hoja** en 21 categorías. Descontando Common Core Math (751 hojas de re-etiquetado curricular K-12, no cómputo nuevo), la superficie real es de **~958 capacidades**.

Es un plan explícitamente plurianual. Lo que la arquitectura fija son los cimientos, para que ese apilamiento nunca exija refactor del kernel.

---

## E0 — Espina dorsal (sin matemáticas interesantes) · ✅ cerrada

**Crates:** `ms-core`, `ms-num`, `ms-parse` (LaTeX), `ms-serialize`, `ms-rules` con traza, `ms-wasm`.
**TS:** `src/lib/engine/worker.ts`, `types.ts`, `represent/toSteps.ts`.

**Criterio de salida:** entra `\frac{2}{3}+\frac{1}{6}` desde el navegador y sale `\frac{5}{6}` exacto, aproximación decimal, **y la traza de pasos**.

Sin gráficas todavía. **La primera representación es la de pasos, no la visual** — es coherente con que los pasos sean el objetivo primario.

> Aquí se fijan los principios §1–§3, §6, §8 y §9 de [`01-rust-principles.md`](01-rust-principles.md). Los lints y perfiles se configuran **en el primer commit**; después es tarde y se vuelve refactor de cimientos.

---

## E1 — Aritmética y números · ✅ cerrada

*Elementary Math (36 hojas) + el núcleo de Numbers*

> **Cerrada en las cinco fases.** El criterio de salida está en verde en el
> navegador, el corpus no tiene ninguna deuda viva y la cobertura del proyecto la
> calcula `ms-cli`: 3,34 % de las 958. El estado y lo que trajo cada fase están en
> [`../context/STATUS.md`](../context/STATUS.md); la ejecución, en
> [`../context/E1-PLAN.md`](../context/E1-PLAN.md).

Enteros grandes, racionales con las cuatro operaciones y potencias enteras,
PEMDAS, bases numéricas —leídas con `1011_{2}` y escritas con
`\operatorname{enBase}(2, x)`—, divisibilidad, MCD/mcm, decimales periódicos.

Entra `ms-verify` **nivel 0** y `ms-assume`.

**Y `facts`, adelantado desde E3 — ya hecho, en la fase 3.** Las propiedades de un
entero —paridad, signo, primalidad, divisores, factorización— *son* hechos, y
aparecen aquí. Estrenar el campo con las clases que E1 sabe emitir de verdad hace
que E3 sólo tenga que añadir clases; al revés, E3 tendría que estrenar el campo
**y** cambiar el contrato. El campo entró con datos dentro, no antes: un array
siempre vacío no es un contrato
([`../crates/ms-serialize/src/result.rs`](../crates/ms-serialize/src/result.rs)).
El crate `ms-facts` nace aquí, con `ms-num` como única dependencia; E3 le añade
`alternates` y los hechos sobre expresiones, que es cuando entrará `ms-core`.

> **`\gcd(a, b)` consultable se movió a E2**, y el MCD sigue disponible en E1 por
> la vía de los hechos: se factorizan los dos números.
>
> **La decisión del kernel que arrastraba sí se tomó, en la fase 4 de E1**, porque
> el redondeo la necesitaba: `Head` creció **una** vez con `Fn(Builtin)`, un enum
> cerrado dentro de otro. Ni una variante por función —en E6 serían cientos
> dentro de un enum `Copy`— ni `Head::Fn(SymId)`, que no vuelve a tocar el kernel
> y a cambio apaga la exhaustividad: con un símbolo cualquiera ahí, añadir una
> función no rompe ningún `match` y una que el verificador no conozca sale sin
> verificar sin que nadie se entere.
>
> Lo que queda para E2 es el azúcar, no la forma: `\gcd` es hoy una variante de
> `Builtin` y una regla. Y para E3, `UserFn(SymId)` —`f(x)` con `f` sin definir—,
> que es otra decisión: una función que el motor no conoce no se evalúa, y eso hay
> que decirlo en su sitio.

**Criterio de salida:** entra `\frac{7}{12}\div\frac{14}{9}+2^{40}` desde el
navegador y sale `\frac{8796093022211}{8}` exacto, con su aproximación decimal,
sus seis pasos, y el documento marcado **`verificado · nivel 0`**.

> **Las 58 hojas de *Numbers* no caben enteras en E1.** El bloque incluye
> irracionales, algebraicos, trascendentes y complejos, cuya aritmética depende de
> la torre algebraica y de la capa hipergeométrica que este mismo documento sitúa
> en E5. Esas familias se cuentan en **E5**, no aquí. *Pie charts & tape diagrams*
> tampoco es motor: es representación, y sale de `facts` en la capa de TypeScript.
> El recorte completo está en [`../context/STATUS.md`](../context/STATUS.md).

---

## E2 — Polinomios y álgebra base ✅

*Cerrada. Cobertura al cierre: 40 de 958 — 4,18 %*

- `ms-alg` genérico en `C: Coefficient`, con el puente `Expr ↔ Poly` aparte.
- `expandir`, `factorizar` y `mcd` como **consultas con nombre**.
- `ms-solve`: resolver lineales y polinómicas factorizando sobre ℚ.
- `ms-verify` **nivel 1** por sustitución, y el muestreo del nivel 0.
- **`engine/lean/` arrancó**, con cinco reglas de E1 demostradas contra Mathlib.

**Criterio de salida, verificado:** entra `x^{2}-5x+6 = 0` desde el navegador y
salen `\left\{x = 2, x = 3\right\}` con cuatro pasos y el sello
**`verificado · nivel 1`**.

> **Tres recortes, decididos con el usuario y escritos aquí para que no se
> reabran solos.**
>
> **Álgebra lineal (63 hojas) sale de E2.** Una matriz exige una cabeza nueva en
> el IR, `\pmatrix` en el parser, un tipo de valor nuevo y su escritura en
> LaTeX: es un eje **ortogonal** a la espina polinómica, no un paso más de ella.
> Entra donde le toque por su cuenta, no colgando de los polinomios.
>
> **Los e-grafos (`egg`) salen de E2.** Los polinomios **sí** tienen forma
> canónica computable, que es justo el caso en el que los e-grafos no aportan:
> son búsqueda para cuando no la hay ([`02-architecture.md`](02-architecture.md)
> §«La división que importa»). Y `egg` es una dependencia grande contra el
> presupuesto de WASM. Con ellos se fue **la forma de salida propia de
> `ms-simplify`**, así que los dos cabos sueltos que la esperaban —el `\frac`
> con varios factores en el denominador y el `1` de `Times(1, b^{-1})`— siguen
> abiertos.
>
> **Lean entró como cable, no como corpus:** cinco reglas, no las doce. Demostrar
> que el cable existe antes de que haya 200 teoremas que escribir es lo que hace
> que el resto sea trabajo y no investigación.

> **Y la decisión que E2 tomó sobre la marcha:** expandir y factorizar **no
> pueden ser reglas automáticas**, porque son inversas y el reescritor ciclaría.
> Son consultas con nombre —`\operatorname{expandir}`,
> `\operatorname{factorizar}`— que es además como las enuncia el árbol de
> capacidades. Dentro de una ecuación factorizar sí es automático, y ahí no hay
> con quién ciclar.

---

## E3 — Cálculo diferencial

*Parte de Calculus & Analysis (149 hojas)*

> **E2 le deja el límite de los irracionales, no su solución.** `x^{2}-2=0` sale
> con un error que dice que sus soluciones no son racionales — no que no las
> tenga, que sería falso. Escribirlas exige `\sqrt` y con él la torre algebraica,
> que este mismo documento sitúa en E5.

Derivadas, regla de la cadena, diferenciación implícita, límites (Gruntz), series.

> **Las derivadas con pasos son el mayor retorno pedagógico por unidad de esfuerzo del proyecto entero.**

Entra `ms-facts`, que **añade clases** al campo `facts` que E1 ya estrenó, y con él la **primera representación visual**: `represent/toGraph.ts` consume hechos y los pasa a `graphEngine.ts`.

**La gráfica llega derivada de hechos ya calculados, nunca como salida directa del motor** ([`02-architecture.md`](02-architecture.md) §1).

---

## E4 — Integración

- **Rubi como datos** → tabla + heurísticas + sustitución + partes.
- Risch parcial (caso transcendente) si hace falta.
- Definidas vía primitiva; numéricas por Gauss–Kronrod.

Risch completo queda **fuera de alcance a propósito**. Cuando no hay primitiva elemental se reporta con certificado, o se cae a numérico. Nunca se inventa una respuesta.

---

## E5 — Trigonometría, complejos, funciones especiales

*Trigonometry (16) + Complex Analysis (16) + Mathematical Functions (82)*

Aquí entra la **capa hipergeométrica**. Es lo que hace viables las 82 hojas de funciones especiales: sin ella son 82 implementaciones independientes; con ella son casos derivados de ₚFq y Meijer G.

---

## E6+ — Ancho

Number Theory (39), Discrete Mathematics (65), Statistics (30), Probability (45), Geometry (114), Differential Equations (9), Applied Mathematics (56), Logic & Set Theory (16).

---

## E∞ — Common Core Math (751 hojas)

Tabla de mapeo sobre lo ya construido. **Días, no meses** — es un índice curricular, no cómputo nuevo.

---

## Fuera del motor

*Famous Math Problems* (6) y *Mathematical Definitions* (5) **no son motor**: son contenido curado. Van por el pipeline editorial de `media/maths/`, no por Rust.

---

## Riesgos y respuestas

| Riesgo | Respuesta |
|---|---|
| La integración simbólica robusta es genuinamente difícil | Rubi cubre gran parte, con pasos incluidos. Risch completo fuera de alcance a propósito: se reporta "no elemental" con certificado, o se cae a numérico. **Nunca se inventa una respuesta** |
| El reescritor cicla o no termina | Presupuesto de iteraciones + detección de ciclos por hash-consing (ids iguales = expresión ya vista). Los e-grafos evitan el problema por construcción en la fase de simplificación |
| El matching con `Flat`+`Orderless` explota | Red de discriminación compilada en `build.rs`; benchmarks de `criterion` desde E0, no al final |
| Rust es más lento de desarrollar que envolver SymPy | Cierto, y es el coste aceptado. Sin él no hay pasos, que es el objetivo primario |
| Lean se vuelve un pozo de tiempo | Acotado por diseño: solo las ~200 reglas núcleo, solo en CI, y **opcional por regla**. Nunca bloquea una etapa |
| 958 capacidades es enorme | Lo es. El plan es plurianual e incremental. La arquitectura existe para que el apilamiento nunca exija refactor del kernel |
