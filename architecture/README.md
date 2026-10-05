# Arquitectura del motor matemático

Construimos un motor de cómputo simbólico propio — **solo la capa matemática** — en Rust, compilado a WASM, corriendo en el navegador.

**El objetivo primario no es la respuesta: son los pasos.** SymPy no puede producir derivaciones paso a paso, y por eso no se envuelve una librería existente. Un motor que solo escupe el resultado es una calculadora; uno que traza la derivación es material didáctico.

---

## Lee en este orden

| # | Documento | Qué responde |
|---|---|---|
| 00 | [`00-cas-internals.md`](00-cas-internals.md) | **Cómo funciona un CAS comercial por debajo.** Expresiones, reescritura, atributos, los algoritmos reales, y por qué no existe forma canónica general |
| 01 | [`01-rust-principles.md`](01-rust-principles.md) | **Los 11 principios de Rust**, cada uno con su consecuencia obligatoria. Empieza aquí si vas a escribir código |
| 02 | [`02-architecture.md`](02-architecture.md) | **Los crates, el contrato de salida y las 4 invariantes.** Incluye la frontera dura: el motor computa, no representa |
| 03 | [`03-lean-verification.md`](03-lean-verification.md) | **Lean 4 y los 3 niveles de verificación.** Qué puede y qué no puede hacer un asistente de demostración por nosotros |
| 04 | [`04-roadmap.md`](04-roadmap.md) | **Las etapas E0 → E∞** y los riesgos con respuesta |
| 05 | [`05-testing.md`](05-testing.md) | **Corpus dorado, property tests, presupuestos, lints.** Cómo sabemos que está bien |

**¿En qué etapa estamos ahora mismo?** → [`../context/STATUS.md`](../context/STATUS.md)

---

## Las decisiones, en una tabla

| Decisión | Valor | Por qué |
|---|---|---|
| Kernel | Propio, Rust, día 1 | SymPy no puede dar pasos |
| Runtime | WASM en el navegador | Latencia cero, offline, sin coste de servidor |
| Objetivo primario | Pasos pedagógicos | Es lo que los CAS comerciales venden aparte y no se puede retrofitear |
| Entrada | **LaTeX exclusivamente** | Los símbolos (`∫`, `√`) son cosa de la interfaz, no del parser |
| Salida | Resultado normalizado semántico | El motor **no representa**; emite hechos |
| Ubicación | `engine/`, agnóstico | No conoce Astro, React ni MathSlice |
| SymPy / Maxima | Solo oráculos en CI | Testing diferencial, nunca en el camino crítico |

---

## Las 4 invariantes

1. Ninguna regla se dispara sin sus suposiciones satisfechas — **impuesto por tipos**.
2. Toda reescritura emite traza — lo silencioso se marca explícitamente.
3. Ningún resultado se muestra sin pasar por `ms-verify`.
4. Ningún crate menciona un motor de representación — **verificado por `grep` en CI**.

Las cuatro tienen comprobación mecánica. Una regla de diseño que no se puede verificar es solo una preferencia.

---

## Estructura de `engine/`

```
engine/
├── architecture/   ← estás aquí: CÓMO lo construimos
├── docs/           ← QUÉ hay que cubrir: árbol de 1709 capacidades de referencia
├── context/        ← EN QUÉ VAMOS: STATUS.md, estado vivo y métricas
├── crates/         (por crear) el workspace Rust
├── lean/           (por crear) proyecto de soundness
└── corpus/         (por crear) casos dorados extraídos de docs/
```
