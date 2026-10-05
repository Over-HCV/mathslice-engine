# STATUS del motor — estado vivo

> **Este es el archivo que responde "¿en qué va el motor, dónde trabajo?".**
> Se actualiza al cerrar cada etapa. No duplica contenido: solo estado y punteros.
>
> Estado del resto del repositorio (app web, contenido) → [`../../context/STATUS.md`](../../context/STATUS.md)
>
> Última actualización: **2026-07-26** (E3 con su criterio de salida en verde)

---

## En una línea

**E0, E1 y E2 cerradas y E3 con su criterio de salida en verde, todo comprobado
en un navegador real: entra `\frac{d}{dx}\left(\left(x^{2}+1\right)^{3}\right)`
y sale `6x\left(x^{2} + 1\right)^{2}` con cuatro pasos y el sello
`verificado · nivel 0 por derivación automática`; entra `x^{2}-5x+6 = 0` y salen
`\left\{x = 2, x = 3\right\}` con sello de nivel 1; y diez de las treinta reglas
llevan detrás un teorema demostrado en Lean contra Mathlib.**

---

## Etapa actual: E3 — Cálculo diferencial. 🟡 Criterio de salida en verde, falta la fase 7.

| Fase de E3 | | |
|---|---|---|
| 0 — Línea base | ✅ | Medida en la misma sesión, que es la única forma de que las cifras se puedan restar |
| 1 — La derivada en el IR | ✅ | `Builtin::Derivative`, `\frac{d}{dx}` en el parser y en el escritor |
| 2 — Las reglas | ✅ | Crate `ms-diff`, ocho reglas, y `like-terms`, que era de E2 |
| 3 — Verificación | ✅ | Números duales: derivar **evaluando**, exacto en ℚ |
| 4 — Lean | ✅ | `Calculus.lean` sobre `Polynomial ℚ`, siete teoremas, cinco reglas respaldadas |
| 5 — Corpus y cobertura | ✅ | `e3.json` con 12 casos, dos hojas de *Derivatives* marcadas |
| 6 — Hechos graficables | ✅ | Puntos exactos en el campo `facts`; el motor no sabe con qué se dibujan |
| 7 — Bloque multilínea con definiciones | ⬜ | `:=`, `UserFn`, `bindings` y la sesión de bloque. Diseño escrito en [`E3-PLAN.md`](E3-PLAN.md) |
| 8 — Cierre | ✅ | Bench contra la línea base de la fase 0, `twiggy`, `engine:browser` (112 casos contra el WASM real, cada uno pintado) y los documentos. **Falta la pasada visual sobre `/motor` con los ojos**, que es la que en E2 encontró lo que el corpus no |

Plan de ejecución fase a fase → [`E3-PLAN.md`](E3-PLAN.md).

### Criterio de salida de E3, verificado

`\frac{d}{dx}\left(\left(x^{2}+1\right)^{3}\right)` →
`6x\left(x^{2} + 1\right)^{2}`, con cuatro pasos: la regla de la cadena → la
derivada de una suma es la suma de las derivadas → la derivada de una constante
es cero → derivamos una potencia de x. El pie dice **Verificado (nivel 0, por
derivación automática en 12 puntos)**.

**Los dos últimos pasos salen en orden canónico, no de lectura**: el `+1` se
deriva antes que el `x^{2}` aunque salga escrito después. Cruzarlos exigiría que
el reescritor conociera el orden del escritor, que es presentación.

---

## Lo que trajo E3

### La derivada es una petición con nombre, y por otro motivo que expandir

`Builtin::Derivative` se escribe `\frac{d}{dx}\left(f\right)` —que es lo que se
lee en clase— y se acepta también como `\operatorname{derivar}(f, x)`. Las dos
son la misma expresión.

Expandir y factorizar son peticiones porque son **inversas entre ellas** y como
reglas ciclarían. Derivar no tiene inversa en este rule set —integrar es E4— así
que lo que la cabeza compra aquí es otra cosa: `2x` no es «la derivada de algo», y
sin la cabeza una expresión ya derivada y una sin derivar son indistinguibles.

