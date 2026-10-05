# Verificación del proyecto

> Cómo sabemos que el motor está bien. No confundir con [`03-lean-verification.md`](03-lean-verification.md), que es cómo verificamos un *resultado*; esto es cómo verificamos el *proyecto*.

---

## 1. Corpus dorado — la métrica del proyecto

Cada hoja de [`../docs/`](../docs/) es un caso de prueba. Los archivos ya traen el nombre de cada caso:

```
Compute an indefinite integral:
Solve a polynomial equation:
Find the degree of a polynomial:
```

`ms-cli` extrae esas hojas y reporta cobertura contra las 958 capacidades reales:

```sh
pnpm engine:coverage      # o: cd engine && cargo run -p ms-cli -- coverage
cargo run -p ms-cli -- capabilities "Repeating Decimals"   # busca en el inventario
```

El denominador sale de contar las hojas de `docs/` **menos** las 751 de
`common-core-math.md`, que no son capacidades nuevas: son las mismas re-etiquetadas
por grado escolar. El numerador sale del campo `capability` que cada caso de
`engine/corpus/` puede declarar, con el id completo de la hoja:

```json
{ "name": "un-decimal-es-una-fraccion",
  "capability": "Numbers > Rational Numbers > Rational Numbers > Do exact arithmetic on rational numbers" }
```

El campo es **opcional**: un caso que no lo lleva no cuenta. Los que fijan un
error o un límite —división entre cero, base fuera de rango— no responden a
ninguna capacidad, y contarlos sería inflar la métrica. Por el mismo motivo un id
que no exista en el inventario **hace fallar** a la herramienta en vez de sumar.

### El mismo cruce, mirable: `/motor`

```sh
pnpm engine:tree          # emite engine/tree/: un JSON por categoría y un índice
pnpm engine:tree:check    # falla si lo versionado no coincide (va en engine:gates)
```

`engine/tree/` es el inventario con su check y con los casos de cada capacidad
dentro. Lo lee `/motor` en el sitio, que enseña el árbol por categoría y
**ejecuta cada caso contra el WASM real**, comparándolo con lo que el corpus fija:
la misma comprobación que `pnpm engine:browser`, hecha a mano y a la vista.

Dos cosas que lo mantienen honesto:

- **El check se calcula, no se escribe.** Una hoja está marcada si tiene al menos
  un caso; una sección, si lo están todas las suyas. Nada en la capa web puede
  marcar nada.
- **`tree --check` corre en `engine:gates`.** Un caso nuevo sin regenerar deja la
  página afirmando un progreso que no existe, y eso es peor que no tener página.

> **Ese porcentaje es *la* métrica del proyecto.** Se reporta en [`../context/STATUS.md`](../context/STATUS.md).

---

## 2. Testing diferencial contra SymPy y Maxima

En CI — **nunca en runtime, nunca en WASM**. Comparar nuestras respuestas contra SymPy y Maxima sobre el corpus, paralelizado con `rayon`.

Cada discrepancia se triagea a mano: o es bug nuestro o es bug de ellos. **Ambas se documentan.**

> Esto es lo que sustituye los 35 años de casos borde acumulados en el kernel de un CAS comercial. Es la parte no glamorosa y es la que decide si el motor es confiable.

---

## 3. Property testing

`proptest`. **Las leyes matemáticas SON property tests**, así que esta técnica no es un extra: es la forma natural de probar un CAS.

| Propiedad | Por qué importa |
|---|---|
| `parse(latex(e)) == e` | **La más importante.** LaTeX es la única entrada; si el round-trip falla, todo lo demás es humo |
| canonicalización ⇒ mismo hash | El hash-consing es la base del rendimiento y de la detección de ciclos |
| `d/dx ∫f dx == f` | Verifica integración contra derivación, que es el par mejor probado |
| `factor ∘ expand == id` | El invariante de `ms-alg`. **Existe desde E2**, enunciado al revés porque es la dirección comprobable: multiplicar los factores devuelve el polinomio |
| el e-grafo nunca extrae una forma no equivalente | La saturación por igualdad es potente y por eso peligrosa. **Sin e-grafos todavía**: E2 los recortó, ver [`04-roadmap.md`](04-roadmap.md) §E2 |

