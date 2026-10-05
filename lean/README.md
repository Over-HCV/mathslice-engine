# `engine/lean` — la corrección de las reglas, demostrada

Este proyecto no calcula nada. Enuncia como teorema **lo que cada regla del
motor hace** y lo demuestra, una vez, en tiempo de diseño
([`../architecture/03-lean-verification.md`](../architecture/03-lean-verification.md)).

Coste en runtime: **cero**. Coste en el WASM que se envía al navegador: **cero**.
Los `.olean` de Mathlib pesan cientos de megas y el presupuesto del motor son 3
MB comprimidos; nada de esto viaja al navegador, y ésa es la razón de que sea
viable.

## Cómo se corre

```sh
pnpm engine:lean     # cruza los nombres y compila
```

Son dos cosas, y las dos importan:

1. **`scripts/check-soundness.sh`** cruza cada
   `#[soundness = lean_local("MathSlice.X")]` de `crates/` con un `theorem X`
   real de aquí. Una errata en el nombre afirmaría una demostración que no
   existe, que es justo el fallo que el campo `soundness` viene a impedir.
2. **`lake build`** demuestra los teoremas.

**No está en `engine:gates`**, y es a propósito: el roadmap dice que Lean nunca
bloquea una etapa. `numeric_only` es una respuesta válida para cualquier regla.

## Primera vez

Hace falta `elan`, y Mathlib se baja precompilada:

```sh
curl -sSf https://elan.lean-lang.org/elan-init.sh | sh
cd engine/lean && lake exe cache get
```

Son unos 7 GB en `.lake/`, que está en el `.gitignore` y se recupera con ese
mismo comando.

## Qué hay demostrado

Cinco reglas de E1, en [`MathSlice/Rational.lean`](MathSlice/Rational.lean). El
criterio de salida de E1 sale con **cuatro de sus seis pasos** respaldados por un
teorema, y el nombre viaja hasta el documento de salida.

Dos cosas que la demostración enseñó y el código no decía:

- **`rational-reduce` no necesita que el denominador sea no nulo.** Lean rechazó
  la hipótesis por no usarse.
- **`rational-reciprocal` es cierta sin ninguna hipótesis**, porque en Lean
  `0⁻¹ = 0` por convención. El motor **no puede apoyarse en eso**: para él
  `\frac{a}{0}` es un error, no cero. La guarda de la regla sigue haciendo falta
  aunque el teorema no la pida — un teorema más fuerte que la regla no la
  respalda entera.