**`\frac{d}{dx}` no puede parsearse como fracción**: `d` partido por `d\cdot x`
se cancela al canonicalizar y la petición desaparece por el camino.

**Y no es un átomo.** Liga como el menos unario, así que bajo un exponente
necesita paréntesis: sin ellos `Power(derivada, 2)` volvía a entrar como la
derivada de `f^{2}`. Lo encontró el round-trip con una forma que nadie escribe a
mano.

### `ms-diff`: derivar es estructural, y no pasa por el polinomio

Ocho reglas en su orden de intento, y **`ms-diff` no depende de `ms-alg`** a
propósito: pasar por `Poly` obligaría a que todo lo derivable fuera un polinomio,
y `\frac{1}{x}` no lo es. La gracia de la regla de la cadena es precisamente no
expandir `\left(x^{2}+1\right)^{3}`.

**La potencia antes que la cadena**, aunque una sea caso particular de la otra:
por la cadena, `x^{2}` daría `2x\frac{d}{dx}\left(x\right)` y un factor `1`
después. El resultado es el mismo y el paso que se lee es peor.

**La constante sale fuera con regla propia.** Sin ella,
`\frac{d}{dx}\left(3x^{2}\right)` se anuncia como «derivada de un producto»,
que es correcto y no es lo que se enseña.

**Y lo que no se sabe derivar se dice.** `\frac{d}{dx}\left(x^{\frac{1}{2}}\right)`
sale con un error que explica el límite —hace falta `\sqrt`, que es E5— en vez de
devolver la pregunta sin responder.

### Los números duales: derivar evaluando

Evaluar `f` en `x = a + \varepsilon` con `\varepsilon^{2} = 0` da `f(a)` en la
parte real y **`f'(a)` en la dual**, exacto en ℚ y sin haber derivado: sale de
sumar y multiplicar. La regla del producto cae sola de que `\varepsilon^{2} = 0`.

Es el camino independiente que hace que el sello signifique algo, y `deny.toml`
lo impone: **`ms-verify` no ve `ms-diff`**. Una segunda implementación simbólica
se habría equivocado en los mismos sitios.

Sigue siendo **nivel 0**: coincidir en doce puntos no es demostrar.

### Lean sobre `Polynomial ℚ`, y dos teoremas que no se declaran

`Polynomial.derivative` es una derivada **formal**: no pasa por límites, igual
que el motor. Siete teoremas demostrados; sólo cinco reglas los declaran, porque
`derivative-power` y `derivative-chain` aceptan exponentes negativos, que se
salen del anillo de polinomios.

Es el error simétrico al de `rational-reciprocal` en E2 —allí el teorema era más
fuerte que la regla, aquí sería más débil— y la respuesta es la misma: sellar
menos.

### Dos cosas que eran de E2 y sólo se vieron aquí

**`like-terms`.** La regla del producto deja `x + \left(x + 1\right)`, que se lee
`x + x + 1`. Hasta ahora las sumas simbólicas salían de `ms-alg` ya agrupadas.
No expande: juntar `x\left(x+1\right)` con `x^{2}+x` sería expandir, y expandir
es una petición.

**El `1` de `Times(1, x)`.** `identity-element` exigía dos argumentos por culpa de
`Times(1, b^{-1})`, que se escribe `\frac{1}{b}`. La condición correcta no era
cuántos quedan sino **qué** queda. Cabo suelto de E1 cerrado por un camino que no
era el previsto.

### El motor da puntos, no gráficas

Un resultado con una variable libre trae en `facts` la rejilla de puntos donde
está definido, con las dos coordenadas **exactas** como cociente de enteros. Ni
ejes, ni rango de vista, ni color: eso es representación y vive en TypeScript.

Se ensamblan en `ms-wasm` y no en `ms-facts`, que sabe de números y no ve la
arena: meterlos allí abriría tres aristas nuevas en `deny.toml` para no ganar
nada.

---

## Lo que trajo E2

### La ecuación es una cabeza, no un envoltorio

`Head` creció dos variantes de golpe y el compilador enumeró los sitios donde
olvidarse habría dado una respuesta incorrecta.

