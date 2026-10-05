# E3 — Cálculo diferencial · plan de ejecución

> **Traspaso vivo de la etapa.** El criterio de salida está en verde y quedan la
> fase 7 y lo que se anota abajo. Estado del motor → [`STATUS.md`](STATUS.md).

---

## Criterio de salida ✅

```
entrada:  \frac{d}{dx}\left(\left(x^{2}+1\right)^{3}\right)
salida:   6x\left(x^{2} + 1\right)^{2}
pasos:    cadena → suma → constante → potencia
sello:    verificado · nivel 0 por derivación automática
```

Verde en el navegador contra el WASM real, no sólo en un test.

Límites (Gruntz) y series quedan **fuera a propósito**, aunque
[`../architecture/04-roadmap.md`](../architecture/04-roadmap.md) §E3 los liste:
exigen orden asintótico y funciones transcendentes, que el mismo documento sitúa
en E5.

---

## Fase 0 — Línea base ✅

Medida **en esta sesión y en esta máquina**, antes de tocar una línea. Las cifras
de dos sesiones no se restan.

| Medida | Al abrir E3 |
|---|---|
| `rewrite/exit-criterion` — el criterio de salida de E1 | **6,844 µs** |
| `rewrite/coprime-sum/4` · `/8` | 11,051 µs · 20,271 µs |
| `sweep/normal-form` — 3 · 6 · 9 · 12 · 16 · 21 reglas | 0,793 · 0,848 · 1,106 · 1,158 · 1,216 · **1,345 µs** |
| Coste por regla del barrido lineal | **~30,7 ns** |
| WASM | 279 275 B crudo · **92 194 B** brotli |
| Tests · corpus en el navegador · reglas | 272 · 100 · 21 |

---

## Fases

| | Fase | |
|---|---|---|
| 1 | La derivada en el IR | ✅ |
| 2 | `ms-diff` y las ocho reglas, en su orden | ✅ |
| 3 | Verificación por números duales | ✅ |
| 4 | Lean sobre `Polynomial ℚ` | ✅ |
| 5 | Corpus `e3.json`, cobertura y hojas | ✅ |
| 6 | Hechos graficables | ✅ |
| 7 | El bloque multilínea con definiciones | ⬜ **pendiente** |
| 8 | Cierre: bench, `twiggy`, `/motor` y documentos | ✅ |

---

## Lo que salió distinto de lo planeado

### 1. La derivada no es un átomo, y el round-trip lo demostró

El plan daba por hecho que `\frac{d}{dx}\left(f\right)` se escribiría como se
escribe una llamada: un átomo que nada puede partir. Es falso. El operador liga
como el menos unario —se lleva la potencia que tenga delante— así que
`Power(derivada, 2)` salía `\frac{d}{dx}\left(f\right)^{2}` y **volvía a entrar
como la derivada de `f^{2}`**, que es otra expresión.

Lo encontró la propiedad de round-trip con una forma que nadie habría escrito a
mano: `Power(Derivative(0, x), 0)`.

### 2. Ocho reglas, no seis

Dos que el plan no tenía:

- **`derivative-constant-multiple`.** Sin ella,
  `\frac{d}{dx}\left(3x^{2}\right)` se anuncia como «derivada de un producto» y
  produce un `\frac{d}{dx}\left(3\right)x^{2}` que después vale cero. Es
  correcto y no es lo que se enseña.
- **`derivative-unsupported`.** Lo que no se sabe derivar **se dice**, en vez de
  devolver la pregunta sin responder. Mismo trato que `polynomial-irreducible`.

Y una corrección que costó un test: **una derivada cuyo operando contiene otra
derivada no está atascada**. El reescritor prueba de fuera adentro, así que en
una segunda derivada la de fuera se mira primero y ninguna regla la reconoce.
Rendirse ahí declaraba imposible algo que sale solo en cuanto la de dentro se
resuelve.

### 3. `like-terms`, que es de E2 y hacía falta en E3

La regla del producto produce `x + \left(x + 1\right)`, que se lee `x + x + 1`.
Hasta E2 no hizo falta agrupar términos semejantes porque las sumas simbólicas
salían de `ms-alg`, que devuelve un término por grado.

**No puede expandir**: sólo junta lo que ya tiene la misma forma. Convertir
`x\left(x+1\right)` en `x^{2}+x` es expandir, y expandir es una petición con
nombre justamente para que ninguna regla lo haga por su cuenta.

### 4. El `1` de `Times(1, x)` se puede quitar, y llevaba dos etapas sin quitarse

`identity-element` exigía **dos** argumentos en un producto porque
`Times(1, b^{-1})` se escribe `\frac{1}{b}` y un `b^{-1}` solo, no. La condición
correcta no era cuántos quedan sino **qué** queda: se conserva el `1` sólo cuando
lo que sobrevive es una potencia.

