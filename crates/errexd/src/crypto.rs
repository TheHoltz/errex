//! Password hashing + opaque session ID generation.
//!
//! We use argon2id with the OWASP-blessed defaults (m=19MiB, t=2, p=1).
//! The PHC string format encodes the params alongside the salt and digest,
//! so a future bump to higher params won't invalidate older hashes — we can
//! upgrade on next-login by re-hashing if `password_hash::PasswordHash::parse`
//! reveals an older variant. (Not implemented yet; one-line follow-up.)
//!
//! Session IDs are 256 bits of entropy, hex-encoded — two `Uuid::new_v4()`
//! values concatenated. We don't roll our own RNG to avoid an extra
//! `getrandom` direct dependency; uuid v4 already pulls a CSPRNG via the
//! same crate.
//!
//! Why this lives outside `store`: keeps the SQLx layer free of crypto
//! dependencies, and lets us unit-test hashing without a database.
//!
//! # Constant-time considerations
//!
//! `verify_password` returns `false` on EVERY failure mode (parse error,
//! mismatched digest, unknown algorithm) so the timing characteristic does
//! not differ between "no such user" and "wrong password" at the call site.
//! The caller still needs to do a hash-anyway pass when the username is
//! unknown — see `auth::login` for that pattern.

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use password_hash::{PasswordHash, SaltString};

/// Hash a plaintext password with argon2id + a fresh random salt. Returns
/// the PHC-formatted string suitable for direct storage in a TEXT column.
pub fn hash_password(plain: &str) -> Result<String, password_hash::Error> {
    let salt = SaltString::generate(&mut password_hash::rand_core::OsRng);
    let hash = Argon2::default().hash_password(plain.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a plaintext against a PHC-formatted hash. Returns `false` for any
/// failure — wrong password, unparseable hash, unsupported algorithm. The
/// caller MUST NOT branch on the failure reason: leaking "valid format but
/// wrong password" vs "garbage in the column" gives an attacker a probe.
pub fn verify_password(stored_hash: &str, plain: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// 64-char hex (256 bits of entropy) — fine for opaque session cookies.
/// Two simple-formatted v4 UUIDs concatenated; both pull from a CSPRNG.
pub fn generate_session_id() -> String {
    let mut s = String::with_capacity(64);
    s.push_str(&uuid::Uuid::new_v4().simple().to_string());
    s.push_str(&uuid::Uuid::new_v4().simple().to_string());
    s
}

/// Lightweight password-strength gate. We deliberately do not pull in a
/// full dictionary checker — for self-host one-operator instances, a
/// minimum length is the only defensible constraint without making it
/// annoying. 12 chars matches NIST SP 800-63B "memorized secret" guidance
/// when no other compensating controls (MFA) are in play.
pub fn validate_password_strength(p: &str) -> Result<(), &'static str> {
    if p.chars().count() < 12 {
        return Err("password must be at least 12 characters");
    }
    if p.chars().count() > 256 {
        // Defends against pathological argon2 input cost with no benefit.
        return Err("password must be at most 256 characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_roundtrips() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password(&hash, "correct horse battery staple"));
    }

    #[test]
    fn verify_rejects_wrong_password() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(!verify_password(&hash, "wrong horse battery staple"));
    }

    #[test]
    fn each_hash_is_unique_thanks_to_salt() {
        let a = hash_password("same password").unwrap();
        let b = hash_password("same password").unwrap();
        assert_ne!(a, b, "salt must randomise output for identical input");
        // ...but both still verify against the same plaintext.
        assert!(verify_password(&a, "same password"));
        assert!(verify_password(&b, "same password"));
    }

    #[test]
    fn verify_returns_false_for_garbage_hash() {
        // Critical: the verify path must NOT propagate a parse error to the
        // caller. A garbage value in the column (corruption, truncated
        // migration) must look exactly like "wrong password" to the outside.
        assert!(!verify_password("not a phc string", "anything"));
        assert!(!verify_password("", "anything"));
    }

    #[test]
    fn verify_returns_false_for_empty_password_against_real_hash() {
        let hash = hash_password("real").unwrap();
        assert!(!verify_password(&hash, ""));
    }

    #[test]
    fn generate_session_id_is_64_hex_chars() {
        let id = generate_session_id();
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_session_id_is_unique_per_call() {
        let a = generate_session_id();
        let b = generate_session_id();
        assert_ne!(a, b);
    }

    #[test]
    fn validate_password_strength_accepts_twelve_or_more() {
        assert!(validate_password_strength("twelve chars").is_ok()); // 12 chars exactly
        assert!(validate_password_strength("a much longer passphrase indeed").is_ok());
    }

    #[test]
    fn validate_password_strength_rejects_too_short() {
        assert!(validate_password_strength("short").is_err());
        assert!(validate_password_strength("eleven chrs").is_err()); // 11 chars
        assert!(validate_password_strength("").is_err());
    }

    #[test]
    fn validate_password_strength_rejects_too_long() {
        let huge = "a".repeat(257);
        assert!(validate_password_strength(&huge).is_err());
    }
}