**`Equal` no se pela en la frontera como `enBase`.** `enBase` se pela porque el
reescritor no debe verla nunca: no cambia el valor. Una ecuación **sí** es lo
que se reescribe —`(x-2)(x-3)=0` es un estado intermedio de la traza— y pelarla
dejaría los pasos sin sujeto.

**`Solutions` y no `List`**, que es lo que el plan decía. Las soluciones de una
ecuación son un **conjunto**, así que *orderless* es obviamente correcto y las
raíces salen siempre en el mismo orden. Un `List` genérico sin orden habría sido
la sorpresa; el día que haga falta algo ordenado entra como otra cabeza.

**El `=` no es un operador del Pratt.** Como operador tendría que asociar, y
`x = 2 = 3` construiría `Equal(Equal(x,2), 3)`: se escribe igual que lo tecleado
y significa otra cosa. Es una relación no asociativa y sólo en la raíz.

### Expandir y factorizar no pueden ser reglas automáticas

Son inversas. Una regla que expandiera todo producto de sumas y otra que
factorizara toda suma se devolverían el trabajo hasta agotar el presupuesto.

Entran como **funciones con nombre** —`\operatorname{expandir}`,
`\operatorname{factorizar}`— que es además lo que dice el árbol de capacidades,
donde «Expand a polynomial» es un ejemplo de consulta. Con la petición en la
cabeza no hay ciclo: la regla la consume.

**Dentro de una ecuación factorizar sí es automático**, y no hay con quién
ciclar porque expandir no se hace nunca por su cuenta.

### El polinomio es una vista, no un nodo

No hay `Node::Poly` y no la habrá. `ms-alg` lo construye desde
`Plus`/`Times`/`Power` y lo colapsa de vuelta, que es lo que
`as_fraction`/`make_fraction` ya hacían con los racionales.

`poly.rs` es **genérico en `C: Coefficient`** y no conoce `Integer`, `Rational`
ni la arena: es el código que E4 volverá a usar sobre 𝔽ₚ. `bridge.rs` es la
frontera, y es concreta a propósito.

**La factorización es concreta sobre ℚ, y no es un descuido**: el teorema de la
raíz racional es una afirmación *sobre ℚ*, y escribirlo genérico sería
generalizar un teorema que no generaliza.

### El nivel 1 existe, y afirma menos de lo que parece

Cada solución se sustituye en la ecuación original y los dos lados se evalúan en
ℚ. La sustitución **no construye nada** en la arena: se evalúa leyendo el símbolo
como si fuera su valor.

**Afirma que cada solución devuelta cumple la ecuación. No afirma que no falte
ninguna.** Comprobar la completitud exigiría dividir por los factores hallados y
buscar raíces en el cociente, que es el código que produjo la respuesta: sería un
eco. Sellar menos y que el sello signifique algo.

### Y el muestreo, que el documento pedía desde E0 y no existía

Con una variable libre no hay número que comparar, así que entrada y salida se
evalúan en **12 puntos racionales fijos**. Eso rescató cinco casos de E0 y E1 que
llevaban desde entonces marcados «sin verificar».

Dos correcciones a [`../architecture/03-lean-verification.md`](../architecture/03-lean-verification.md):
los puntos son **fijos**, no aleatorios —un sello irreproducible no sirve cuando
alguien discute un resultado— y son **racionales exactos**, no flotantes a 50
dígitos con aritmética de bolas: el evaluador ya vive en ℚ y no hay error que
acotar. Las bolas harán falta con los irracionales, en E5.

Sigue sin demostrar, y por eso es nivel 0.

### Lean está dentro y demuestra

`engine/lean/` compila contra Mathlib v4.32.1. Cinco reglas racionales de E1
enunciadas como teorema sobre ℚ y demostradas; el criterio de salida de E1 sale
con **cuatro de sus seis pasos** respaldados, y el nombre del teorema viaja hasta
el documento.

`#[soundness]` dejó de ser una cadena libre: es un enum con las tres formas que
el documento ya especificaba —`lean(…)`, `lean_local(…)`, `numeric_only`—.
`None` y `NumericOnly` son distintos a propósito: una dice que nadie lo miró, la
otra que se miró y se decidió que no.

