# Lean 4 y los tres niveles de verificación

> Cómo compramos, de forma barata y mecánica, buena parte de los 35 años de casos borde que le dan a los CAS comerciales su reputación de robustez.

---

## 1. Qué es Lean y qué no es

Lean 4 es un **asistente de demostración** con tipos dependientes. Apache 2.0, gratis, desarrollado por la Lean FRO. Mathlib, su librería matemática comunitaria, supera los 200 000 teoremas y definiciones.

**Es otra categoría de herramienta:**

| | CAS (comercial, nosotros) | Asistente de demostración (Lean) |
|---|---|---|
| Qué hace | **Computa** una respuesta | **Verifica** una afirmación |
| ¿Puede equivocarse? | Sí, y lo hace | No (kernel de confianza diminuto) |
| `∫x²sin x dx` | Devuelve la primitiva | **No la encuentra** — tú la traes y demuestras que es correcta |
| Velocidad | milisegundos | minutos u horas de trabajo humano por teorema |

**Lean no puede reemplazar el motor.** No existe táctica que encuentre primitivas. Mathlib tiene teoría de la integral, pero calcular una antiderivada concreta es trabajo manual. Sus tácticas de cómputo (`ring`, `norm_num`, `linarith`, `polyrith`, `decide`) **demuestran identidades dadas**; no exploran el espacio de soluciones.

Tampoco es viable en runtime: los archivos `.olean` de Mathlib pesan cientos de MB. Contra un presupuesto de 3 MB de WASM ([`01-rust-principles.md`](01-rust-principles.md) §8), descartado sin discusión.

---

## 2. Dónde sí encaja, y es excelente: soundness de las reglas, en tiempo de diseño

**Demostrar una vez, en CI, que cada regla de reescritura es correcta.**

El campo `soundness` del macro `rules!` apunta a un teorema:

```rust
#[soundness = lean("Mathlib.Analysis.Calculus.Deriv.Mul.deriv_mul")]  // ya está en Mathlib
#[soundness = lean_local("engine/lean/Rules/TrigHalfAngle.lean")]      // demostrado por nosotros
#[soundness = numeric_only]                                            // solo muestreo, y queda marcado
```

`engine/lean/` es un proyecto Lean que enuncia cada regla del núcleo como teorema. CI corre `lake build`.

**Métrica del proyecto: % de reglas con soundness verificada por máquina.**

### Por qué esto es fuerte

- **Coste en runtime: cero. Coste en WASM: cero.** Es tiempo de compilación y de CI, no de consulta.

- **Ataca exactamente el punto débil del §7 de [`00-cas-internals.md`](00-cas-internals.md).** Las reglas que fallan en casos borde —`√(x²) = x`, `log(ab) = log a + log b`, `(xᵃ)ᵇ = xᵃᵇ`— son justo las que **no se pueden demostrar sin hipótesis**. Lean te obliga a escribir la hipótesis, y esa hipótesis se convierte en el `where` de la regla.

  > **La demostración descubre la suposición faltante.** No hay forma más barata de encontrar ese bug — es el bug #1 de todos los CAS y aquí sale gratis al intentar demostrar.

- **Los CAS comerciales tienen cero de esto.** Existe trabajo académico sobre CAS verificados, pero ningún producto lo hace.

### Alcance realista

Las ~6700 reglas de Rubi: **no**.
Las ~200 reglas núcleo de álgebra y cálculo: **sí**, y son las que producen la mayoría de los errores.

Lean está acotado por diseño y **nunca bloquea una etapa**: `numeric_only` es una respuesta válida para cualquier regla.

---

## 3. Los tres niveles de `ms-verify`

| Nivel | Qué hace | Cuándo corre | Coste y garantía |
|---|---|---|---|
| **0 — evaluación de la identidad** | **Sin variables libres:** evalúa las dos expresiones en ℚ y compara los racionales exactos. **Con una variable libre:** las evalúa en 12 puntos racionales **fijos** y compara | Runtime, cada consulta | µs. Exacto en el primer caso; evidencia fuerte en el segundo |
| **1 — inversión simbólica** | Sustituye cada raíz en la ecuación original y evalúa los dos lados; con integrales, deriva la primitiva y compara | Runtime, cuando la operación tiene inversa | ms. Determinista para esa consulta concreta |
| **2 — soundness en Lean** | Cada regla aplicada en la traza tiene teorema demostrado | Tiempo de diseño / CI | Cero en runtime. **Es la garantía fuerte** |

El `ResultDoc` reporta el nivel alcanzado **y el método**, porque «evaluado en ℚ» y «muestreado en 30 puntos» no valen lo mismo y el nivel solo no los distingue.

> **El nivel 0 no es un sinónimo de «probabilístico».** Lo que hay en E1 es aritmética cerrada —`\frac{7}{12} \div \frac{14}{9} + 2^{40}` no tiene ninguna variable—, y comprobarla es evaluar los dos lados exactamente y comparar. Determinista, total y sin falsos positivos. Reportar como probabilístico algo que es exacto sería subestimar la propia garantía.

> **El muestreo de E2 salió con dos correcciones sobre lo que este documento decía.** Los puntos son **fijos**, no aleatorios: un sello que sale distinto en cada ejecución no se puede reproducir, y entonces no sirve cuando alguien discute un resultado. Y son **racionales exactos**, no flotantes a 50 dígitos con aritmética de bolas: el evaluador ya trabaja en ℚ, así que la comparación es exacta en cada punto y no hace falta acotar ningún error. La aritmética de bolas volverá a hacer falta cuando entren los irracionales, en E5.
>
> Lo que sigue siendo cierto es que **no demuestra**: dos expresiones distintas pueden coincidir en los puntos probados. Por eso el muestreo es nivel 0 y no 1, y por eso el `method` del documento lo dice.

> **El nivel 1 de E2 afirma menos de lo que su nombre sugiere, y a propósito.** Sustituir las raíces comprueba que **cada solución devuelta cumple la ecuación**; no comprueba que no falte ninguna. Verificar la completitud exigiría dividir por los factores hallados y buscar raíces en el cociente, que es exactamente el código que produjo la respuesta: la comprobación se convertiría en un eco. Es preferible sellar menos y que el sello signifique algo.

**Un resultado de nivel 2 significa:** *toda regla usada en esta derivación está demostrada, y las hipótesis de cada una se cumplieron.*

Eso es más de lo que un sistema de cálculo comercial puede decir de cualquier resultado suyo.

### Por qué tres niveles y no uno

Los niveles 0 y 1 son de **consulta**: verifican *este* resultado, ahora, barato. El nivel 2 es de **diseño**: verifica la *regla*, una vez, para siempre.

Se complementan. El nivel 2 garantiza que la regla es correcta bajo sus hipótesis; los niveles 0 y 1 atrapan errores de *aplicación* (un binding mal hecho, un caso que el matcher confundió). Ninguno sustituye al otro.

---

## 4. Dos usos secundarios, baratos

- **Export a Lean.** El motor emite el esqueleto de demostración de un resultado, para que un usuario avanzado lo verifique o lo extienda. Cae casi solo desde la traza que ya producimos.

- **Contenido.** Lean es material didáctico directo para `a-origins/01-mathematical-logic/02-proof-techniques` y `03-godel-incompleteness-theorems` del árbol de `media/maths/tree.md`. Es herramienta y tema a la vez.

---

## 5. Cuándo entra

**A partir de E2** ([`04-roadmap.md`](04-roadmap.md)), no antes. Hasta que no exista un rule set estable no hay nada que demostrar.
