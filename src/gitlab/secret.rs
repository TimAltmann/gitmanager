//! Geheimer Text (Token) für glab (E3): nur verschieben, nie anzeigen.
//! Kein eigenes Crate (`zeroize`): Drop überschreibt den Puffer nach
//! bestem Bemühen, Kopien in Kind-Env/-stdin liegen außerhalb.

use std::fmt;

/// Token im Speicher. `Debug` zeigt `SecretString(***)`; kein `Display`,
/// kein `Serialize`, kein `Clone` – der Wert wird nur verschoben und geht
/// ausschließlich in Kind-stdin (Login) oder Kind-Env (Prüfung).
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Klartext, nur für Kind-stdin bzw. Kind-Env von glab.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***)")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        // Ohne `unsafe`: Puffer übernehmen und überschreiben; `black_box`
        // verhindert, dass der Compiler das Schreiben wegoptimiert.
        let mut bytes = std::mem::take(&mut self.0).into_bytes();
        bytes.fill(0);
        std::hint::black_box(&bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_value() {
        let secret = SecretString::new("glpat-geheim-1234".to_string());
        let shown = format!("{secret:?}");
        assert_eq!(shown, "SecretString(***)");
        // auch eingebettet und mit `{:#?}`
        let nested = format!("{:#?}", Some(&secret));
        assert!(!nested.contains("geheim"), "{nested}");
    }

    #[test]
    fn expose_returns_value() {
        let secret = SecretString::new("glpat-abc".to_string());
        assert_eq!(secret.expose(), "glpat-abc");
        assert!(!secret.is_empty());
        assert!(SecretString::new(String::new()).is_empty());
    }
}
