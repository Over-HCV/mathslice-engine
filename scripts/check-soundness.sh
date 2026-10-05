#!/usr/bin/env bash
# Cada `#[soundness = lean_local("MathSlice.X")]` tiene que corresponder a un
# `theorem X` real en engine/lean/.
#
# Sin esto, una errata en el nombre afirmaría una demostración que no existe —
# y el campo `soundness` está precisamente para que eso no pueda pasar. Es la
# misma lógica que hace fallar a `ms-cli` ante una capacidad inventada: una
# métrica que se puede inflar con una errata no sirve para nada.
#
# No compila Lean. Eso lo hace `lake build`, que corre justo después en
# `pnpm engine:lean`; esto sólo cruza los dos lados.
set -euo pipefail

cd "$(dirname "$0")/.."

declared=$(rg --only-matching --no-filename \
    'lean_local\("MathSlice\.([a-zA-Z0-9_]+)"\)' --replace '$1' crates/ \
    | sort --unique)

if [[ -z "$declared" ]]; then
    echo "Ninguna regla declara un teorema local todavía."
    exit 0
fi

missing=""
for theorem in $declared; do
    if ! rg --quiet "^theorem ${theorem}\b" lean/MathSlice/; then
        missing="${missing}  ${theorem}\n"
    fi
done

if [[ -n "$missing" ]]; then
    echo "Reglas que nombran un teorema que no existe en engine/lean/:"
    printf "%b" "$missing"
    exit 1
fi

count=$(echo "$declared" | wc -l | tr -d ' ')
echo "Soundness: ${count} teoremas nombrados por reglas, y los ${count} existen."
