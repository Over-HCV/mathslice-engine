# Inventario de capacidades matemáticas

## Análisis: qué tomaría replicar el motor (solo nivel matemático)

> **Este análisis es anterior a la decisión de arquitectura, y se conserva por lo
> que mide, no por lo que recomienda.** Lo que vale es el dimensionado de la
> superficie —1709 hojas, 751 de índice curricular, 958 reales— y el reparto por
> dificultad, que es lo que ordena el roadmap. Lo que **no** vale es su conclusión
> técnica: aquí se lee «envoltorio de despacho sobre SymPy/SciPy», y lo que se
> decidió construir es un **kernel simbólico propio en Rust**, con SymPy usado
> únicamente como oráculo en CI. El motivo está en
> [`../architecture/02-architecture.md`](../architecture/02-architecture.md): un
> envoltorio devuelve el resultado, y este proyecto existe para devolver **los
> pasos**, que es justo lo que no sobrevive a llamar a `sympy.solve()`.
>
> Los meses estimados abajo tampoco se transfieren: son los de envolver
> librerías maduras, no los de escribir el kernel. El calendario real es el de
> [`../architecture/04-roadmap.md`](../architecture/04-roadmap.md), por etapas.

Alcance de este análisis: únicamente la capa de cómputo matemático (motor simbólico/numérico, solvers, ploteo). Queda fuera a propósito todo lo que un sistema de cálculo hace *antes* de llegar a ese motor — entender lenguaje natural, desambiguar la consulta, elegir qué "pod" mostrar — que es un problema aparte (y más grande) que el de resolver la matemática en sí.

El árbol de abajo es un inventario completo de lo que un sistema de cálculo general cubre, no una estimación a ojo: 21 categorías, 1709 capacidades hoja (el nivel "Add numbers:", "Solve a polynomial equation:", etc.). De esas 1709, **751 viven bajo Common Core Math** — que no es una categoría de cómputo nueva, es un índice curricular K-12 que re-etiqueta por grado escolar capacidades que ya están contadas en Arithmetic, Algebra, Geometry, Statistics, etc. Replicar esa categoría, una vez existen las demás, es un trabajo de mapeo/tabla de contenidos (días, no meses), no de ingeniería nueva. La superficie real del motor es entonces **~958 capacidades** repartidas en las otras 20 categorías.

**Por qué la mayoría es más barata de lo que parece:** salvo Common Core Math, casi todas las categorías tienen equivalentes maduros en el ecosistema Python/OSS (SymPy para álgebra simbólica y cálculo, NumPy/SciPy para numérico y estadística, mpmath para funciones especiales y precisión arbitraria, NetworkX para teoría de grafos, Matplotlib/Plotly para ploteo). El trabajo real no es "inventar un CAS desde cero" para la mayoría de los ejemplos — es construir la capa de despacho (parsear la expresión matemática, decidir qué función de qué librería llamar, formatear la salida) sobre esas librerías.

**Desglose por dificultad real (no por tamaño de categoría):**

- [ ] **Nivel 1 — envoltorio directo sobre OSS existente** (~500 hojas: Elementary Math 36, Statistics 30, Probability 45, Plotting & Graphics 23, Linear Algebra 63, Trigonometry 16, Numbers 58, y la mitad "fácil" de Algebra ~60 de 120 — resolución de ecuaciones simples, simplificación, matrices básicas). SymPy/SciPy/NumPy/Matplotlib cubren esto casi de fábrica. Estimado: **2-3 meses** con un equipo pequeño (2-3 ingenieros), mayormente construyendo la capa de despacho, parser de notación matemática y formato de salida — no los algoritmos en sí.

- [ ] **Nivel 2 — necesita trabajo propio encima de las librerías** (~300 hojas: el resto de Algebra ~60, Number Theory 39, Complex Analysis 16, Discrete Mathematics 65, Mathematical Functions 82, Geometry 114). Las librerías cubren el núcleo pero las propiedades "con nombre" que un sistema de cálculo reporta (propiedades de un polígono específico, identidades de funciones especiales, teoría de grupos finitos) exigen lógica adicional no trivial encima de SymPy/NetworkX/mpmath. Estimado: **4-6 meses** adicionales.

- [ ] **Nivel 3 — ingeniería difícil, sin equivalente OSS a la profundidad de un CAS comercial** (Calculus & Analysis 149, Differential Equations 9, Applied Mathematics 56, Logic & Set Theory 16). Acá es donde se nota la diferencia real: SymPy resuelve integrales/ODEs con heurísticas, pero la robustez de un CAS comercial maduro frente a casos borde es producto de +35 años de ingeniería acumulada en su kernel — no algo que se cierre en meses. Con este alcance, la meta realista no es "100% paridad" sino **~80% de cobertura del set de ejemplos concreto**, y llegar ahí toma **6-12+ meses** solo en este nivel.

- [ ] **Fuera de la categoría "algoritmo": Famous Math Problems (6) y Mathematical Definitions (5)**. Esto no es cómputo, es una mini-enciclopedia curada (definiciones, contexto histórico, estado de problemas abiertos). Es trabajo de contenido con revisión de un experto en el tema, no de ingeniería — unas semanas de escritura/curación, no de desarrollo.

**Total estimado:** un equipo pequeño (2-3 ingenieros senior + apoyo puntual de contenido matemático) construyendo sobre SymPy/SciPy/NumPy/mpmath/NetworkX/Matplotlib —no desde cero— llega a **~80% del set de ejemplos concreto, solo nivel matemático, sin NLU**, en aproximadamente **12-18 meses**. Cerrar el resto (robustez real frente a entradas arbitrarias, no solo los ejemplos listados) es donde el esfuerzo deja de ser lineal y empieza a acercarse a lo que realmente costó construir un CAS comercial: años, no meses.

## Árbol de capacidades

21 categorías, 1709 capacidades hoja.

<!-- [ ] engine-tree:start -->
@import "elementary-math.md"
@import "algebra.md"
@import "calculus-and-analysis.md"
@import "geometry.md"
@import "differential-equations.md"
@import "plotting-and-graphics.md"
@import "numbers.md"
@import "trigonometry.md"
@import "linear-algebra.md"
@import "number-theory.md"
@import "discrete-mathematics.md"
@import "complex-analysis.md"
@import "applied-mathematics.md"
@import "logic-and-set-theory.md"
@import "mathematical-functions.md"
@import "mathematical-definitions.md"
@import "famous-math-problems.md"
@import "continued-fractions.md"
@import "statistics.md"
@import "probability.md"
@import "common-core-math.md"
<!-- [ ] engine-tree:end -->
