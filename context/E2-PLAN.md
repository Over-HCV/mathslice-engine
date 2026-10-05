# E2 — Polinomios y álgebra base · plan de ejecución

> **⚠️ E2 está cerrada. Este archivo ya no es un traspaso: es el registro de cómo
> se ejecutó.** Las siete fases están hechas. Lo que se decidió está resumido en
> [`STATUS.md`](STATUS.md); lo que queda aquí es el detalle, útil para entender
> por qué algo se hizo así y no para saber qué hacer ahora.
>
> **Qué hacer ahora → [`STATUS.md`](STATUS.md) §Lo siguiente**, que apunta a E3 en
> [`../architecture/04-roadmap.md`](../architecture/04-roadmap.md).
>
> Se conserva por lo mismo que `E1-PLAN.md`: la mitad del valor de un plan está
> en las cosas que salieron distintas, y están abajo.

---

## Lo que salió distinto de lo planeado

Ocho cosas. Las cuatro primeras cambiaron el diseño; las cuatro últimas son
defectos que ningún test habría encontrado.

### 1. `Head::Solutions`, no `Head::List`

El plan decía `List`, orderless, conteniendo las ecuaciones solución. Se llamó
`Solutions` porque el atributo tiene que ser **obviamente** correcto: las
soluciones de una ecuación son un **conjunto**, y ahí *orderless* no se discute.
Un `List` genérico sin orden habría sido la sorpresa, y el día que E3 necesite
algo genuinamente ordenado —los puntos de una interpolación, las filas de una
matriz— entra como otra cabeza con el atributo contrario, sin pelearse con ésta.

### 2. Expandir y factorizar no son reglas, son consultas

**Es la desviación que más cambió la etapa.** El plan las ponía como reglas del
reescritor. Son inversas: una regla que expandiera `\left(x-2\right)\left(x-3\right)`
y otra que factorizara `x^{2}-5x+6` se devolverían el trabajo hasta agotar el
presupuesto.

Entran como `Head::Fn(Builtin)` —`\operatorname{expandir}`,
`\operatorname{factorizar}`— que es además lo que dice el árbol de capacidades,
donde «Expand a polynomial» es un **ejemplo de consulta**. Con la petición en la
cabeza no hay ciclo posible: la regla la consume y lo que queda ya no pide nada.

Y **dentro de una ecuación factorizar sí es automático**, porque ahí no hay con
quién ciclar: expandir no se hace nunca por su cuenta.

### 3. El `=` no es un operador del Pratt

El plan decía «precedencia mínima». Como operador tendría que asociar hacia algún
lado, y `x = 2 = 3` construiría `Equal(Equal(x,2), 3)`: una ecuación cuyo lado
izquierdo es otra ecuación, que **se escribe igual que la cadena que el usuario
tecleó** y significa otra cosa. Es una relación no asociativa, resuelta fuera del
bucle de precedencias, y sólo en la raíz.

### 4. El muestreo del nivel 0, que no estaba en el plan

`03-lean-verification.md` lo describía desde E0 y no existía. Con una variable
libre no hay número que comparar, así que `\operatorname{factorizar}(p)` salía
«sin verificar» — y no cambiar el valor es *exactamente* lo comprobable.

Entró con dos correcciones al documento: los puntos son **fijos** (un sello
irreproducible no sirve cuando alguien discute un resultado) y **racionales
exactos**, no flotantes a 50 dígitos con aritmética de bolas (el evaluador ya
vive en ℚ y no hay error que acotar). Rescató cinco casos de E0 y E1.

### 5. `equation-zero-product` no disparaba nunca

Y no era un defecto de lectura: era la fase 3 entera sin funcionar. La
comparación «este lado ya estaba factorizado» iba sobre ids **en crudo**, y el
producto recién construido lleva los factores en el orden en que salieron de la
factorización mientras el lado izquierdo los tiene ya canonicalizados.

Decía «cambió» sin cambiar nada. La regla se disparaba una segunda vez, el
reescritor canonicalizaba, veía el estado anterior y **paraba ahí**: la ecuación
salía factorizada y sin resolver, sin error y sin paso que lo explicara. Lo
encontró ejecutar la consulta y leer la salida, no el corpus.

