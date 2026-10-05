#!/usr/bin/env node
/*
  El corpus dorado contra el motor REAL en un navegador REAL.

  `cargo nextest` prueba el kernel compilado para el host. Esto prueba lo que de
  verdad recibe el estudiante: el `.wasm` servido por Vite, instanciado dentro de
  un module worker, cruzando la frontera de wasm-bindgen, con la isla de React
  hidratada al lado. Es la clase de rotura que ni `cargo nextest` ni
  `astro check` pueden ver — y la etapa E0 se cerró sin que nadie la comprobara
  (05-testing.md §10).

  Uso:
      pnpm engine:browser                  # contra http://localhost:4321
      pnpm engine:browser -- --url http://localhost:4322

  Necesita un servidor de desarrollo ya arrancado (`pnpm dev`). No lo arranca por
  su cuenta a propósito: quien lo arranca decide el puerto y ve sus logs.
*/

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS_DIR = resolve(HERE, "..", "corpus");
/* La página del motor que carga el worker. `/engine-lab` era la anterior y se
   borró: /motor enseña lo mismo y además dice a qué capacidad responde cada caso. */
const LAB_PATH = "/motor/elementary-math";
/* El módulo que la página importa. Es el mismo camino que usa CapabilityTree.tsx,
   así que probamos el cliente de verdad y no una copia suya. */
const RUNNER_MODULE = "/src/lib/engine/runner.ts";
/* La isla del árbol de capacidades, que es la que trae el runner a la página. */
const ISLAND_COMPONENT = "CapabilityTree";
const DEBUG_PORT = 9333;
/* La primera consulta paga la descarga y la instanciación del WASM. */
const PAGE_SETTLE_MS = 6000;

/// Cuántas veces se intenta desplegar. El árbol tiene categoría → sección → hoja.
const EXPAND_ROUNDS = 5;
const APPROX_TOLERANCE = 1e-12;

const CHROME_CANDIDATES = [
  process.env.CHROME_PATH,
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
].filter(Boolean);

function findChrome() {
  const found = CHROME_CANDIDATES.find((path) => existsSync(path));
  if (!found) {
    throw new Error(
      `No se encontró Chrome. Prueba con CHROME_PATH=/ruta/a/chrome.\nBuscado en:\n  ${CHROME_CANDIDATES.join("\n  ")}`,
    );
  }
  return found;
}

function parseArguments(argv) {
  const flag = argv.indexOf("--url");
  const base = flag === -1 ? "http://localhost:4321" : argv[flag + 1];
  if (!base) throw new Error("--url necesita un valor");
  return { base: base.replace(/\/$/, "") };
}

async function loadCorpus() {
  const files = (await readdir(CORPUS_DIR)).filter((name) => name.endsWith(".json")).sort();
  const cases = [];
  for (const file of files) {
    const parsed = JSON.parse(readFileSync(join(CORPUS_DIR, file), "utf8"));
    for (const entry of parsed.cases ?? []) cases.push({ ...entry, stage: parsed.stage ?? file });
  }
  if (cases.length === 0) throw new Error(`No hay casos en ${CORPUS_DIR}`);
  return cases;
}

