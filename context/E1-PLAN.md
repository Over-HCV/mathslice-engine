# E1 — Aritmética y números · plan de ejecución

> **⚠️ E1 está cerrada. Este archivo ya no es un traspaso: es el registro de cómo
> se ejecutó.** Las cinco fases están hechas, incluida la base de salida que la 4
> dejó pendiente. Lo que se decidió en cada una está resumido en
> [`STATUS.md`](STATUS.md); lo que queda aquí es el detalle de la ejecución, útil
> para entender por qué algo se hizo así y no para saber qué hacer ahora.
>
> **Qué hacer ahora → [`STATUS.md`](STATUS.md) §Lo siguiente**, que apunta a E2 en
> [`../architecture/04-roadmap.md`](../architecture/04-roadmap.md).
>
> Se conserva en vez de borrarse porque la mitad del valor de un plan está en las
> cinco cosas que salieron distintas de lo planeado, y ésas están escritas abajo.

---

## Lo que ya no hay que hacer

Las fases 1, 2, 3 y 4 están completas —la 4 menos la base de salida—. Lo que dicen
sus secciones más abajo se conserva como registro de por qué se hizo así, no como
trabajo pendiente.

De la **fase 4**, cinco cosas salieron distintas de lo planeado:

- **`Head::Fn(Builtin)`, no `Fn(SymId)`.** Un enum cerrado dentro de otro: `Head`
  crece una vez y la exhaustividad no se apaga. La decisión que el roadmap
  arrastraba a E2 se tomó aquí porque el redondeo la necesitaba.
- **El redondeo lo comparten la regla y el verificador** vía `ms-num`, al
  contrario de lo que decía el plan. El criterio de desempate es una convención,
  no un hecho matemático, y dos copias discreparían en el `.5`.
- **Las mixtas no se aceptan de entrada** — decisión del usuario. Sólo son una
  vista del resultado, y `2\frac{1}{3}` sigue siendo el producto que la notación
  dice que es.
- **`Fact::Decimal` no estaba planeada** y hacía falta: sin ella, redondear a dos
  cifras contestaba `\frac{173}{50}` y el `3.46` sólo salía como aproximación.
  La destapó abrir la página, no un test.
- **La base no acabó viviendo en `ms-assume`.** El desarrollo posicional no
  necesita estado ninguno, y la base de *salida* resultó ser otra decisión.

Y cuatro de las fases 1–3 que conviene saber antes de tocar nada:

- **`\div` no existía en el lexer.** El criterio de salida lo usaba. Se añadió como
  `Slash`: el árbol no distingue `\div` de `/`, porque el IR no tiene división.
- **La regla de la división no tiene la forma que decía la tabla.** El parser ya
  desazucara `÷`, así que no hay ningún `\frac{a}{b} \div \frac{c}{d}` que
  reescribir; lo que queda en el árbol es `Power(Times(c, Power(d,-1)), -1)`, el
  recíproco de un cociente. La regla se llama `rational-reciprocal`.
- **`facts` no se añadió en la tarea 1.3**, aunque la decisión de adelantarlo a E1
  estaba tomada. Hasta la fase 3 sería un array siempre vacío, que es exactamente
  lo que `result.rs` prohíbe. Entró en la 3.3, con datos dentro, y con las clases
  `sign`, `parity`, `primality`, `factorization`, `divisors`. El discriminante en
  el cable es `class`, no el `kind` del boceto de `02-architecture.md`: `kind` ya
  significa otras dos cosas en el mismo JSON. El boceto se corrigió.
- **La fase 3 no hizo `\gcd(a, b)` consultable**, y no por falta de tiempo. Ver el
  recuadro de la fase 3. Da igual lo obvia que parezca: no la añadas sin hablarlo.

---

## Antes de tocar nada, lee

1. [`../../context/README.md`](../../context/README.md) — enruta el repositorio.
2. [`STATUS.md`](STATUS.md) — el estado vivo. Empieza aquí de verdad.
3. [`../architecture/README.md`](../architecture/README.md) y el orden 00→05 que indica.
4. [`../architecture/01-rust-principles.md`](../architecture/01-rust-principles.md) —
   11 principios con consecuencias obligatorias. No son recomendaciones: si un
   cambio las viola, el cambio está mal.

El repositorio está en español y dirigido por documentación. El contenido vive una
sola vez y los documentos se enrutan entre sí; antes de añadir información,
averigua dónde le toca vivir. Los títulos de las reglas se escriben en español: son
lo que lee el estudiante.

