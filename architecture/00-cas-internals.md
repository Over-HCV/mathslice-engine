# Cómo funciona un CAS comercial por debajo (solo lo matemático)

> Punto de partida del diseño. Antes de decidir cómo construimos el motor, hay que entender qué construyó el CAS comercial de referencia y por qué esas decisiones aguantaron 35 años.
> Lo que sigue cubre **solo la capa de cómputo matemático**. El front-end lingüístico y la capa de datos curados quedan fuera a propósito.

Un sistema de cálculo comercial son tres capas. Nos interesa **solo la del medio**.

```
[1] Front-end lingüístico  →  NLU, desambiguación, traducción a su lenguaje propio
[2] Kernel del CAS         →  EL MOTOR MATEMÁTICO            ← esto es lo que nos interesa
[3] Capa de "pods"         →  qué mostrar, datos curados, formato de salida
```

---

## 1. Una sola estructura de datos

Todo es `head[arg1, …, argn]`. Sin excepciones sintácticas.

```
2 + 3     es en realidad   Plus[2, 3]
x^2       es               Power[x, 2]
{1, 2, 3} es               List[1, 2, 3]
∫x²dx     es               Integrate[Power[x, 2], x]
```

Átomos: `Integer`, `Rational`, `Real`, `Complex`, `String`, `Symbol`. Todo lo demás es un árbol de aplicaciones.

**Por qué importa:** una estructura ⇒ un evaluador, un matcher, un impresor. Es la razón de que el sistema escalara a ~6000 funciones sin colapsar bajo su propio peso.

**Lección #1, no negociable.** La copiamos tal cual → [`02-architecture.md`](02-architecture.md) §2, crate `ms-core`.

---

## 2. Evaluar = reescribir hasta punto fijo

El evaluador no "ejecuta" nada. Reescribe la expresión repetidamente hasta que deja de cambiar:

1. Evalúa el head.
2. Evalúa los argumentos — **salvo** que los atributos `HoldAll`/`HoldFirst`/`HoldRest` lo impidan. Es necesario para `Set`, `If`, `Table`, y para que `Integrate[f, x]` no intente evaluar `x`.
3. Aplica atributos estructurales.
4. Busca reglas que hagan match: primero las del usuario (`DownValues`), luego las internas.
5. Si algo cambió, vuelve a 1. Corta en `$IterationLimit`.

---

## 3. Los atributos hacen la canonicalización

| Atributo | Efecto | Ejemplo |
|---|---|---|
| `Flat` | asociatividad: aplana anidamiento | `Plus[a, Plus[b,c]]` → `Plus[a,b,c]` |
| `Orderless` | conmutatividad: orden canónico | `Plus[b,a]` → `Plus[a,b]` |
| `OneIdentity` | `f[x]` ≡ `x` para el matching | permite que `a` matchee `x_ + y_.` |
| `Listable` | threading automático sobre listas | `Sin[{1,2}]` → `{Sin[1], Sin[2]}` |
| `Hold*` | bloquea evaluación de argumentos | `Integrate`, `Plot`, `Set` |

`Flat` + `Orderless` son lo que permite que **una sola** regla `x_ + x_ -> 2 x_` cubra `a+b+a`.

También son lo que vuelve el matching combinatoriamente caro. Por eso un CAS maduro filtra las reglas con una **red de discriminación** — indexa por head y forma de argumentos — antes de intentar el match real.

> Consecuencia para nosotros: la red de discriminación no es una optimización opcional, es parte del diseño desde el día 0. Ver [`01-rust-principles.md`](01-rust-principles.md) §5: la compilamos en `build.rs`.

---

## 4. Torre numérica: exacto por defecto

- `Integer` y `Rational`: precisión arbitraria **siempre** (GMP por debajo). `2/3` **no** es `0.667`.
- `Real`: **aritmética de significancia** — cada número arrastra su precisión y el error se propaga a primer orden. No son bounds rigurosos; es un modelo heurístico de propagación.
- `N[expr, 50]` re-evalúa la expresión **simbólica** a 50 dígitos. No refina el float previo.