async function waitForServer(base) {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${base}${LAB_PATH}`);
      if (response.ok) return;
    } catch {
      /* El servidor todavía no acepta conexiones. */
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `No hay servidor de desarrollo en ${base}${LAB_PATH}.\nArráncalo con «pnpm dev» y vuelve a intentarlo.`,
  );
}

/* ---- CDP, lo mínimo: un socket y un contador de mensajes -------------------- */

class Session {
  #socket;
  #nextId = 0;
  #pending = new Map();

  consoleErrors = [];

  constructor(socket) {
    this.#socket = socket;
    socket.addEventListener("message", (event) => this.#receive(JSON.parse(event.data)));
  }

  static async attach(port) {
    const deadline = Date.now() + 15_000;
    while (Date.now() < deadline) {
      try {
        const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
        const page = targets.find((target) => target.type === "page");
        if (page) {
          const socket = new WebSocket(page.webSocketDebuggerUrl);
          await new Promise((ready, failed) => {
            socket.addEventListener("open", ready, { once: true });
            socket.addEventListener("error", failed, { once: true });
          });
          return new Session(socket);
        }
      } catch {
        /* Chrome todavía no ha abierto el puerto de depuración. */
      }
      await new Promise((r) => setTimeout(r, 300));
    }
    throw new Error(`Chrome no expuso su puerto de depuración (${port})`);
  }

  #receive(message) {
    if (message.id !== undefined) {
      const settle = this.#pending.get(message.id);
      if (settle) {
        this.#pending.delete(message.id);
        settle(message);
      }
      return;
    }
    if (message.method === "Runtime.exceptionThrown") {
      this.consoleErrors.push(`excepción: ${message.params.exceptionDetails.text}`);
    }
    if (message.method === "Log.entryAdded" && message.params.entry.level === "error") {
      this.consoleErrors.push(`consola: ${message.params.entry.text}`);
    }
  }

  send(method, params = {}) {
    return new Promise((settle) => {
      const id = ++this.#nextId;
      this.#pending.set(id, settle);
      this.#socket.send(JSON.stringify({ id, method, params }));
    });
  }

  /** Evalúa en la página y devuelve el valor, o lanza si la página lanzó. */
  async evaluate(expression) {
    const response = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    const details = response.result?.exceptionDetails;
    if (details) {
      throw new Error(`la página lanzó: ${details.exception?.description ?? details.text}`);
    }
    return response.result?.result?.value;
  }

  close() {
    this.#socket.close();
  }
}

/* ---- Comparación: mismas reglas que crates/ms-wasm/tests/corpus.rs ---------- */

function compare(expected, document) {
  const problems = [];

  if (expected.error) {
    if (document.error?.kind !== expected.error) {
      problems.push(`esperaba error «${expected.error}», salió «${document.error?.kind ?? "ninguno"}»`);
    }
    if (document.result !== null) problems.push("devolvió resultado además del error");
  } else {
    if (document.error) problems.push(`falló inesperadamente: ${document.error.kind}`);
    const result = document.result;
    if (!result) {
      problems.push("no devolvió resultado");
    } else {
      if (result.latex !== expected.result_latex) {
        problems.push(`valor «${result.latex}», esperaba «${expected.result_latex}»`);
      }
      if (expected.kind && result.kind !== expected.kind) {
        problems.push(`tipo «${result.kind}», esperaba «${expected.kind}»`);
      }
      const approx = result.numeric?.approx ?? null;
      if (expected.approx === null || expected.approx === undefined) {
        if (approx !== null) problems.push(`trae una aproximación (${approx}) que no debería`);
      } else if (approx === null) {
        problems.push("no trae aproximación");
      } else if (Math.abs(approx - expected.approx) > APPROX_TOLERANCE) {
        problems.push(`${approx} no aproxima a ${expected.approx}`);
      }
    }
  }

  const steps = document.trace.map((step) => step.rule);
  const wanted = expected.steps ?? [];
  if (steps.join(",") !== wanted.join(",")) {
    problems.push(`derivación [${steps.join(", ")}], esperaba [${wanted.join(", ")}]`);
  }

  /* Misma regla que «verification»: opcional, y quien lo declara lo fija entero,
     incluido el orden — la interfaz pinta los hechos en el orden en que llegan. */
  if (expected.facts !== undefined) {
    const produced = canonical(document.facts ?? []);
    const wanted = canonical(expected.facts);
    if (produced !== wanted) {
      problems.push(`hechos ${produced}, esperaba ${wanted}`);
    }
  }

  /* Opcional: un caso que no lo declara no afirma nada sobre su verificación.
     Los que lo declaran impiden que una regla nueva rompa la comprobación en
     silencio, que es justo lo que ms-verify existe para detectar. */
  if (expected.verification && document.verification?.status !== expected.verification) {
    problems.push(
      `verificación «${document.verification?.status ?? "ninguna"}», esperaba «${expected.verification}»`,
    );
  }

  return problems;
}

/* JSON con las claves ordenadas. serde escribe los campos en el orden en que
   están declarados y el corpus los guarda alfabéticamente: comparar las cadenas
   sin normalizar haría fallar documentos idénticos. */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  const entries = Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`);
  return `{${entries.join(",")}}`;
}

/* ---- El programa ------------------------------------------------------------ */