---

## Contexto

El motor es una rebanada vertical viva: entra LaTeX en el navegador y sale el valor
exacto, su aproximación decimal y la traza de pasos. E1 tiene que mover las dos
cosas que E0 dejó planas:

- **La cobertura del corpus es la métrica del proyecto**
  ([`../architecture/05-testing.md`](../architecture/05-testing.md) §1).
- **Nada está verificado.** Todo sale `unverified` nivel 0. `ms-verify` nivel 0 es
  lo que empieza a cambiarlo, y la invariante 3 deja de ser una convención.

### Lo que ya está hecho (fase 0, commit `89211b2`)

Abrir la página del motor en un navegador real —el hueco que E0 dejó anotado— destapó
tres defectos, ya arreglados:

| Entraba | Salía | Sale ahora |
|---|---|---|
| `\frac{2}{1-1}` | `\frac{2}{0}`, sin error | error `division-by-zero` |
| `\frac{3}{4}-\frac{5}{6}` | `\frac{\left(-1\right)}{12}` | `\frac{-1}{12}` |
| `\frac{1}{2}\cdot\frac{2}{3}` | `\frac{1}{3}` sin paso que lo explique | igual, fijado por corpus hasta la tarea 2.1 |

Y existe `pnpm engine:browser`: el corpus contra el worker y el `.wasm` reales en
Chrome headless. Falla si un caso cambia, si la isla no hidrata o si la consola
tiene un error. Comprobado que falla cuando debe.

**Estado medido hoy:** 75 tests en verde · 22 casos de corpus (4 con deuda) · 53 KB
de WASM brotli contra un presupuesto de 3 MB · 5,0 µs la línea base del matcher ·
rama `MS1-Engine-E0`, sin push.

### Decisiones ya tomadas — no las reabras solo

- **Criterio de salida de E1** (abajo). Confirmado con el usuario.
- **Alcance de E1**, recortado respecto al roadmap. Confirmado y ya escrito en
  [`STATUS.md`](STATUS.md) y [`../architecture/04-roadmap.md`](../architecture/04-roadmap.md).
- **`Rule::apply` cambia de firma** en la fase 1 para recibir `&Assumptions`. Confirmado.
- **Rama nueva para E1**, a partir de `MS1-Engine-E0`. Sugerido: `MS1-Engine-E1`.

---

## Criterio de salida

> Entra `\frac{7}{12}\div\frac{14}{9}+2^{40}` desde el navegador y sale
> `\frac{8796093022211}{8}` exacto, `≈ 1.0995116e12`, con su traza de seis pasos y
> el documento marcado **`verificado · nivel 0`**.

1. Dividir es multiplicar por el recíproco
2. Multiplicamos las fracciones
3. Simplificamos la fracción
4. Elevamos a la potencia
5. Buscamos denominador común
6. Sumamos los numeradores

Fuerza a existir todo lo que E1 aporta y hoy no está: reglas de `×` y `÷` con paso
propio, potencia entera sobre bignum, suma de entero con fracción, y `ms-verify`
devolviendo `verified`.

**Alcance de la etapa: el núcleo aritmético.** Irracionales, algebraicos,
trascendentes y complejos son E5, no E1 — su aritmética depende de la torre
algebraica. La tabla completa está en [`STATUS.md`](STATUS.md).

---

## Fase 1 — los dos crates nuevos ✅

> **Lints, perfiles y entradas de `deny.toml` en el primer commit de cada crate.**
> Añadirlos después significa arreglar cientos de sitios de golpe
> ([`../architecture/05-testing.md`](../architecture/05-testing.md) §8).
> Un crate nuevo que dependa de `ms-core` rompe `cargo deny check` hasta que lo
> añadas a sus wrappers. No es un fallo: es la regla de dependencias funcionando.

Aristas que hay que declarar en `engine/deny.toml` y en `[workspace.dependencies]`
de `engine/Cargo.toml`:

```
ms-core     wrappers += ms-assume, ms-verify
ms-num      wrappers += ms-verify
ms-assume   wrappers  = ms-rules, ms-wasm      (entrada nueva)
ms-verify   wrappers  = ms-wasm               (entrada nueva)
```

### 1.1 — `ms-assume`, y las reglas dejan de poder dispararse sin él

Contexto de suposiciones tipado: dominio del símbolo (`Integer | Rational | Real`),
signo, y la base numérica que necesitará la tarea 4.4.

