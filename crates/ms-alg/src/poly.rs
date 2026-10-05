//! Polinomios univariados ralos, genéricos en el anillo de coeficientes.
//!
//! **Aquí no aparece ningún tipo numérico concreto.** Ni `Integer`, ni
//! `Rational`, ni `BigInt`: si uno se cuela, es un bug
//! (01-rust-principles.md §4). Lo que E2 necesita hoy son coeficientes
//! racionales; lo que E4 necesitará es el mismo código sobre 𝔽ₚ para factorizar
//! módulo varios primos y levantar por Hensel, y ése es el algoritmo que no se
//! puede permitir estar escrito dos veces.

use ms_num::{Coefficient, Field};

/// Un polinomio en una variable, guardado como `(exponente, coeficiente)`
/// ordenado por exponente creciente y **sin coeficientes nulos**.
///
/// Ralo y no denso porque lo que entra viene de una expresión que el estudiante
/// escribió: `x^{100}+1` tiene dos términos, no ciento uno.
///
/// La invariante —ordenado, sin ceros— la mantienen todos los constructores, y
/// es lo que hace que comparar dos polinomios sea comparar dos vectores y que
/// `degree` sea mirar el último.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Poly<C> {
    terms: Vec<(u32, C)>,
}

impl<C: Coefficient> Poly<C> {
    /// El polinomio nulo, que es el que no tiene términos.
    pub fn zero() -> Self {
        Self { terms: Vec::new() }
    }

    /// Una constante. La nula sale sin términos, como debe.
    pub fn constant(value: C) -> Self {
        if value.is_zero() {
            return Self::zero();
        }
        Self {
            terms: vec![(0, value)],
        }
    }

    /// `x`.
    pub fn variable() -> Self {
        Self {
            terms: vec![(1, C::one())],
        }
    }

    /// `coefficient · x^exponent`.
    pub fn monomial(exponent: u32, coefficient: C) -> Self {
        if coefficient.is_zero() {
            return Self::zero();
        }
        Self {
            terms: vec![(exponent, coefficient)],
        }
    }

    /// Construye desde términos en cualquier orden y con repeticiones: los suma,
    /// los ordena y quita los que se anulan.
    ///
    /// Es la puerta por la que entra lo que viene del puente, donde
    /// `x + 2x - 3x` llega como tres términos distintos del mismo grado.
    pub fn from_terms(terms: impl IntoIterator<Item = (u32, C)>) -> Self {
        let mut collected: Vec<(u32, C)> = Vec::new();

        for (exponent, coefficient) in terms {
            match collected.iter_mut().find(|(power, _)| *power == exponent) {
                Some((_, accumulated)) => {
                    *accumulated = accumulated.clone() + coefficient;
                }
                None => collected.push((exponent, coefficient)),
            }
        }

        collected.retain(|(_, coefficient)| !coefficient.is_zero());
        collected.sort_by_key(|(exponent, _)| *exponent);
        Self { terms: collected }
    }

    /// Los términos, de menor a mayor exponente. Nunca hay coeficientes nulos.
    pub fn terms(&self) -> &[(u32, C)] {
        &self.terms
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    /// El grado, o `None` para el polinomio nulo.
    ///
    /// `None` y no `0` porque el grado del polinomio nulo **no es cero**: la
    /// convención habitual es −∞, y devolver 0 lo haría indistinguible de una
    /// constante justo donde importa —al dividir, al factorizar, al contar
    /// raíces—. Un `Option` obliga a decidir qué hacer con el caso vacío.
    pub fn degree(&self) -> Option<u32> {
        self.terms.last().map(|(exponent, _)| *exponent)
    }

    /// El coeficiente de `x^exponent`, cero si no está.
    pub fn coefficient(&self, exponent: u32) -> C {
        self.terms
            .iter()
            .find(|(power, _)| *power == exponent)
            .map_or_else(C::zero, |(_, coefficient)| coefficient.clone())
    }

    /// El coeficiente del término de mayor grado, o `None` si es nulo.
    pub fn leading_coefficient(&self) -> Option<C> {
        self.terms
            .last()
            .map(|(_, coefficient)| coefficient.clone())
    }

    pub fn add(&self, other: &Self) -> Self {
        Self::from_terms(self.terms.iter().chain(other.terms.iter()).cloned())
    }

    pub fn neg(&self) -> Self {
        Self {
            terms: self
                .terms
                .iter()
                .map(|(exponent, coefficient)| (*exponent, -coefficient.clone()))
                .collect(),
        }
    }

    pub fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }

    pub fn mul(&self, other: &Self) -> Self {
        let mut products = Vec::with_capacity(self.terms.len() * other.terms.len());

        for (left_exponent, left) in &self.terms {
            for (right_exponent, right) in &other.terms {
                products.push((
                    left_exponent + right_exponent,
                    left.clone() * right.clone(),
                ));
            }
        }

        Self::from_terms(products)
    }

    /// `self` evaluado en `point`, por Horner sobre la forma densa.
    pub fn evaluate(&self, point: &C) -> C {
        let Some(degree) = self.degree() else {
            return C::zero();
        };

        let mut accumulated = C::zero();
        for exponent in (0..=degree).rev() {
            accumulated = accumulated * point.clone() + self.coefficient(exponent);
        }
        accumulated
    }
}

impl<C: Field> Poly<C> {
    /// División euclídea: `self = divisor · cociente + resto`, con el resto de
    /// grado menor que el divisor.
    ///
    /// Pide [`Field`] y no [`Coefficient`] porque el algoritmo divide entre el
    /// coeficiente principal del divisor en cada paso. Sobre ℤ eso no siempre da
    /// un entero —`x^{2}` entre `2x` no tiene cociente entero— y por eso `Field`
    /// es la cota correcta y no una precaución: sobre un anillo cualquiera la
    /// división con resto **no existe**, y exigirla en el tipo evita escribir un
    /// algoritmo que devolvería una respuesta incorrecta la mitad de las veces.
    ///
    /// `None` sólo cuando el divisor es nulo.
    pub fn divide(&self, divisor: &Self) -> Option<(Self, Self)> {
        let divisor_degree = divisor.degree()?;
        let leading = divisor.leading_coefficient()?.inv()?;

        let mut remainder = self.clone();
        let mut quotient_terms: Vec<(u32, C)> = Vec::new();

        while let Some(remainder_degree) = remainder.degree() {
            if remainder_degree < divisor_degree {
                break;
            }
            // `?` en vez de indexar: el grado viene de `terms.last()`, así que el
            // coeficiente existe, pero afirmarlo con un unwrap sería confiar en
            // una invariante en vez de comprobarla (§6).
            let scale = remainder.leading_coefficient()? * leading.clone();
            let shift = remainder_degree - divisor_degree;

            let step = Self::monomial(shift, scale.clone());
            quotient_terms.push((shift, scale));
            remainder = remainder.sub(&divisor.mul(&step));
        }

        Some((Self::from_terms(quotient_terms), remainder))
    }

    /// El polinomio dividido entre su coeficiente principal, para que el de mayor
    /// grado sea 1. El nulo se queda como está.
    pub fn monic(&self) -> Option<Self> {
        let Some(leading) = self.leading_coefficient() else {
            return Some(Self::zero());
        };
        let inverse = leading.inv()?;
        Some(Self {
            terms: self
                .terms
                .iter()
                .map(|(exponent, coefficient)| {
                    (*exponent, coefficient.clone() * inverse.clone())
                })
                .collect(),
        })
    }
}