---

## 4. Soundness en Lean

`lake build` en CI. Métrica: **% de reglas con teorema demostrado**.

Detalle en [`03-lean-verification.md`](03-lean-verification.md) §2.

---

## 5. Test de frontera

Invariante 4 de [`02-architecture.md`](02-architecture.md):

```sh
grep -ri "desmos\|mafs\|svg\|canvas\|viewport\|color" engine/crates/   # debe salir vacío
```

Vacío, o rompe el build.

> Es la **única** defensa mecánica contra que la representación se filtre al kernel con el tiempo. Sin este test, en dos años habrá un `viewport_hint` en algún struct y la frontera habrá muerto sin que nadie lo decidiera.

---

## 6. Presupuesto de WASM

**< 3 MB comprimido** para E0–E3. Medido en CI con umbral duro que rompe el build.

Referencia: Pyodide pesa ~10 MB solo para arrancar, y ya lo pagamos en `src/lib/pyRunner.ts`.

Toda dependencia nueva se justifica contra este presupuesto.

---

## 7. Fuzzing

`cargo-fuzz` sobre `ms-parse`. Entra texto arbitrario escrito por usuarios: el parser es la superficie de ataque y la fuente más probable de panics.

---

## 8. Lints en `deny`

De [`01-rust-principles.md`](01-rust-principles.md):

| Lint | Principio |
|---|---|
| `wildcard_enum_match_arm` — sin `_ =>` en `Node`/`Head` | §1 |
| sin `unwrap`/`expect`/`panic` fuera de tests | §6 |
| `#![forbid(unsafe_code)]` | §9 |
| `overflow-checks = true` **también en release** | §9 |

Configurados en el **primer commit**. Añadirlos después significa arreglar cientos de sitios de golpe.

---

## 9. Benchmarks

`criterion`, desde E0. El matcher es lo que se degrada silenciosamente conforme crece el rule set — pasar de 50 a 6700 reglas sin medir es cómo se muere el rendimiento sin que nadie note cuándo.

---

## 10. El corpus en un navegador de verdad

Todo lo anterior prueba el kernel compilado para el host. Nada de eso ve lo que recibe el estudiante: el `.wasm` servido por Vite, instanciado dentro de un module worker, cruzando la frontera de `wasm-bindgen`, con la isla de React hidratada al lado.

> E0 se cerró sin que nadie abriera la página del motor en un navegador. Se había comprobado ejecutando el `.wasm` en Node y viendo que el build emitía el worker y el asset — que es otra cosa. **Esa clase de hueco se cierra con un test, no con una nota.**

### Automático — `pnpm engine:browser`

```sh
pnpm dev              # en otra terminal
pnpm engine:browser
```

`engine/scripts/browser-check.mjs` arranca Chrome headless, carga `/motor/elementary-math`, importa el **mismo** `src/lib/engine/runner.ts` que usa la interfaz y corre todos los casos de `engine/corpus/*.json` contra el worker real. Compara con las mismas reglas que `crates/ms-wasm/tests/corpus.rs`: valor exacto, tipo, aproximación, derivación, hechos, verificación y error.

Y desde E2 comprueba además **lo que la página pinta**: despliega el árbol,
ejecuta todos los casos y recorre los nodos de texto buscando comandos de LaTeX
fuera de un `.katex` — donde KaTeX guarda el fuente en su `<annotation>` y ahí
es correcto.

Falla —con código de salida distinto de cero— si:

- un caso del corpus da otro valor o otra derivación,
- **algún resultado se imprime como LaTeX crudo en vez de renderizarse**,
- la isla del árbol no hidrata (su `<astro-island>` conserva el atributo `ssr`),
- la consola del navegador tiene **cualquier** error.