Por eso un sistema así muestra "Exact result" y "Decimal approximation" como pods separados: son **dos objetos distintos** del kernel, no dos formatos del mismo objeto.

> Nosotros mejoramos este punto: aritmética de bolas (centro ± radio) con bounds rigurosos en vez de significancia heurística. Ver [`03-lean-verification.md`](03-lean-verification.md) §3, nivel 0.

---

## 5. El punto incómodo: no existe forma canónica general

`Simplify` **no** es una normalización. Es una **búsqueda**:

1. Genera variantes aplicando reglas de transformación.
2. Puntúa cada una con una función de complejidad (`LeafCount` por defecto).
3. Devuelve la mejor que encontró.

`FullSimplify` busca más ancho, añadiendo identidades de funciones especiales.

**Esto no es pereza de ingeniería.** Por el **teorema de Richardson**, decidir si una expresión elemental es idénticamente cero es **indecidible**. No existe forma canónica computable para expresiones generales. Toda simplificación general es búsqueda heurística — en cualquier CAS que exista.

**Pero** en subdominios sí existe forma canónica, y es barata:

| Dominio | Forma canónica | Función WL |
|---|---|---|
| Polinomios | expandida y ordenada | `Expand`, `Collect` |
| Funciones racionales | numerador/denominador coprimos | `Together`, `Cancel` |
| Factorización sobre ℤ | producto de irreducibles | `Factor` |

> **Esta división — dominios con forma canónica vs. búsqueda heurística fuera de ellos — es la decisión arquitectónica más importante del kernel entero.** Determina la separación entre `ms-alg` (canónico, determinista) y `ms-simplify` (búsqueda). Ver [`02-architecture.md`](02-architecture.md) §2.

---

## 6. Los algoritmos reales detrás de cada operación

| Operación | Algoritmo |
|---|---|
| MCD de polinomios | MCD modular / heurístico. Euclides directo explota en tamaño de coeficientes |
| Factorización sobre ℤ | Berlekamp o Cantor–Zassenhaus sobre 𝔽ₚ + levantamiento de Hensel + recombinación (Zassenhaus / van Hoeij) |
| `Solve` de sistemas polinomiales | Bases de Gröbner (Buchberger, F4/F5) |
| `Reduce` sobre los reales | **CAD** — Cylindrical Algebraic Decomposition (Collins). Doblemente exponencial, pero es un procedimiento de decisión completo |
| `Integrate` indefinida | **Algoritmo de Risch** — procedimiento de decisión para funciones elementales: encuentra la primitiva o **demuestra** que no existe en términos elementales. Enorme. Complementado con una tabla masiva de reglas por patrón |
| `Integrate` definida | Método **Meijer G** (Marichev–Adamchik): convierte los factores a funciones G, aplica el teorema de convolución, convierte de vuelta. Así resuelve definidas que no tienen primitiva elemental |
| `Limit` | Algoritmo de Gruntz (MRV — *most rapidly varying subexpression*). **No** L'Hôpital |
| `Series` | aritmética sobre series de potencias truncadas, como objeto de primera clase |
| `DSolve` | clasificación por patrón (separable, exacta, lineal, Bernoulli, Riccati, Euler…) + simetrías de Lie + transformadas integrales |
| Funciones especiales | **todo se reduce a hipergeométricas** ₚFq y Meijer G |
| `NIntegrate` | Gauss–Kronrod adaptativo + manejo explícito de singularidades |
| `FindRoot` | Newton / secante / Brent |

### El truco de unificación: la capa hipergeométrica

`Sin`, `Erf`, `BesselJ`, `LegendreP`, las integrales elípticas — **todas** son ₚFq o Meijer G con parámetros distintos. Miles de "identidades" no son entradas de tabla: son casos derivados de un único motor.

