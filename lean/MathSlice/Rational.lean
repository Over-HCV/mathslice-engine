/-
La corrección de las reglas racionales del motor, demostrada.

Cada teorema enuncia **lo que la regla hace**, no lo que nos gustaría que
hiciera. El nombre del teorema es el que la regla nombra en su
`#[soundness = lean_local("…")]`, y `scripts/check-soundness.sh` comprueba que
los dos existan: un nombre con una errata afirmaría una demostración que no hay.

Sobre ℚ y no sobre un cuerpo cualquiera porque es donde el motor calcula
(`ms_num::Rational`). Generalizar aquí demostraría un teorema más fuerte del que
respalda la regla, y un teorema que no habla del código no lo respalda.
-/
import Mathlib.Data.Rat.Defs
import Mathlib.Tactic.FieldSimp
import Mathlib.Tactic.Ring

namespace MathSlice

/--
`rational-multiply`: multiplicar dos fracciones es multiplicar numeradores y
denominadores.

**La hipótesis no es decorado.** Sin `b ≠ 0` y `d ≠ 0` el enunciado es falso en
Lean —la división por cero vale cero por convención— y esa hipótesis es
exactamente la que el reescritor comprueba antes de disparar la regla, vía
`has_division_by_zero`. Es el §2 del documento funcionando: la demostración
descubre la suposición que hay que exigir.
-/
theorem rational_multiply_sound (a b c d : ℚ) (hb : b ≠ 0) (hd : d ≠ 0) :
    (a / b) * (c / d) = (a * c) / (b * d) := by
  field_simp

/--
`rational-reduce`: simplificar una fracción por un factor común no cambia su
valor.

La única hipótesis que hace falta es que el factor sea no nulo. **No** que lo
sea el denominador, y eso lo descubrió intentar demostrarlo: Lean rechazó `hb`
por no usarse. `\frac{0 \cdot k}{b \cdot k}` sigue valiendo lo que valía, y
simplificar no necesita saber nada de `b`.
-/
theorem rational_reduce_sound (a b k : ℚ) (hk : k ≠ 0) :
    (a * k) / (b * k) = a / b := by
  rw [mul_div_mul_right _ _ hk]

/--
`product-with-zero`: un producto con un factor nulo es cero.

Sin hipótesis, y por eso es la regla más segura del motor: vale para todo
racional, incluido el que no está definido en ninguna otra parte.
-/
theorem product_with_zero_sound (a : ℚ) : a * 0 = 0 := by
  ring

/--
`rational-common-denominator` seguido de `rational-add-numerators`: sumar dos
fracciones es ponerles denominador común y sumar los numeradores.

Las dos reglas del motor son **dos pasos** porque el estudiante tiene que ver
los dos, pero su composición es una sola identidad, y es ésta la que hay que
demostrar: lo que importa es que la pareja conserve el valor.
-/
theorem rational_add_sound (a b c d : ℚ) (hb : b ≠ 0) (hd : d ≠ 0) :
    a / b + c / d = (a * d + c * b) / (b * d) := by
  field_simp

/--
`rational-reciprocal`: dividir entre una fracción es multiplicar por su
recíproco.

Es el primer paso del criterio de salida de E1, y **sale sin ninguna hipótesis**,
lo cual es una trampa que conviene dejar escrita.

Es verdad sin hipótesis porque en Lean `0⁻¹ = 0` por convención, así que la
identidad se cumple también cuando `c = 0`: los dos lados valen cero. El motor
**no puede apoyarse en eso**: para él `\frac{a}{0}` no vale cero, es un error que
sube hasta la interfaz. Por eso `rational-reciprocal` sigue exigiendo
`is_known_nonzero` sobre el numerador antes de disparar, y esa guarda no sobra
aunque el teorema no la pida.

Es el §2 del documento al revés: la demostración no descubrió una hipótesis que
faltaba, descubrió que la hipótesis que la regla exige protege algo que este
teorema no dice. Un teorema más fuerte que la regla no la respalda entera.
-/
theorem rational_reciprocal_sound (a b c d : ℚ) :
    (a / b) / (c / d) = (a / b) * (d / c) := by
  rw [div_eq_mul_inv, inv_div]

end MathSlice