Lo que lo hace real y no decorado es el §3: `Rule::apply` pasa de
`fn(&mut Arena, ExprId) -> RuleResult` a
`fn(&mut Arena, ExprId, &Assumptions) -> RuleResult`.
Hoy son 5 reglas y el cambio es mecánico; en E4 serían ~6700.

Toca, en cadena: `crates/ms-rules/src/rule.rs` (el campo `apply`), `rewrite.rs`
(`rewrite` y `apply_once` lo enhebran), `rational.rs` (las 5 reglas),
`crates/ms-rules-macro/src/lib.rs` sólo si el macro asume la aridad,
`ms-wasm/src/lib.rs`, `benches/matcher.rs` y los tests. El macro genera
`apply: #name`, así que no debería enterarse.

**Terminado cuando:** ninguna regla compila sin recibir el contexto, y el bench
sigue midiendo lo mismo.

### 1.2 — `ms-verify` nivel 0 y `Verified<T>`

`Verified<T>(T, Evidence)` como newtype de coste cero. `ms-wasm` construye el
`ResultDoc` **sólo** a partir de un `Verified<ResultValue>`: así la invariante 3
pasa de convención a error de compilación.

Necesita un evaluador exacto `Expr → Option<Rational>` sobre expresiones cerradas.
Vive en `ms-verify`, no en `ms-num`: `ms-num` no depende de nada del workspace y
por tanto no conoce `ExprId`. Se parecerá a `rational_value` de
`crates/ms-rules/src/fraction.rs`, y eso está bien — son contextos distintos y el
principio 12 lo permite; lo que no vale es que `ms-verify` dependa de `ms-rules`.

> **Matiz que hay que corregir en la documentación.**
> [`../architecture/03-lean-verification.md`](../architecture/03-lean-verification.md)
> describe el nivel 0 como «muestreo numérico… probabilístico». En E1 no lo es: sin
> variables libres, verificar un paso es evaluarlo exactamente en ℚ — determinista
> y total. El muestreo es lo que aplica cuando entren símbolos en E2. Matízalo ahí
> en vez de reportar como probabilístico algo que es exacto.

### 1.3 — El cambio de contrato, todo de una vez

Tres cosas cambian la forma del `ResultDoc`, y hacerlas juntas cuesta **un** cambio
de contrato en vez de tres:

- `Verification` gana `method` — está en
  [`../architecture/02-architecture.md`](../architecture/02-architecture.md) §3 y no
  en el código.
- `TraceStep` gana `path`: los índices de argumento desde la raíz hasta donde
  disparó la regla. Hoy `before`/`after` son fotos del árbol entero; con derivadas
  en E3 el paso es «derivamos *este* subtérmino» y sin locus no se puede señalar.
- El nivel de verificación real sustituye al `unverified` fijo.

Toca a la vez: `crates/ms-serialize/src/result.rs`, su espejo
`src/lib/engine/types.ts`, `verificationLabel` de
`src/lib/engine/represent/toSteps.ts`, el test de forma de
`crates/ms-wasm/tests/corpus.rs` y `engine/scripts/browser-check.mjs` si comparas
campos nuevos.

---

## Fase 2 — aritmética racional completa · aquí cayó el criterio de salida ✅

Todas las reglas por `rules!`, con id estable, título en español y plantilla de
paso. **El orden de declaración es el orden de intento**, y las reglas nuevas van
antes de `integer-fold-*`, o el fold silencioso vuelve a robarles el trabajo — que
es exactamente el defecto que la tarea 0.3 dejó fijado por corpus.

| | Tarea | Cierra la deuda |
|---|---|---|
| 2.1 | `\frac{a}{b}\cdot\frac{c}{d}` con «Multiplicamos las fracciones» y cancelación cruzada | `multiplicacion-de-fracciones-sin-paso-propio` |
| 2.2 | `\frac{a}{b}\div\frac{c}{d}` con «Dividir es multiplicar por el recíproco». Necesita `c ≠ 0` del contexto de 1.1 | — |
| 2.3 | Suma parcial: `\frac{1}{2}+x+\frac{1}{3}` combina los racionales y deja la `x` en paz | `suma-parcial-de-racionales-que-no-se-combina` |
| 2.4 | Potencia entera sobre bignum, con tope de magnitud (ver trampas) | `potencia-entera-sin-calcular` |
| 2.5 | Cero absorbente y neutros: `2\cdot 0\cdot x → 0` | `cero-que-no-absorbe-el-producto` |