Lo destapó la regla del producto de E3, que deja un `1` en cuanto la derivada de
un factor se resuelve. Cabo suelto de E1 cerrado por un camino que no era el
previsto.

### 5. Los duales entraron en el evaluador, no al lado

El plan tenía una comprobación aparte para las derivadas. Vive dentro del
evaluador corriente, en `evaluate_call`: así
`\frac{d}{dx}\left(x^{2}\right)` y `2 + \frac{d}{dx}\left(x^{2}\right)` se
comprueban por el mismo camino, en vez de que el segundo salga sin verificar por
tener la derivada un nivel más abajo. El método del sello se elige por lo que
hubo que hacer para conocer el valor de la entrada, que es lo que el sello
afirma.

### 6. Cinco teoremas de siete, y los dos que faltan se dicen

`Calculus.lean` demuestra siete enunciados sobre `Polynomial ℚ`. Sólo **cinco**
reglas declaran `lean_local`: `derivative-power` y `derivative-chain` aceptan
exponentes **negativos**, que se salen del anillo de polinomios, y declarar el
teorema las contaría como respaldadas haciendo más de lo que se demostró.

Es el error simétrico al de `rational-reciprocal` en E2 —allí el teorema era más
fuerte que la regla; aquí sería más débil— y la respuesta es la misma: sellar
menos y que el sello signifique algo.

---

## Lo que te va a morder si no lo sabes

Lo de [`E1-PLAN.md`](E1-PLAN.md) y [`E2-PLAN.md`](E2-PLAN.md) sigue vigente
entero. Lo que E3 añade:

- **`\frac{d}{dx}` no es una fracción.** Si se parsea como tal, `d` partido por
  `d\cdot x` se cancela al canonicalizar y la petición desaparece.
- **La derivada necesita paréntesis bajo una potencia** (§1 de arriba).
- **Derivar no tiene inversa hoy, pero la tendrá en E4.** El día que entre
  integrar, la pregunta del ciclo vuelve, y la respuesta ya está tomada: las dos
  son peticiones con nombre y ninguna se dispara sola.
- **La potencia antes que la cadena.** Los dos órdenes dan el resultado correcto;
  sólo uno da el paso que se lee bien.
- **El verificador no puede llamar a `ms-diff`**, y `deny.toml` lo impone. Los
  números duales derivan **evaluando**: ésa es toda su gracia.
- **Los pasos de una suma salen en orden canónico, no de lectura.** En el
  criterio de salida, el `+1` se deriva antes que el `x^{2}` aunque salga escrito
  después. Cruzarlos exigiría que el reescritor conociera el orden del escritor,
  que es presentación.
- **`.lake/` son 7 GB.** Ignorado; se recupera con `lake exe cache get`.

---

## Fase 7 — El bloque multilínea con definiciones (pendiente)

El diseño, acordado, para que la fase no empiece de cero:

Hoy el frontend parte el bloque `$$…$$` y manda cada línea sola, sin contexto. Y
`a = 2` no define nada: parsea como `Head::Equal` y el motor lo *resuelve*,
devolviendo `\left\{a = 2\right\}`.

**Definir es `:=` (`\coloneqq`), explícito.** Adivinar si `a = 2` es definición o
ecuación por la forma del lado izquierdo es magia implícita, y falla justo en el
caso interesante: `x^{2}-5x+6 = 0` también tiene una incógnita a la izquierda.

1. `Head::Define` en el kernel — binaria, no *orderless*, no se pela.
2. `Head::UserFn(SymId)` para `f(x) := …`, que `node.rs` ya deja anotado como
   decisión de E3. Una función que el motor no conoce **no se evalúa**.
3. `bindings` entrando al contrato (`crates/ms-serialize/src/result.rs`), donde
   está anotado como diferido desde E1.
4. `solve_block(latex)` en `crates/ms-wasm/src/lib.rs`: parte por `\\`, arrastra
   un entorno de línea en línea, devuelve un documento por línea.
5. El frontend (`src/components/artifacts/MathRunner.tsx`,
   `src/lib/engine/runner.ts`) manda el bloque entero.

**Criterio propio:** en el editor de notas,
`f(x) := \left(x^{2}+1\right)^{3} \\ \frac{d}{dx}f(x)` devuelve la derivada con
sus pasos.

---

## Cómo compruebas que no has roto nada

```sh
pnpm engine:test     # cargo nextest sobre el workspace
pnpm engine:gates    # clippy + cargo-deny + frontera + tree --check
pnpm engine:lean     # nombres de teorema + lake build  (NO bloquea)
pnpm engine:build    # compila a WASM  (ANTES de engine:browser)
pnpm engine:size     # twiggy contra el presupuesto de 3 MB
pnpm engine:bench    # sólo contra medidas de la misma sesión
pnpm engine:browser  # el corpus contra el WASM real, y lo que la página pinta
pnpm check           # astro check
```