`scripts/check-soundness.sh` cruza cada nombre con un `theorem` real, y **está
comprobado que falla ante una errata**.

**Y la demostración enseñó dos cosas que el código no decía:**

- **`rational-reduce` no necesita que el denominador sea no nulo.** Lean rechazó
  la hipótesis por no usarse.
- **`rational-reciprocal` es cierta sin ninguna hipótesis**, porque en Lean
  `0⁻¹ = 0` por convención. El motor **no puede apoyarse en eso**: para él
  `\frac{a}{0}` es un error, no cero, así que la guarda `is_known_nonzero` sigue
  haciendo falta aunque el teorema no la pida. Es el §2 del documento al revés —
  un teorema más fuerte que la regla no la respalda entera.

### Cinco defectos de lectura más, y ninguno lo encontró un test

Van **quince** en el proyecto, todos de la misma familia: el valor correcto y lo
que el estudiante lee, no.

| Se leía | Se lee |
|---|---|
| `6 + \left(-5\right)x + x^{2}` | `x^{2} - 5x + 6` — orden por grado y sin paréntesis en el coeficiente de cabeza |
| `\operatorname{factorizar}(x^{2}+1)` devolvía la pregunta | «No se puede factorizar más sobre los racionales» |
| `x^{2} - 1 \cdot 4 = 0` al pasar al otro lado | `x^{2} - 4 = 0` |
| `\left\{x = 2, x = 2\right\}` para una raíz doble | `\left\{x = 2\right\}` |
| `\left\{x = 2, x = 3\right\}` en crudo en `/motor` | renderizado con KaTeX |

Y uno que **no** era de lectura y sólo salió al mirar: `equation-zero-product`
no disparaba **nunca**. La comparación «ya estaba factorizada» iba sobre ids en
crudo, y el producto recién construido lleva los factores en el orden en que
salieron mientras el lado izquierdo los tiene canonicalizados. Decía «cambió»
sin cambiar nada, el reescritor veía el mismo estado y **paraba** — dejando la
ecuación factorizada y sin resolver, sin error y sin paso que lo explicara.

---

## Lo que trajeron E0 y E1

### Las condiciones laterales dejaron de ser algo que hay que recordar

`Rule::apply` recibe `&Assumptions` de forma **obligatoria**. Una regla no se
puede escribir sin tener a mano con qué validar su hipótesis, que es la forma
habitual en que un CAS da una respuesta incorrecta.

### La verificación existe y es exacta, no probabilística

`ms-verify` evalúa entrada y salida en ℚ y las compara. **No depende de
`ms-rules`** —`deny.toml` prohíbe esa arista— porque comprobar un resultado con el
código que lo produjo no comprueba nada.

`Verified<T>` sólo se construye desde un `Evidence`, y `Evidence` sólo desde
`check`: la invariante 3 pasó de convención de documento a error de compilación.

### `Head::Fn(Builtin)`, la decisión más cara del kernel

`Head` creció **una** vez con un enum cerrado dentro. Las alternativas eran una
variante por función —cientos en E6— o `Fn(SymId)`, que apaga la exhaustividad.
Con el enum cerrado, añadir un `Builtin` rompe el `match` del evaluador exacto,
el del escritor de LaTeX y el de la tabla de aridad, que son los tres sitios
donde olvidarse produce una respuesta incorrecta.

### Un entero no sale solo, y un número se puede escribir de varias formas

`facts` entró en el contrato con datos dentro: signo, paridad, primalidad,
factorización y divisores. Y con las representaciones: mixtas, decimales,
periódicos, porcentajes, valor posicional y bases, en los dos sentidos.

La primalidad se **deriva** de la factorización: dos caminos hasta la misma
respuesta son dos caminos que pueden discrepar.

### El motor no vive sólo en el laboratorio

`/motor` es el banco de pruebas, con el árbol de capacidades y cada caso
ejecutándose contra el WASM real. Y el editor de notas usa el motor: un bloque
`$$…$$` es un cuaderno donde cada línea se resuelve por su cuenta.

