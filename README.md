# MathSlice Engine

Motor de cómputo simbólico en Rust, compilado a WASM y ejecutado en el navegador.
Es el motor de [MathSlice](https://github.com/Over-HCV/mathslice).

**El objetivo no es la respuesta: son los pasos.** Cada resultado lleva la traza
de la derivación que lo produjo, para que sirva como material didáctico y no solo
como calculadora.

## Estructura

| Ruta | Qué hay |
|---|---|
| `crates/` | El workspace de Rust: `ms-core` (expresiones), `ms-num` (torre numérica), `ms-alg`, `ms-diff`, `ms-solve`, `ms-rules`, `ms-parse`, `ms-facts`, `ms-verify`, `ms-wasm` (bindings) y `ms-cli` (herramientas) |
| `architecture/` | Las decisiones de diseño, en orden de lectura — empieza por [`architecture/README.md`](architecture/README.md) |
| `corpus/` | Corpus dorado: los casos que el motor tiene que resolver, por etapa |
| `docs/` + `tree/` | El inventario de capacidades y su cobertura actual |
| `lean/` | Demostraciones en Lean 4 de la corrección de las reglas |

## Desarrollo

Requiere el toolchain fijado en `rust-toolchain.toml` (rustup lo instala solo) y
[`cargo-nextest`](https://nexte.st), [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny)
y [`wasm-pack`](https://github.com/drager/wasm-pack).

```sh
cargo nextest run --workspace          # tests
cargo clippy --workspace --all-targets # lint
cargo deny check                       # licencias y dirección de dependencias
cargo run -q -p ms-cli -- tree --check # el árbol de cobertura está al día

cd crates/ms-wasm && wasm-pack build --target web   # compila a WASM
```

## Licencia

MIT — ver [`LICENSE`](LICENSE).