/*
  Comandos de LaTeX que, si aparecen como TEXTO visible, significan que algo se
  imprimió en vez de renderizarse. No es una lista exhaustiva de LaTeX: es la
  lista de lo que este motor emite, que es lo único que puede escaparse.
*/
const LATEX_LEAK = String.raw`\\(frac|left|right|operatorname|cdot|overline|pm|text)\b|\^\{|_\{`;

/*
  Ejecuta todos los casos de la página y busca LaTeX sin renderizar.

  KaTeX guarda el fuente en un `<annotation encoding="application/x-tex">` dentro
  de `.katex-mathml`, así que ahí el LaTeX crudo es correcto y se salta: lo que
  se busca es texto suelto FUERA de un `.katex`.

  Hay que pulsar los botones porque los resultados no existen hasta entonces, y
  ése es justo el estado que nadie estaba mirando.
*/
async function checkRenderedLatex(session) {
  // Sólo los cerrados, y por rondas: abrir una sección revela las de dentro.
  //
  // Pulsar todos los botones a ciegas **cerraba** los que ya estaban abiertos
  // —el árbol abre por defecto las ramas pequeñas— y la comprobación pasaba en
  // verde sobre una página sin nada desplegado. Falso negativo, que de las dos
  // formas de fallar es la mala: un check que no puede fallar no comprueba nada.
  for (let round = 0; round < EXPAND_ROUNDS; round += 1) {
    const opened = await session.evaluate(`(() => {
      const closed = [...document.querySelectorAll('button[aria-expanded="false"]')];
      for (const button of closed) button.click();
      return closed.length;
    })()`);
    await new Promise((r) => setTimeout(r, 300));
    if (opened === 0) break;
  }

  const runs = await session.evaluate(`(() => {
    const buttons = [...document.querySelectorAll("button")].filter((button) =>
      button.textContent.includes("Ejecutar"),
    );
    for (const button of buttons) button.click();
    return buttons.length;
  })()`);
  if (runs === 0) {
    throw new Error(
      "no se desplegó ningún caso, así que no había nada que mirar. " +
        "Un check que no puede fallar no comprueba nada.",
    );
  }
  await new Promise((r) => setTimeout(r, PAGE_SETTLE_MS));

  const encoded = await session.evaluate(`(() => {
    const pattern = new RegExp(${JSON.stringify(LATEX_LEAK)});
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const found = [];
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = node.textContent.trim();
      if (!text || !pattern.test(text)) continue;
      if (node.parentElement?.closest(".katex")) continue;
      found.push(text.slice(0, 90));
    }
    return JSON.stringify([...new Set(found)]);
  })()`);
  return JSON.parse(encoded);
}

/* ---- El editor de matemáticas de /anotaciones -------------------------------- */

/*
  Lo que se escribe en el bloque `$$…$$` tiene que llegar al motor tal y como el
  parser lo lee. Entre lo uno y lo otro hay tres traducciones que ningún test de
  Rust ve: la macro de MathLive que hace el átomo, `getValue("latex-expanded")`, y
  la normalización de `mathlive-latex.ts`.

  Las tres han fallado ya:

  - configurar el campo antes de meterlo en el DOM rompía el montaje entero
    (`inlineShortcuts` lanza «Mathfield not mounted») y el bloque salía con su
    paleta y ni una línea donde escribir;
  - `latex-expanded` escribe `\operatorname{\mathrm{expandir}}`, que el lexer
    rechaza porque exige letras pegadas dentro de las llaves.

  Ninguna de las dos rompe nada visible al escribir: rompen al resolver, y parecen
  un motor que no sabe expandir. De ahí este caso.
*/
const NOTES_PATH = "/anotaciones";
const MATHLIVE_MODULE = "/src/components/notes/math/mathlive-latex.ts";
/** La consulta del editor, y lo que el motor tiene que contestar. */
const EDITOR_CASE = { latex: "\\left(x-2\\right)\\left(x-3\\right)", result: "x^{2} - 5x + 6" };
/** MathLive se carga con `import()` al abrir el primer bloque de matemáticas. */
const MATHLIVE_LOAD_MS = 4000;