Detalle completo de E1 en [`E1-PLAN.md`](E1-PLAN.md).

---

## Métricas

| Métrica | Valor | Objetivo |
|---|---|---|
| Cobertura del corpus (de 958 capacidades reales) | **4,38 %** — 42 de 958, calculada por `ms-cli` y mirable en [`/motor`](../../src/pages/motor/index.astro) | — |
| Reglas con soundness demostrada en Lean | **10 de 30 — 33,3 %** | — |
| Resultados verificados | Nivel 0 exacto, nivel 0 por muestreo, **nivel 0 por derivación automática**, y nivel 1 por sustitución | — |
| Tamaño del WASM comprimido | **97,0 KB** brotli · 297 KB en crudo (92,2 KB al abrir E3) | **< 3 MB** en E0–E3 |
| Tests | **295** en verde | — |
| Casos del corpus en el navegador | **112 de 112** (E0 + E1 + E2 + E3) | — |
| Reglas declaradas | **30** (21 al abrir E3) | — |
| Línea base del matcher | **6,99 µs** el criterio de salida de E1 con 30 reglas (6,84 µs con 21, misma sesión) | — |
| Coste por regla del barrido lineal | **~32,5 ns**, medido con 3, 6, 9, 12, 16 y 30 reglas en la misma sesión | — |

**La red de discriminación sigue sin justificarse, y se ha vuelto a mirar con el
rule set de E3 delante, que es lo que E2 dejó dicho:** 793 ns con 3 reglas y
1,67 µs con 30, o sea ~32,5 ns por regla — la misma pendiente que con 21, así que
el barrido crece lineal y sin sorpresas. Nueve reglas más costaron **147 ns** en
el criterio de salida de E1 (6,84 → 6,99 µs). Contra los 30 ms que tarda el WASM
en arrancar, el barrido lineal aguanta de sobra. Se vuelve a mirar en E4, cuando
Rubi traiga reglas por centenares — que es donde el cálculo cambia de orden.

> **Las cifras de dos sesiones distintas no se restan.** Para eso está el grupo
> `sweep` del bench: barre una expresión ya en forma normal con distintos tamaños
> de rule set, y todas las medidas salen del mismo minuto.
>
> El recuento de reglas sale de contar los `#[id = …]` de `set.rs`, no de la
> memoria: una vez este documento decía 9 donde había 10, y 239 tests donde
> había 244.

---

## Lo siguiente, concreto

**La fase 7 de E3: el bloque multilínea con definiciones.** Es lo único de esta
etapa que falta, tiene criterio propio y el diseño está escrito en
[`E3-PLAN.md`](E3-PLAN.md) §Fase 7: `:=` explícito para definir —adivinar si
`a = 2` es definición o ecuación falla justo en el caso interesante—,
`Head::UserFn(SymId)` para `f(x) := …`, `bindings` entrando por fin al contrato,
`solve_block` en la frontera y el frontend mandando el bloque entero en vez de la
línea suelta.

Su criterio: en el editor de notas,
`f(x) := \left(x^{2}+1\right)^{3} \\ \frac{d}{dx}f(x)` devuelve la derivada con
sus pasos.

Después, **E4 — Integración**
([`../architecture/04-roadmap.md`](../architecture/04-roadmap.md) §E4), que hereda
tres cosas de E3 **por decisión, no por olvido**:

- **Derivar tendrá inversa.** Integrar es la otra mitad del par, y la pregunta
  del ciclo que E2 resolvió para expandir/factorizar vuelve tal cual — con la
  respuesta ya tomada: las dos son peticiones con nombre y ninguna se dispara
  sola.
- **La red de discriminación**, con ~32,5 ns por regla medidos sobre 30 reglas.
  Rubi son reglas por centenares: es ahí donde el cálculo cambia de orden, no
  aquí.
- **`\sqrt` y con él los irracionales**, que E2 dejó a E3 y E3 vuelve a dejar:
  la torre algebraica sigue siendo E5, y `\frac{d}{dx}\left(\sqrt{x}\right)` sale
  con el error que lo dice.

