//! Whitespace normalization for extracted ICU messages.
//!
//! Mirrors `utils.js` `getICUMessageValue`:
//!   message = message.trim().replace(/\s+/gm, ' ');
//!
//! Caveat: JS `\s` matches `[\r\n\t\f\v ]` plus various Unicode whitespace
//! code points; Rust's `char::is_whitespace` is also Unicode-aware. The two
//! definitions are very close but not identical (e.g. ZWNBSP `\u{FEFF}` is
//! whitespace under Unicode but not under JS `\s` prior to ES2018). For
//! plain ASCII messages — the overwhelmingly common case — they agree.
//! If a fixture surfaces a divergence, port the exact JS character class.

pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_runs_and_trims() {
        assert_eq!(normalize_whitespace("  hello   world  "), "hello world");
    }

    #[test]
    fn newlines_become_spaces() {
        assert_eq!(normalize_whitespace("a\n\nb\tc"), "a b c");
    }
}
