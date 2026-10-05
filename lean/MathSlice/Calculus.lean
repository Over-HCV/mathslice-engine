/-
La corrección de las reglas de derivación del motor, demostrada.

Sobre `Polynomial ℚ` y **no** sobre `deriv` en ℝ, y la elección importa. El motor
deriva expresiones que construye con sumas, productos y potencias de exponente
entero sobre ℚ: eso es exactamente un polinomio (y su localización), no una
función real cualquiera. Enunciar sobre ℝ demostraría un teorema más fuerte del
que respalda la regla, y la lección de E2 —`rational-reciprocal` era cierta sin
hipótesis en Lean y el motor seguía necesitando la guarda— es que un teorema más
fuerte que la regla no la respalda entera.

`Polynomial.derivative` es además una derivada **formal**: no pasa por límites,
igual que el motor. La demostración habla del mismo objeto que el código.

Cada nombre lo escribe una regla en su `#[soundness = lean_local("…")]`, y
`scripts/check-soundness.sh` comprueba que los dos existan.
-/
import Mathlib.Algebra.Polynomial.Derivative

namespace MathSlice

open Polynomial

/--
`derivative-sum`: la derivada de una suma es la suma de las derivadas.

Sin hipótesis, y la regla del motor tampoco tiene ninguna: reparte sobre los
sumandos sin mirar qué son.
-/
theorem derivative_sum_sound (p q : Polynomial ℚ) :
    derivative (p + q) = derivative p + derivative q := by
  simp

/--
`derivative-product`: la regla del producto.

El motor la enuncia con un sumando por factor, que para dos factores es esto
mismo; para más, es esto aplicado en cadena, que es lo que hace el reescritor al
volver a disparar sobre el resultado.
-/
theorem derivative_product_sound (p q : Polynomial ℚ) :
    derivative (p * q) = derivative p * q + p * derivative q := by
  simp [derivative_mul]

/--
`derivative-constant`: la derivada de una constante es cero.

`C a` es exactamente lo que el motor reconoce como constante: una expresión sin
la variable dentro.
-/
theorem derivative_constant_sound (a : ℚ) : derivative (C a) = 0 := by
  simp

/--
`derivative-variable`: la derivada de la variable es uno.
-/
theorem derivative_variable_sound : derivative (X : Polynomial ℚ) = 1 := by
  simp

/--
`derivative-constant-multiple`: la constante sale fuera.

**Es un caso particular de la regla del producto y aun así tiene teorema propio**,
por lo mismo que tiene regla propia: lo que se demuestra es lo que la regla hace,
y la regla hace esto, no aquello.
-/
theorem derivative_constant_multiple_sound (a : ℚ) (p : Polynomial ℚ) :
    derivative (C a * p) = C a * derivative p := by
  simp

/--
`derivative-power`: la potencia de la variable.

`n` es un natural aquí y en la regla puede ser un entero negativo: un exponente
negativo es una fracción y sale del anillo de polinomios. Ese caso **no lo
respalda este teorema**, y decirlo es la mitad del valor de escribirlo.
-/
theorem derivative_power_sound (n : ℕ) :
    derivative ((X : Polynomial ℚ) ^ n) = (n : Polynomial ℚ) * X ^ (n - 1) := by
  simp [derivative_X_pow]

/--
`derivative-chain`: la regla de la cadena para una potencia.

Es la que hace que `\left(x^{2}+1\right)^{3}` se derive sin expandirlo, y la que
más fácil se escribe mal: el exponente que baja y el que queda son distintos.
-/
theorem derivative_chain_sound (p : Polynomial ℚ) (n : ℕ) :
    derivative (p ^ n) = (n : Polynomial ℚ) * p ^ (n - 1) * derivative p := by
  simp [derivative_pow]

end MathSlice