**Cerrar una deuda** significa quitarle el campo `deuda` al caso y darle sus pasos.
Si al terminar la fase 2 queda algún `deuda` vivo en `e0.json`, la fase no está
terminada.

Cada capacidad llega con las tres cosas del §10: su caso en `engine/corpus/e1.json`,
al menos una propiedad de `proptest` (conmutatividad, asociatividad,
`a/b · b/a = 1`, `(a^m)^n = a^{mn}`), y un bench si toca el matcher.

---

## Fase 3 — divisibilidad y propiedades de los enteros ✅

- **3.1** ~~MCD y mcm consultables~~ → **aplazado a E2**, decidido con el usuario.
  Ver el recuadro de abajo. `ms_num::gcd` y `lcm` siguen donde estaban, usados por
  `rational-reduce` y `rational-common-denominator`.
- **3.2** ✅ Factorización prima con tope explícito, en `ms-num`: división por
  prueba en `u64` hasta 10⁶ sobre rueda de 2·3·5, y Miller–Rabin **determinista**
  para el residuo. Magnitud máxima 2⁶⁴. Cuando el presupuesto no alcanza, sale
  una factorización parcial con su resto a la vista — no un error, no una mentira.
- **3.3** ✅ Propiedades de un entero, como el campo `facts` del contrato:
  `sign`, `parity`, `primality`, `factorization`, `divisors`. Crate `ms-facts`
  (dep: `ms-num`), espejo `FactDoc` en `ms-serialize`, traducción en `ms-wasm`, y
  `FactList.tsx` en el laboratorio.

> **Por qué no hay `\gcd(a, b)`.** Una consulta con nombre obliga antes a decidir
> cómo vive una función en el IR: variantes concretas en `Head` —que el kernel
> enumera para siempre, y en E6 son cientos— o `Head::Fn(SymId)`, que deja de
> crecer y a cambio apaga los `match` exhaustivos que hoy protegen. Es la decisión
> más cara del kernel a cambio de azúcar sintáctico, y la 3.3 ya da MCD y
> factorización por la vía de los hechos: se factorizan los dos números y se leen.
> Se toma en **E2**, donde el DSL de patrones de `rules!` y `ms-simplify` la
> fuerzan con más información encima de la mesa.
>
> La otra decisión de esta fase —adelantar `facts` de E3 a E1— sí se tomó y está
> escrita: el campo entró con datos dentro y con las cinco clases de arriba.

---

## Fase 4 — representaciones numéricas ✅ (menos la base de salida)

**Es la primera fase que añade reglas desde la 2**, así que vuelve a aplicar lo de
allí: el orden de declaración en `rules!` **es** el orden de intento, y una regla
nueva va antes de `integer-fold-*` o el fold silencioso le roba el paso — el mismo
defecto que la tarea 0.3 dejó fijado por corpus.

Cada capacidad llega con las tres cosas del §10: su caso en `engine/corpus/e1.json`,
al menos una propiedad de `proptest`, y un bench —esta vez sí, porque son las
primeras reglas nuevas desde que se midió la línea base. **Vuelve a medir la línea
base antes de comparar**: la fase 3 midió 5,25 µs con las mismas 9 reglas contra
los 5,7 µs anotados, y esa diferencia es la máquina, no el código.

Lo que la fase 3 deja puesto y no hay que construir: `Domain`/`Sign`/
`SymbolAssumption` en `ms-assume` (4.4 añade la base, no el mecanismo), el campo
`facts` ya en el contrato y admitiendo clases nuevas sin cambiarlo, y
`ms_num::factorize`/`divisors`, que es lo que 4.5 necesita para el periodo de
`1/n` — depende de `n` sin sus factores 2 y 5.

- **4.1** Mixtas ↔ impropias, con pasos.
- **4.2** Porcentajes: de un número, de una cantidad, aumento y descuento
  sucesivos, fracción ↔ porcentaje.
- **4.3** Redondeo y valor posicional, incluidos los métodos de redondeo del medio.
- **4.4** Bases numéricas: `ms-parse` aprende el subíndice (`1011_{2}`), conversión
  con pasos, aritmética en base, comparación entre bases. La base vive en
  `ms-assume`, no en el nodo — por eso 1.1 va primero.
- **4.5** Decimales periódicos ↔ fracción exacta, con la derivación clásica.

---

