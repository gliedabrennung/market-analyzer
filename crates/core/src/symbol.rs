use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Symbol(String);

impl Symbol {
    pub fn new(raw: &str) -> Result<Self, CoreError> {
        let is_valid = !raw.is_empty()
            && raw.len() <= 32
            && raw
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        if is_valid {
            Ok(Self(raw.to_string()))
        } else {
            Err(CoreError::InvalidSymbol(raw.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Symbol {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Symbol {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Symbol::new(&value)
    }
}

impl From<Symbol> for String {
    fn from(value: Symbol) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_symbol() {
        assert!(Symbol::new("BTCUSDT").is_ok());
    }

    #[test]
    fn rejects_lowercase() {
        assert!(Symbol::new("btcusdt").is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(Symbol::new("../../etc").is_err());
        assert!(Symbol::new("BTC/USDT").is_err());
    }

    #[test]
    fn rejects_empty_and_too_long() {
        assert!(Symbol::new("").is_err());
        assert!(Symbol::new(&"A".repeat(33)).is_err());
    }
}