### 6. Escribir `-5x` sin paréntesis rompió el round-trip

Arreglar `\left(-5\right)x` —que nadie escribe así— hizo que `- 5x` volviera a
entrar como `Times(-1, 5, x)` en vez de `Times(-5, x)`: un factor de más, mismo
valor, distinta expresión. Lo destapó la propiedad del round-trip con
`0 - 1 \cdot 0^{0}`.

El arreglo mejoró el parser por su cuenta: el menos unario ahora empuja el signo
al literal que ya está delante, bajando por la espina izquierda del producto
porque a esa altura nada está aplanado todavía.

### 7. Lean demostró de más, y eso también es información

`rational-reciprocal` salió **cierta sin ninguna hipótesis**, porque en Lean
`0⁻¹ = 0` por convención. El motor no puede apoyarse en eso: para él
`\frac{a}{0}` es un error, no cero.

Es el §2 de `03-lean-verification.md` al revés. No descubrió una hipótesis que
faltaba: descubrió que **la guarda de la regla protege algo que el teorema no
dice**, y que un teorema más fuerte que la regla no la respalda entera. Queda
escrito en el `.lean` para que dentro de un año nadie «arregle» la guarda.

(Y en la dirección normal: `rational-reduce` no necesita que el denominador sea
no nulo. Lean rechazó la hipótesis por no usarse.)

### 8. `browser-check.mjs` no miraba la página

Se descubrió al cerrar, y explica los quince defectos de lectura del proyecto:
el script llamaba a `solve()` y comparaba **datos**, así que un resultado
correcto impreso como LaTeX crudo pasaba en verde.

Ahora despliega el árbol, ejecuta los casos y recorre los nodos de texto
buscando comandos de LaTeX fuera de un `.katex`. La comprobación manual que E1
repetía como ritual pasó a ser una puerta.

> **Y un falso negativo propio, que conviene recordar:** la primera versión
> pulsaba todos los botones a ciegas, lo cual **cerraba** los que el árbol abre
> por defecto, y daba verde sobre una página sin nada desplegado. Ahora sólo
> pulsa los que tienen `aria-expanded="false"` y **lanza error si no despliega
> ningún caso**: un check que no puede fallar no comprueba nada.

---

## Las decisiones de alcance, y por qué

Confirmadas con el usuario antes de empezar. Están también en
[`../architecture/04-roadmap.md`](../architecture/04-roadmap.md) §E2, que es
donde viven de verdad.

| Decisión | Elegido | Motivo |
|---|---|---|
| Criterio de salida | Resolver una cuadrática | La vertical completa: fuerza `=`, forma polinómica, factorización, resolución y verificación nivel 1 |
| Álgebra lineal (63 hojas) | **Recortada** | Eje ortogonal a la espina polinómica: cabeza nueva, `\pmatrix`, tipo de valor nuevo |
| e-grafos (`egg`) | **Fuera** | Los polinomios sí tienen forma canónica; los e-grafos son para cuando no la hay. Y es una dependencia grande contra el presupuesto de WASM |
| `engine/lean/` | El cable, no el corpus | Demostrar que el cable existe antes de que haya 200 teoremas que escribir |

**Lo que el recorte de los e-grafos se llevó por delante:** la forma de salida
propia de `ms-simplify`, y con ella los dos cabos sueltos que la esperaban —el
`\frac` con varios factores en el denominador y el `1` de `Times(1, b^{-1})`—.
Siguen abiertos y anotados.

---

## Las siete fases, tal y como se ejecutaron

### Fase 0 — línea base

Medida **en la misma sesión** que la del cierre, que es la única forma de que las
cifras se puedan restar. 244 tests, 87 casos, 3,34 % de cobertura, 77,4 KB
brotli, ~39,6 ns por regla con 12 reglas.

Y se cerró el diff pendiente de `docs/main.md`: el análisis de esfuerzo que abría
el inventario concluía «envoltorio sobre SymPy», lo contrario de la decisión de
arquitectura. Quedó marcado como histórico, conservando su dimensionado —que es
lo que vale— y desmintiendo su conclusión.

### Fase 1 — la ecuación en el IR