> Es lo que hace que la categoría *Mathematical Functions* (82 hojas en [`../docs/mathematical-functions.md`](../docs/mathematical-functions.md)) sea viable en vez de imposible. Sin esta unificación, son 82 implementaciones independientes.

---

## 7. Suposiciones y cortes de rama: donde los CAS mienten

```
Sqrt[x^2]     ≠  x          (es |x| para x real, y ni eso en ℂ)
Log[a b]      ≠  Log[a] + Log[b]
(x^a)^b       ≠  x^(a b)
```

El CAS de referencia lleva `Assumptions` como **canal lateral** hacia `Simplify`/`Refine`/`ComplexExpand`.

**Aquí es donde todo CAS —incluidos los comerciales— produce respuestas incorrectas.** Es la fuente #1 de errores reales, muy por encima de los bugs de algoritmo.

> Este es el punto que atacamos de frente. Las suposiciones son un contexto **tipado y de primera clase** (`ms-assume`), no un argumento opcional: una regla que necesita `x > 0` no compila sin declararlo. Ver [`01-rust-principles.md`](01-rust-principles.md) §3.

---

## 8. Lo que el sistema añade encima del kernel

- Traducción de la consulta a su lenguaje propio.
- **Selección de pods**: dada la entrada, decidir qué mostrar (interpretación, formas alternas, raíces, gráfica, serie, propiedades…). Es una **tabla curada por dominio**, no algo automático.
- **El step-by-step NO sale del kernel.** Es un producto aparte, con sus propios conjuntos de reglas de nombres legibles (linaje Rubi).

> Ese último punto es la confirmación directa de nuestra tesis central: **si los pasos no se diseñan desde el día 0, no se retrofitean.** El CAS comercial tuvo que construir un segundo sistema. Nosotros hacemos la traza parte del evaluador. Ver [`01-rust-principles.md`](01-rust-principles.md) §5.

---

## 9. Resumen: qué copiamos y qué no

**Copiamos** (probado, no hay mejor idea):

1. Expresión uniforme `head[args]` como único tipo del kernel.
2. Evaluación como reescritura a punto fijo, con `Flat`/`Orderless`/`Hold*`.
3. Exacto por defecto, numérico solo bajo demanda.
4. Red de discriminación para filtrar reglas antes del match.
5. Formas canónicas solo en subdominios; búsqueda heurística fuera de ellos.
6. Capa hipergeométrica como unificador de funciones especiales.
7. Resultado = documento estructurado, no un valor suelto.

**No copiamos** — y el porqué está en cada documento:

| CAS comercial | Nosotros | Dónde |
|---|---|---|
| `Simplify` = búsqueda voraz | E-grafos / saturación por igualdad | [`02-architecture.md`](02-architecture.md) §2 (`ms-simplify`) |
| Pasos = producto separado | Traza integrada en el evaluador | [`01-rust-principles.md`](01-rust-principles.md) §5 |
| Aritmética de significancia | Aritmética de bolas, bounds rigurosos | [`03-lean-verification.md`](03-lean-verification.md) §3 |
| Suposiciones como argumento opcional | Contexto tipado, verificado en compilación | [`01-rust-principles.md`](01-rust-principles.md) §3 |
| Confianza por antigüedad | Verificación en 3 niveles, hasta Lean | [`03-lean-verification.md`](03-lean-verification.md) |
| Lenguaje propio obligatorio | IR interno; la entrada es LaTeX | [`02-architecture.md`](02-architecture.md) §2 (`ms-parse`) |
| Pods = cómputo + presentación fundidos | Hechos semánticos; representar es otra capa | [`02-architecture.md`](02-architecture.md) §1 |
| Servidor, round-trip por consulta | WASM en el navegador | [`01-rust-principles.md`](01-rust-principles.md) §8 |
| Tabla de integración propietaria | Rubi (~6700 reglas, MIT, con nombres de paso) | [`04-roadmap.md`](04-roadmap.md) E4 |
