use crate::application::ApplicationError;
use serde::Serialize;

/// Represents a validated three-letter ISO-style currency code.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(transparent)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    /// Validates an uppercase three-letter currency code without silently normalizing input.
    pub fn parse(value: &str) -> Result<Self, ApplicationError> {
        let is_valid =
            value.len() == 3 && value.bytes().all(|character| character.is_ascii_uppercase());

        if !is_valid {
            return Err(ApplicationError::validation());
        }

        Ok(Self(value.to_owned()))
    }

    /// Returns the canonical code for display or persistence adapters.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stores money as signed minor units, never as a floating-point value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Money {
    minor_units: i64,
    currency: CurrencyCode,
}

impl Money {
    /// Creates an exact integer money amount in the supplied currency.
    pub fn new(minor_units: i64, currency: CurrencyCode) -> Self {
        Self { minor_units, currency }
    }

    /// Returns the exact stored minor-unit amount.
    pub fn minor_units(&self) -> i64 {
        self.minor_units
    }

    /// Returns the currency associated with this amount.
    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }

    /// Adds an amount only when both operands use the same currency and do not overflow.
    pub fn checked_add(&self, other: &Self) -> Result<Self, ApplicationError> {
        self.require_matching_currency(other)?;
        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or_else(ApplicationError::validation)?;

        Ok(Self::new(minor_units, self.currency.clone()))
    }

    /// Subtracts an amount only when both operands use the same currency and do not overflow.
    pub fn checked_sub(&self, other: &Self) -> Result<Self, ApplicationError> {
        self.require_matching_currency(other)?;
        let minor_units = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or_else(ApplicationError::validation)?;

        Ok(Self::new(minor_units, self.currency.clone()))
    }

    /// Negates an amount while rejecting the one signed-integer value that cannot be negated.
    pub fn checked_negate(&self) -> Result<Self, ApplicationError> {
        let minor_units =
            self.minor_units.checked_neg().ok_or_else(ApplicationError::validation)?;

        Ok(Self::new(minor_units, self.currency.clone()))
    }

    /// Rejects arithmetic that would combine incompatible currency units.
    fn require_matching_currency(&self, other: &Self) -> Result<(), ApplicationError> {
        if self.currency != other.currency {
            return Err(ApplicationError::validation());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CurrencyCode, Money};
    use crate::application::ErrorCode;

    /// Creates a compact USD fixture for money arithmetic tests.
    fn usd(minor_units: i64) -> Money {
        let currency = CurrencyCode::parse("USD").expect("USD is a valid fixture currency");
        Money::new(minor_units, currency)
    }

    #[test]
    fn adds_exact_minor_units_without_rounding() {
        let total = usd(105).checked_add(&usd(-40)).expect("matching currencies add");

        assert_eq!(total.minor_units(), 65);
        assert_eq!(total.currency().as_str(), "USD");
    }

    #[test]
    fn rejects_cross_currency_arithmetic() {
        let eur = Money::new(100, CurrencyCode::parse("EUR").expect("EUR is valid"));
        let error = usd(100).checked_add(&eur).expect_err("currencies must match");

        assert_eq!(error.code, ErrorCode::Validation);
    }

    #[test]
    fn rejects_overflow_and_unnegatable_minimum() {
        let overflow = usd(i64::MAX).checked_add(&usd(1)).expect_err("addition must not overflow");
        let negation =
            usd(i64::MIN).checked_negate().expect_err("minimum signed value cannot be negated");

        assert_eq!(overflow.code, ErrorCode::Validation);
        assert_eq!(negation.code, ErrorCode::Validation);
    }

    #[test]
    fn accepts_only_uppercase_three_letter_currency_codes() {
        assert!(CurrencyCode::parse("USD").is_ok());
        assert!(CurrencyCode::parse("usd").is_err());
        assert!(CurrencyCode::parse("US").is_err());
        assert!(CurrencyCode::parse("US1").is_err());
    }
}