`Head` creció `Equal` y `Solutions` en **un** commit, y el compilador enumeró los
sitios. `ms-parse` aprendió `=` y `\left\{…\right\}` —el conjunto se lee además
de escribirse, porque una cabeza que el escritor emite y el parser no sabe leer
rompe `parse(latex(e)) == e`, y lo pedía el propio generador del round-trip—.

Crate `ms-alg`: `poly.rs` genérico en `C`, `bridge.rs` concreto. `Arena::sym_id`,
que faltaba.

### Fase 2 — polinomios

Las consultas con nombre, la factorización por raíces racionales, `mcd`. Cuatro
defectos de lectura arreglados, incluido el orden de los términos de una suma:
ahora lo decide **el escritor**, por grado, sin tocar el orden canónico. El
round-trip aguanta porque `Plus` es orderless.

`rational.rs` pasó a llamarse `set.rs`: es el conjunto entero de reglas y su
orden es semántico.

### Fase 3 — resolver

Crate `ms-solve`, cuatro reglas de ecuación y `ResultKind::Solutions` con su
espejo en TypeScript. Aquí cayó el criterio de salida.

Una ecuación con raíces irracionales sale con un **error que lo dice**, siguiendo
el precedente de `enBase` con una fracción: no dice «no tiene solución», que
sería falso.

### Fase 4 — verificación

Nivel 1 por sustitución, y el muestreo. `Evidence::level` dejó de escribirse y se
deriva del método: eran dos datos que decían lo mismo.

### Fase 5 — Lean

`elan`, Lean 4 y Mathlib v4.32.1. Cinco teoremas. `#[soundness]` pasó de cadena
libre a enum con las tres formas del documento. `check-soundness.sh`, comprobado
que falla ante una errata.

### Fase 6 — cierre

Bench sobre 21 reglas (~31 ns por regla, la red de discriminación sigue sin
justificarse), `twiggy`, `/motor` abierto y mirado, y los documentos.

---

## Lo que te va a morder si no lo sabes

Lo de [`E1-PLAN.md`](E1-PLAN.md) sigue vigente **entero**. Lo que E2 añade:

- **Expandir y factorizar son inversas.** Si alguna vez las conviertes en reglas
  automáticas, el reescritor cicla. La petición en la cabeza es lo que lo impide.
- **Una regla que devuelve algo estructuralmente igual a lo que recibió para el
  reescritor.** Compara **formas canónicas**, no ids en crudo: el resultado de
  una regla no está canonicalizado y el estado que tiene delante sí.
- **El escritor ordena los términos de una suma por grado.** Es presentación, y
  el orden canónico no se toca. Si cambias una de las dos cosas, el round-trip es
  el test que te lo dirá.
- **Un teorema más fuerte que la regla no la respalda entera.** `0⁻¹ = 0` en Lean
  hace cierto sin hipótesis algo que el motor sí tiene que guardar.
- **`engine:lean` no está en `engine:gates`**, a propósito. Lean nunca bloquea
  una etapa.
- **`.lake/` son 7 GB.** Está ignorado; se recupera con `lake exe cache get`.

---

## Cómo compruebas que no has roto nada

```sh
pnpm engine:test     # cargo nextest sobre el workspace
pnpm engine:gates    # clippy + cargo-deny + frontera + tree --check
pnpm engine:lean     # nombres de teorema + lake build  (NO bloquea)
pnpm engine:build    # compila a WASM  (hazlo ANTES de engine:browser)
pnpm engine:size     # twiggy contra el presupuesto de 3 MB
pnpm engine:bench    # matcher; compara sólo con medidas de la misma sesión
pnpm engine:browser  # el corpus contra el WASM real, y lo que la página pinta
pnpm check           # astro check
```

**Definición de terminado de E2, cumplida:** el criterio de salida en verde en el
navegador con su sello de nivel 1 · un caso de corpus por capacidad · las hojas
de Algebra marcadas en `/motor` y la cobertura recalculada por `ms-cli` · el
bench medido contra la línea base de la fase 0 · `twiggy` bajo presupuesto · el
recorte de álgebra lineal escrito donde toca · y Lean compilando.