## Fase 5 — cierre de etapa · hecha

Lo que salió distinto de lo escrito abajo, y por qué:

- **5.2 no se pudo hacer como estaba planteado.** «~30 reglas contra la línea base
  de 5,0 µs con 5» compara medidas de sesiones distintas, y la deriva de máquina
  entre sesiones es del mismo orden que el coste de una regla. Se midió la
  pendiente **dentro de una sesión**, con 3, 6, 9 y 12 reglas sobre una expresión
  ya en forma normal: ~39 ns por regla.
- **5.1 desmintió su propia premisa.** El presupuesto no lo gasta la anchura: 30
  fracciones sueltas son 3 iteraciones porque las reglas son n-arias. Lo que
  cuesta es la profundidad, y ahí son ~4 por término.
- **5.5 confirmó los números escritos a mano** en `docs/main.md` —1709, 751, 958—
  y desmintió la cobertura: 3,34 %, no «~1 %».
- **La base de salida entró aquí**, después de la fase 5 y no antes, para que el
  bench y `twiggy` de cierre no se midieran sobre código recién puesto.
- **La comprobación visual de 5.4 quedó pendiente**: el corpus pasa en el
  navegador (87 de 87) pero la extensión de Chrome no estaba conectada para mirar
  `/motor` con los ojos. Es la comprobación que ha encontrado seis defectos
  en esta etapa y ningún test.

El plan original, tal y como se escribió:

- **5.1** PEMDAS sobre expresión compuesta, con el presupuesto y la detección de
  ciclos comprobados sobre el rule set completo.
- **5.2** Bench del matcher con ~30 reglas contra la línea base de 5,0 µs con 5. Es
  el punto en el que se decide si la red de discriminación sigue esperando a E2.
- **5.3** `twiggy` contra el presupuesto de 3 MB con los **tres** crates nuevos de
  la etapa (`ms-assume`, `ms-verify`, `ms-facts`). Al cerrar la fase 3 iba en 64 KB
  brotli, así que el presupuesto no es el riesgo; medirlo sigue siéndolo.
- **5.4** `pnpm engine:browser` en verde sobre `e0.json` + `e1.json`, y el criterio
  de salida comprobado a mano en el navegador.
- **5.5** `ms-cli` mínimo: la métrica del proyecto deja de ser una estimación. El
  «~1 %» de STATUS.md está puesto a mano, mientras
  [`../architecture/05-testing.md`](../architecture/05-testing.md) §1 dice que
  `ms-cli` extrae los casos de `docs/` y calcula la cobertura. Una métrica que se
  escribe a ojo no es una métrica.
- **5.6** Documentos: `STATUS.md`, `04-roadmap.md`, `05-testing.md`,
  `03-lean-verification.md` (el matiz del nivel 0), y los cabos sueltos que se
  cierren.

---

## Lo que te va a morder si no lo sabes

Está en el código y en [`STATUS.md`](STATUS.md), pero cuesta menos leerlo aquí.

- **La reescritura es pre-orden, y no es indiferente.** De abajo arriba,
  «simplificar» devuelve el `4/6` recién construido a `2/3`, deshace el denominador
  común y cicla. Si cambias la estrategia de recorrido, ese es el caso que tienes
  que probar.
- **El IR no tiene cabeza `Divide` ni `Subtract`.** `a/b` es
  `Times(a, Power(b,-1))` y `a-b` es `Plus(a,-b)`. Menos cabezas, menos casos que
  enumerar durante años. No añadas cabezas sin una razón fuerte.
- **`Node::Rat` sólo existe reducido y con denominador ≠ 1.** Las fracciones sin
  reducir de los pasos intermedios son estructurales.
- **El presupuesto es de iteraciones, no de magnitud.** `DEFAULT_BUDGET = 512` acota
  cuántas reescrituras, no lo grandes que se ponen los números. En cuanto exista la
  regla de potencia, `2^{10^{9}}` no ciclará: calculará, y un bignum de mil millones
  de bits mata la pestaña sin gastar una iteración. Tope de dígitos y `MathError`
  propio, decidido **dentro** de la tarea 2.4, no después.
- **No ices el signo fuera de una fracción.** Se intentó en la fase 0 y rompe
  `parse(latex(e)) == e`: `-\frac{1}{12}` vuelve a entrar como
  `Times(-1, 1, Power(12,-1))`, con un `1` sobrante que aporta `\frac{1}{12}`.
  Izarlo exige antes que la canonicalización elimine los elementos neutros, que es
  para lo que existe `Head::identity_element` y todavía nadie usa.