Las tres cosas son invisibles para `cargo nextest` y para `astro check`. La segunda es la que de verdad muerde: el motor puede estar perfecto y el estudiante ver una página muerta. Ha pasado dos veces — una con una caché de dependencias rancia de Vite, y otra cuando la isla importó una función de un módulo que lee `node:fs`, que Vite externaliza en el navegador—. **La señal es el atributo `ssr`**, que Astro quita al hidratar: buscar un elemento del DOM no vale, porque el servidor lo renderiza igual y el marcado de una isla muerta es idéntico al de una viva.

### Por qué la comprobación de renderizado entró tarde, y qué queda a ojo

**Hasta E2 este script comparaba sólo datos.** Llamaba a `solve()` y contrastaba
el documento con el corpus, así que un resultado **correcto** impreso como LaTeX
crudo pasaba en verde. Ése es el hueco por el que se colaron los **quince**
defectos de lectura del proyecto, todos de la misma clase: el valor era correcto
y lo que el estudiante leía, no. Ninguno lo habría encontrado `cargo nextest`.

El decimoquinto —un conjunto de soluciones escrito
`\left\{x = 2, x = 3\right\}` como texto monoespaciado— fue el que dejó claro
que el problema no era mirar poco: era que **mirar fuese el mecanismo**. Un
ritual que se repite cada etapa se acaba saltando; una puerta, no.

> **La lección, que vale más que el arreglo:** cuando un defecto se encuentra
> «sólo mirando», la pregunta no es «hay que mirar más», es «¿por qué esto no lo
> puede ver una máquina?». Casi siempre puede: el LaTeX sin renderizar está en el
> DOM como texto, y hacen falta cero píxeles para leerlo.

**El segundo consumidor: el editor de `/anotaciones`.** Desde julio de 2026 el
bloque `$$…$$` se escribe con MathLive, y entre lo que el estudiante teclea y lo
que el parser lee hay tres traducciones que ningún test de Rust ve: la macro que
convierte `\operatorname{expandir}` en un átomo, `getValue("latex-expanded")` —que
devuelve `\operatorname{\mathrm{expandir}}`, y ese `\mathrm` el lexer lo rechaza— y
la normalización que lo deshace. Ninguna de las tres falla al escribir: fallan al
resolver, y desde fuera parecen un motor que no sabe expandir.

Por eso `browser-check.mjs` termina en `/anotaciones`: siembra un documento, abre
el bloque, inserta la llamada por el mismo camino que la paleta y comprueba que lo
que se guardaría **parsea y resuelve**. Es la misma lección de arriba aplicada a
otra frontera — y ya atrapó que configurar el campo antes de montarlo lo dejaba
sin montar, con el bloque enseñando su paleta y ni una línea donde escribir.

**Lo que sigue necesitando ojos** es lo que el DOM tiene bien y los píxeles mal:
solapamientos, contraste, texto cortado, una traza que no cabe en el ancho. Es
raro, y no hay forma barata de automatizarlo. Al cerrar una etapa, abrir `/motor`
y mirar una consulta larga sigue mereciendo los dos minutos.

Y al mirar, comprobar que están las cuatro cosas del contrato:

- el resultado exacto y su aproximación,
- la traza de pasos, con los títulos en español,
- **el nivel de verificación, visible y no escondido** (invariante 3),
- los `facts` del resultado. `alternates` cuando exista (E3).

Desde E3 en adelante, comprobar además que `toGraph.ts` produce la gráfica **a partir de los hechos**, no de un campo que venga del motor.

---

## Regla general

**Toda frontera arquitectónica de este proyecto tiene un test que la verifica.**

Si una regla de diseño no se puede comprobar mecánicamente, se reescribe hasta que se pueda — o se admite que es solo una preferencia y se deja de llamarla regla.