async function checkMathEditor(session, base) {
  // Un documento sembrado, y no la interfaz de creación: lo que se prueba es el
  // editor, no el menú «/». El perfil de Chrome es temporal, así que este
  // localStorage no es el de nadie.
  await session.evaluate(`(() => {
    localStorage.setItem("mathslice-notes", JSON.stringify([{
      id: "browser-check",
      title: "browser-check",
      updatedAt: Date.now(),
      blocks: [{ id: "math", type: "text", content: "$$" + ${JSON.stringify(EDITOR_CASE.latex)} + "$$" }],
    }]));
  })()`);
  await session.send("Page.navigate", { url: `${base}${NOTES_PATH}` });
  await new Promise((r) => setTimeout(r, PAGE_SETTLE_MS));

  // Abrir el documento y luego la ecuación. Los dos son `<button>`/`onClick` de
  // React, así que `.click()` es el mismo camino que un clic de verdad.
  const opened = await session.evaluate(`(async () => {
    const card = [...document.querySelectorAll("button")].find((button) =>
      button.textContent.trim().startsWith("browser-check"),
    );
    if (!card) return "no apareció el documento sembrado en la lista";
    card.click();
    await new Promise((r) => setTimeout(r, 800));

    // El documento puede abrir el bloque ya en edición o en modo lectura. Si está
    // leyendo, se pulsa la ecuación; si ya está editando, no hay nada que pulsar.
    if (document.querySelectorAll("math-field").length === 0) {
      const line = document.querySelector("[data-block-id] div.cursor-text");
      if (!line) return "el bloque no renderizó la ecuación ni abrió el editor";
      line.click();
      await new Promise((r) => setTimeout(r, ${MATHLIVE_LOAD_MS}));
    }

    return document.querySelectorAll("math-field").length;
  })()`);

  if (typeof opened === "string") return [opened];
  if (opened === 0) {
    return ["el bloque de ecuación no montó ningún <math-field> donde escribir"];
  }

  const encoded = await session.evaluate(`(async () => {
    const field = document.querySelector("math-field");
    // El mismo camino que la paleta: plantilla en forma corta, que es la que
    // MathLive convierte en átomo.
    field.value = "";
    field.insert("\\\\expandir\\\\left(#?\\\\right)", {
      insertionMode: "replaceSelection",
      selectionMode: "placeholder",
      focus: true,
    });
    field.insert(${JSON.stringify(EDITOR_CASE.latex)});

    const { toDocument } = await import(${JSON.stringify(MATHLIVE_MODULE)});
    const stored = toDocument(field.getValue("latex-expanded"));

    const engine = await import(${JSON.stringify(RUNNER_MODULE)});
    const [readable] = await engine.probe([stored]);
    const document_ = readable ? await engine.solve(stored) : null;
    return JSON.stringify({ stored, readable, result: document_?.result?.latex ?? null, error: document_?.error?.kind ?? null });
  })()`);
  const { stored, readable, result, error } = JSON.parse(encoded);

  const problems = [];
  if (!stored.includes("\\operatorname{expandir}")) {
    problems.push(`lo que se guardaría no es la forma que lee el motor: «${stored}»`);
  }
  if (!readable) problems.push(`el motor no sabe leer lo que el editor escribe: «${stored}»`);
  if (error) problems.push(`resolver falló con «${error}»`);
  if (readable && result !== EDITOR_CASE.result) {
    problems.push(`el editor resolvió «${result}», esperaba «${EDITOR_CASE.result}»`);
  }
  return problems;
}

