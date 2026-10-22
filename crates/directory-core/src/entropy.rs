//! Injected randomness and id generation.

use crate::encoding::hex;

/// Source of cryptographically secure random bytes (injected per runtime).
pub trait Entropy {
    fn fill(&self, buf: &mut [u8]);
}

/// A random 128-bit id as 32 lowercase hex characters.
pub fn new_id<E: Entropy + ?Sized>(entropy: &E) -> String {
    let mut buf = [0u8; 16];
    entropy.fill(&mut buf);
    hex(&buf)
}

/// True for ids produced by [`new_id`]; used to reject malformed path params early.
pub fn is_id(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
pub(crate) mod testing {
    use super::Entropy;
    use std::cell::Cell;

    /// Deterministic entropy: an incrementing byte stream.
    #[derive(Default)]
    pub struct CountingEntropy(pub Cell<u8>);

    impl Entropy for CountingEntropy {
        fn fill(&self, buf: &mut [u8]) {
            for b in buf {
                let next = self.0.get().wrapping_add(1);
                self.0.set(next);
                *b = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::CountingEntropy;
    use super::*;

    #[test]
    fn ids_are_hex_and_distinct() {
        let e = CountingEntropy::default();
        let a = new_id(&e);
        let b = new_id(&e);
        assert!(is_id(&a) && is_id(&b));
        assert_ne!(a, b);
        assert!(!is_id("XYZ") && !is_id(&"g".repeat(32)));
    }
}
