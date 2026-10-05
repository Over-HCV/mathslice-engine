#!/usr/bin/env bash
# Invariante 4 de 02-architecture.md §4: ningún crate menciona un motor de
# representación. El motor emite hechos matemáticos; quién decide que un polo se
# dibuja como asíntota punteada es la capa de TypeScript.
#
# Es la ÚNICA defensa mecánica contra que la representación se filtre al kernel.
# Sin esto, en dos años hay un `viewport_hint` en algún struct y la frontera ha
# muerto sin que nadie lo decidiera (05-testing.md §5).
set -euo pipefail

crates_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/crates"

# Dos términos del grep de 05-testing.md §5 se quedan fuera a propósito, porque
# colisionan con vocabulario legítimo y un test que falla en falso se acaba
# desactivando, que es peor que no tenerlo:
#
#   - `color`  → la coloración de grafos es Discrete Mathematics (E6).
#   - `render` → el escritor de LaTeX habla de cadenas "rendered" sin dibujar nada.
#
# Los que quedan no tienen significado matemático: una aparición es siempre una
# fuga de representación. Con `-w` para que sea la palabra, no un trozo de otra.
forbidden='desmos|mafs|katex|svg|canvas|viewport|css|pixel|stylesheet'

if rg --ignore-case --word-regexp --line-number "$forbidden" "$crates_dir"; then
  echo
  echo "FRONTERA ROTA: el kernel menciona una capa de representación (02-architecture.md §4)."
  echo "El motor computa, no representa. Mueve eso a src/lib/engine/represent/."
  exit 1
fi

echo "Frontera intacta: ningún crate conoce un motor de representación."