- **La comprobación de división por cero corre en cada estado del reescritor.**
  Cuesta un 4,5 % medido. Memoizarla por `ExprId` sería correcto —la arena es
  append-only— y se probó y salió peor a este tamaño. Vuelve a intentarlo sólo
  cuando el bench diga que las expresiones ya son grandes.
- **`compare` ya falla en el brazo imposible** (`crates/ms-core/src/order.rs:53`
  devuelve `CoreError::IncomparableNodes`). Era una línea y está hecha: si una
  variante nueva de `Node` colisionara de rango, dos expresiones distintas
  compararían iguales y el hash-consing fundiría matemáticas diferentes sin crash
  y sin test que falle. No lo vuelvas a «arreglar».
- **El corpus en verde no dice que la página se lea bien.** Van cinco defectos de
  esta clase —el valor correcto y lo que el estudiante lee, no— y ninguno lo
  encontró un test: `12^{-1}14^{-1}`, `\frac{14}{9}^{-1}` sin agrupar, y en la
  fase 3 una factorización sin factores que escribía `n = n` y debajo «n se quedó
  sin factorizar». `pnpm engine:browser` comprueba **datos**; la lectura hay que
  mirarla. Abre `/motor` antes de dar una capacidad por terminada.
- **`deny.toml` lleva listas explícitas** de quién puede depender de cada crate.
- **Los `allow-*-in-tests` de `clippy.toml` no alcanzan** a los tests de integración
  ni a los benches. Son crates aparte y llevan su propio `#![allow]`.
- **`wasm-opt` necesita `--enable-bulk-memory`** y compañía, ya configurado en
  `crates/ms-wasm/Cargo.toml`. Si lo quitas, el build falla en validación.
- **El escritor de LaTeX sólo emite `\frac` con un factor en el denominador.** Es
  fidelidad de round-trip, no descuido; está comentado en el sitio.

---

## Cómo compruebas que no has roto nada

```sh
pnpm engine:test     # cargo nextest sobre el workspace
pnpm engine:gates    # clippy + cargo-deny + test de frontera
pnpm engine:build    # compila a WASM  (hazlo ANTES de engine:browser)
pnpm engine:size     # twiggy contra el presupuesto de 3 MB
pnpm engine:bench    # matcher, contra la línea base de 5,0 µs
pnpm engine:browser  # el corpus contra el worker y el WASM reales (necesita pnpm dev)
pnpm check           # astro check
```

> `engine:browser` usa el `.wasm` de `src/lib/engine/pkg/`, que **no se regenera
> solo**. Si no corres `engine:build` antes, estarás probando el motor de ayer. Y si
> el servidor de desarrollo lleva horas levantado, una caché rancia de Vite puede
> impedir que la isla hidrate: reinícialo antes de creerte un fallo de hidratación.

Y la prueba manual de
[`../architecture/05-testing.md`](../architecture/05-testing.md) §10: `pnpm dev`,
abrir `/motor`, ejecutar un caso y comprobar valor exacto,
aproximación, los seis pasos con sus títulos en español, y que el pie dice
**Verificado (nivel 0)** y no **Sin verificar**.

**Definición de terminado de E1:** el criterio de salida en verde en el navegador ·
ningún campo `deuda` vivo en el corpus · un caso y una propiedad por capacidad · el
bench medido contra la línea base · `twiggy` bajo presupuesto · y la cobertura de
STATUS.md calculada por `ms-cli`, no estimada.

---

## Cómo se trabaja aquí

- `.claude/CLAUDE.md` ya carga los principios de organización, comunicación y
  herramientas de `context/core/`. Aplícalos.
- **Planifica antes de tocar código y confirma el plan.** Este motor es un proyecto
  plurianual: la arquitectura existe para que apilar capacidades nunca exija
  refactorizar el kernel, y las decisiones de cimientos se toman despacio.
- **Cuando algo no encaje con lo que dice un documento, dilo.** Los documentos se
  actualizan; no se ignoran en silencio. Hay cabos sueltos ya anotados en
  [`STATUS.md`](STATUS.md).
- Trabaja en una rama nueva para E1 a partir de `MS1-Engine-E0`. **No hagas push sin
  pedirlo.**
- `.astro/dev.json` aparece modificado: es estado del servidor de desarrollo. **No
  lo commitees.**