Antes de empezar, **vuelve a medir la línea base del bench**: las cifras de este
documento salieron de esta sesión y de esta máquina.

### Lo que dejó de faltar

**El `1` de `Times(1, b^{-1})`** dejó de bloquear el caso general: `1x` ya se
escribe `x`, y el `1` sólo se conserva cuando lo que queda solo es una potencia,
que es el único caso que el escritor no sabe poner en un `\frac`. Era un cabo
suelto de E1 y lo cerró la regla del producto de E3, no la forma de salida que se
estaba esperando.

Sigue abierto el `\frac` con varios factores en el denominador, que esperaba lo
mismo.

---

## Cabos sueltos conocidos

| Qué | Dónde | Acción |
|---|---|---|
| **Los pasos de una suma salen en orden canónico, no de lectura.** En el criterio de salida de E3, el `+1` se deriva antes que el `x^{2}` aunque salga escrito después | `crates/ms-rules/src/rewrite.rs` | Ninguna por ahora. Arreglarlo exigiría que el reescritor conociera el orden del escritor, que es presentación, y esa frontera se mantiene separada a propósito |
| **Una derivada de orden superior sale sin verificar.** `\frac{d}{dx}\left(\frac{d}{dx}\left(x^{3}\right)\right)` da `6x`, que es correcto, y el sello dice «sin verificar» | `crates/ms-verify/src/dual.rs` | Los duales de primer orden llevan **una** derivada, así que la de dentro no tiene con qué comprobarse. Verificarla exige jets de orden 2, que es aritmética distinta y no un parámetro |
| **`derivative-power` y `derivative-chain` aceptan exponentes negativos que Lean no respalda.** Los teoremas existen y cubren el exponente natural | `engine/lean/MathSlice/Calculus.lean` | Las reglas **no** declaran `lean_local`, a propósito. Cerrarlo exige salir de `Polynomial ℚ` — funciones racionales o `deriv` sobre ℝ— y decidir sobre qué objeto se enuncia |
| **Los hechos graficables son sólo puntos**: no hay ceros ni puntos críticos | `crates/ms-wasm/src/graph.rs` | Los ceros exigen `ms-alg` y los críticos, `ms-solve` y derivar otra vez. Se puede hacer desde la frontera sin abrir aristas nuevas; nadie lo ha pedido todavía |
| **La rejilla de muestreo es fija**: de −10 a 10 con paso `\frac{1}{2}` | `crates/ms-wasm/src/graph.rs` | Ninguna por ahora. Elegir el rango por la expresión es una decisión de vista, y la vista es de la capa que dibuja |
| **La diferenciación implícita no existe**: en `\frac{d}{dx}\left(y\right)`, la `y` es una constante | `crates/ms-diff/src/lib.rs` | Exige poder escribir `y(x)`, que es `Head::UserFn` — la fase 7 de E3 |
| **La factorización sólo saca factores lineales con raíz racional.** `x^{4}+1` sale sin factorizar aunque sobre ℚ se pueda | `crates/ms-alg/src/factor.rs` | Cantor–Zassenhaus con levantamiento de Hensel. Se valoró y se dejó fuera: ~400 LOC del algoritmo más delicado del crate para un caso que un estudiante no escribe. Los traits de `ms-num` ya están puestos para ello |
| **Una ecuación con raíces irracionales sale con un error**, no con `\pm\sqrt{2}`. No dice «no tiene solución», que sería falso | `crates/ms-rules/src/set.rs`, regla `equation-no-rational-solutions` | Exige `\sqrt` en el parser y en el escritor, y la torre algebraica detrás. E5 |
| **La multiplicidad de una raíz se pierde.** `(x-2)^{2}=0` sale `\left\{x=2\right\}`, que es el conjunto correcto | `crates/ms-solve/src/lib.rs` | Es información sobre la ecuación, no una solución: su sitio es un hecho, cuando E3 traiga los hechos sobre expresiones |
| **El nivel 1 no comprueba que las soluciones estén todas**, sólo que las devueltas cumplan | `crates/ms-verify/src/lib.rs` | Ninguna. Comprobarlo exigiría el código que las encontró, y la comprobación sería un eco |
| **Un paso intermedio puede enseñar constantes sin plegar**: `3x - 7 + 1 = 0`. La regla silenciosa las junta a la iteración siguiente, pero el paso ya se escribió | `crates/ms-rules/src/rewrite.rs` | Ninguna por ahora. Plegar antes de emitir el paso significaría que un paso hace dos cosas |
| **`set.rs` pasa de 900 LOC**, contra la guía de archivos de responsabilidad única | `crates/ms-rules/src/set.rs` | El orden de las reglas **es** semántico y repartirlo en varios `rules!` lo haría invisible. Si crece más, los cuerpos se mueven a los módulos temáticos y aquí quedan las firmas |
| El `grep` de la invariante 4 no busca `color` ni `render` | `scripts/check-boundary.sh` | Ninguna. Un test que falla en falso se acaba desactivando |
| Los `allow-*-in-tests` de `clippy.toml` no alcanzan a los tests de integración ni a los benches | `crates/*/tests/`, `crates/ms-rules/benches/` | Ninguna. Es cómo funciona clippy |
| **`bindings` sigue fuera del contrato**, anotado como diferido junto a `alternates` | `crates/ms-serialize/src/result.rs` | Entra con el DSL de patrones, que E2 tampoco necesitó |
| **Una fracción no se escribe en otra base** | `crates/ms-facts/src/lib.rs` | Exige el periódico generalizado a la base pedida. Nadie lo ha pedido |
| **El desarrollo posicional se lee de menor a mayor** | `crates/ms-core/src/order.rs` | Ninguna. El escritor ya ordena las sumas por grado; el desarrollo posicional es un hecho, no una suma, y va por otro camino |
| **Un decimal periódico no tiene paso con título propio** | `crates/ms-parse/src/parser.rs` | Esperaba a `ms-simplify`, que E2 recortó. Vuelve a quedar abierto |
| **El redondeo lo comparten la regla y el verificador** vía `ms-num` | `crates/ms-num/src/rounding.rs` | Ninguna. Es el reparto de `checked_pow` |
| **Por encima de 2⁶⁴ no hay hechos de divisibilidad** | `crates/ms-num/src/integers.rs` | Subirlo exige aritmética modular sobre bignum. Ninguna prisa |
| **La división por prueba no llega a la raíz de todo el rango** | `crates/ms-num/src/integers.rs` | Pollard rho lo cerraría. Se valoró y se dejó fuera |
| **`\frac` sólo se emite con un factor en el denominador** | `crates/ms-serialize/src/latex.rs` | Esperaba la forma de salida de `ms-simplify`, recortada en E2. Sigue abierto, y sigue siendo fidelidad de round-trip |
| Los símbolos son de una sola letra | `crates/ms-parse/src/lexer.rs` | `xy` es el producto de x por y, como en clase |
| **41 casos del corpus no declaran capacidad**, así que no cuentan para la cobertura | `corpus/e0.json`, `e1.json`, `e2.json` | Etiquetar los que sí respondan a una capacidad. La métrica es conservadora a propósito |
| **Un caso del corpus declara una sola capacidad** | `crates/ms-cli/src/coverage.rs` | Ninguna. Un array haría la métrica más fácil de inflar |
| El DSL de patrones de `rules!` y la red de discriminación | `crates/ms-rules-macro/` | Con 30 reglas y ~32,5 ns por regla sigue siendo adorno, y la pendiente no ha cambiado al pasar de 21 a 30. Se decide en E4, con Rubi delante |

Roadmap completo: [`../architecture/04-roadmap.md`](../architecture/04-roadmap.md).
Capacidades → [`../docs/main.md`](../docs/main.md). Contenido al que servirá →
[`../../media/maths/tree.md`](../../media/maths/tree.md).

---

## Cómo actualizar este archivo

Al cerrar una etapa: cambiar la etapa actual, las métricas, y "lo siguiente, concreto". **No copiar aquí contenido de los documentos de arquitectura** — solo apuntar. El contenido vive una sola vez.