async function main() {
  const { base } = parseArguments(process.argv.slice(2));
  const cases = await loadCorpus();
  await waitForServer(base);

  const profile = mkdtempSync(join(tmpdir(), "ms-browser-check-"));
  const chrome = spawn(
    findChrome(),
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      `--user-data-dir=${profile}`,
      `--remote-debugging-port=${DEBUG_PORT}`,
      "about:blank",
    ],
    { stdio: "ignore" },
  );

  let session;
  try {
    session = await Session.attach(DEBUG_PORT);
    await session.send("Runtime.enable");
    await session.send("Log.enable");
    await session.send("Page.enable");
    await session.send("Page.navigate", { url: `${base}${LAB_PATH}` });
    await new Promise((r) => setTimeout(r, PAGE_SETTLE_MS));

    // La isla tiene que hidratar. Si no, el motor puede estar perfecto y el
    // estudiante seguir viendo una página muerta — que es justo lo que pasaba
    // con la caché rancia de Vite, y otra vez cuando la isla arrastró `node:fs`
    // al navegador: la página entera renderizada y ni un clic que respondiera.
    //
    // Astro quita el atributo `ssr` del `<astro-island>` al hidratarlo, así que
    // eso es la señal. Buscar un elemento del DOM no serviría: el servidor los
    // renderiza igual, y el marcado de una isla muerta es idéntico al de una viva.
    const hydrated = await session.evaluate(`(() => {
      const island = [...document.querySelectorAll("astro-island")].find((element) =>
        (element.getAttribute("component-url") ?? "").includes(${JSON.stringify(ISLAND_COMPONENT)}),
      );
      return !!island && !island.hasAttribute("ssr");
    })()`);
    if (!hydrated) {
      throw new Error(
        `la isla ${ISLAND_COMPONENT} de ${LAB_PATH} no hidrató.\n` +
          `Errores de consola:\n  ${session.consoleErrors.join("\n  ") || "ninguno"}`,
      );
    }

    const encoded = await session.evaluate(`(async () => {
      const engine = await import(${JSON.stringify(RUNNER_MODULE)});
      const documents = [];
      for (const latex of ${JSON.stringify(cases.map((entry) => entry.latex))}) {
        documents.push(await engine.solve(latex));
      }
      return JSON.stringify(documents);
    })()`);
    const documents = JSON.parse(encoded);

    let failures = 0;
    for (const [index, expected] of cases.entries()) {
      const problems = compare(expected, documents[index]);
      if (problems.length === 0) {
        console.log(`  ok   ${expected.stage} · ${expected.name}`);
        continue;
      }
      failures += 1;
      console.log(`  FALLA ${expected.stage} · ${expected.name}   «${expected.latex}»`);
      for (const problem of problems) console.log(`         ${problem}`);
    }

    // Un error de consola no rompe un caso concreto, pero sí la confianza en la
    // página: se reporta y cuenta como fallo.
    if (session.consoleErrors.length > 0) {
      failures += 1;
      console.log(`  FALLA la consola tiene errores:`);
      for (const problem of session.consoleErrors) console.log(`         ${problem}`);
    }

    // Y lo que la página **pinta**, que hasta aquí nadie miraba: el bucle de
    // arriba llama a `solve()` y compara datos, así que un resultado correcto
    // impreso como LaTeX crudo pasaba en verde. Es el defecto número quince de
    // esa familia y el primero que este script atrapa solo.
    const leaks = await checkRenderedLatex(session);
    if (leaks.length > 0) {
      failures += 1;
      console.log(`  FALLA hay LaTeX sin renderizar en la página:`);
      for (const leak of leaks) console.log(`         ${leak}`);
    }

    // El editor va al final porque navega fuera de /motor: lo que se comprueba
    // arriba ya está medido cuando esto cambia de página.
    const editorProblems = await checkMathEditor(session, base);
    if (editorProblems.length > 0) {
      failures += 1;
      console.log(`  FALLA el editor de matemáticas de ${NOTES_PATH}:`);
      for (const problem of editorProblems) console.log(`         ${problem}`);
    } else {
      console.log(`  ok   ${NOTES_PATH} · lo que escribe el editor lo lee el motor`);
    }

    console.log();
    if (failures > 0) {
      console.log(`${failures} fallo(s) de ${cases.length} casos en el navegador.`);
      process.exitCode = 1;
      return;
    }
    console.log(
      `${cases.length} casos del corpus en verde contra el worker y el WASM reales, ` +
        `nada de LaTeX crudo en la página, y el editor escribiendo lo que el motor lee.`,
    );
  } finally {
    session?.close();
    chrome.kill();
    // Chrome sigue escribiendo en su perfil un rato después del kill, y borrarlo
    // antes de que salga da ENOTEMPTY. Un fallo de limpieza no puede tumbar el
    // gate: el directorio es temporal y lo barre el sistema.
    await Promise.race([
      new Promise((exited) => chrome.once("exit", exited)),
      new Promise((r) => setTimeout(r, 3000)),
    ]);
    try {
      rmSync(profile, { recursive: true, force: true });
    } catch {
      /* Perfil temporal: que lo limpie el sistema. */
    }
  }
}

try {
  await main();
} catch (error) {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
