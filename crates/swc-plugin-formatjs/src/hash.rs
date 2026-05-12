//! ID interpolation, parity-locked to `@formatjs/ts-transformer`'s
//! `interpolateName` -> Node's `crypto.createHash(...).digest(...).slice(0, N)`.
//!
//! Reference: `node_modules/@formatjs/ts-transformer/src/interpolate-name.js`.

use crate::fail;
use base64::Engine;
use regex::Regex;
use sha2::{Digest, Sha512};
use std::sync::OnceLock;
use swc_core::common::DUMMY_SP;

/// `[<hashType>:(hash|contenthash):<digestType>:<length>]` — every group
/// after `hash|contenthash` is optional.
fn hash_token_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\[(?:([^:\]]+):)?(?:hash|contenthash)(?::([a-z]+\d*[a-z]*))?(?::(\d+))?\]",
        )
        .expect("hash interpolation regex must compile")
    })
}

/// Matches any `[token]` group. Used by `validate_pattern` to detect
/// unsupported interpolation tokens (e.g. `[name]`, `[ext]`, `[path]`).
fn any_token_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]]*\]").expect("any-token regex must compile"))
}

/// Hard-error if the pattern contains tokens beyond `[hash...]` / `[contenthash...]`.
///
/// Why: babel's `interpolateName` also supports `[name]`, `[ext]`, `[path]`,
/// `[folder]`, `[query]` (substituted from the source file path). We don't —
/// supporting them properly would require threading file metadata through
/// the visitor. Hard-erroring beats silently leaving the unreplaced token
/// in the generated id and quietly breaking translation catalogs.
pub fn validate_pattern(pattern: &str) {
    let hash_re = hash_token_regex();
    for m in any_token_regex().find_iter(pattern) {
        if !hash_re.is_match(m.as_str()) {
            fail(
                DUMMY_SP,
                format!(
                    "idInterpolationPattern token `{}` is not supported. \
                     Only `[<hashType>:(hash|contenthash):<digestType>:<length>]` is implemented. \
                     If you need `[name]`/`[ext]`/`[path]` substitution, extend the plugin.",
                    m.as_str()
                ),
            );
        }
    }
}

pub fn interpolate_pattern(pattern: &str, content: &str) -> String {
    hash_token_regex()
        .replace_all(pattern, |caps: &regex::Captures| {
            let hash_type = caps.get(1).map(|m| m.as_str()).unwrap_or("md5");
            let digest_type = caps.get(2).map(|m| m.as_str()).unwrap_or("hex");
            // babel uses `parseInt(undefined, 10) === NaN`, then
            // `String.slice(0, NaN) === ""`. `getHashDigest`'s own default is
            // 9999 (effectively "no truncation") — we follow that path.
            let length = caps
                .get(3)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(9999);
            hash_digest(content, hash_type, digest_type, length)
        })
        .into_owned()
}

fn hash_digest(content: &str, hash_type: &str, digest_type: &str, length: usize) -> String {
    let bytes: Vec<u8> = match hash_type {
        "sha512" => {
            let mut h = Sha512::new();
            h.update(content.as_bytes());
            h.finalize().to_vec()
        }
        other => fail(
            DUMMY_SP,
            format!(
                "hash type `{}` is not supported. Default is `sha512`. \
                 If you need md5/sha1/sha256, add the appropriate crate and a branch in hash.rs.",
                other
            ),
        ),
    };

    let encoded = match digest_type {
        "base64" => base64::engine::general_purpose::STANDARD.encode(&bytes),
        "hex" => hex_lower(&bytes),
        other => fail(
            DUMMY_SP,
            format!(
                "digest type `{}` is not supported. Default is `base64`; `hex` also works.",
                other
            ),
        ),
    };

    // Node's `String.prototype.slice(0, N)` operates on UTF-16 code units;
    // base64/hex are ASCII so byte-slice == char-slice == code-unit-slice.
    encoded.chars().take(length).collect()
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-checked against Node:
    ///   crypto.createHash('sha512').update('Hello, {name}!').digest('base64').slice(0,6)
    #[test]
    fn sha512_base64_6_matches_node() {
        let id = interpolate_pattern("[sha512:contenthash:base64:6]", "Hello, {name}!");
        assert_eq!(id, "tBFOH1");
    }

    #[test]
    fn sha512_base64_6_with_description() {
        let id = interpolate_pattern(
            "[sha512:contenthash:base64:6]",
            "Hello, {name}!#Greets the user by name",
        );
        assert_eq!(id, "LlYGNJ");
    }
}
