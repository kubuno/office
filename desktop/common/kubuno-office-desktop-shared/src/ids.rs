//! Unique keys: [`fresh_uuid_v4`] (idempotency keys, session nonces, new sheet or page ids).

use crate::session::{hex, sha256};

/// A v4-shaped UUID, unique by construction.
///
/// There is no CSPRNG in this crate's dependency set, and an `Idempotency-Key`
/// does not need one: the server keys replay on `(user_id, key)`, so a guessed
/// key can only replay the guesser's own request. What it *does* need is never
/// to repeat - a repeat across two payloads makes the server answer the second
/// with the first's response and drop it. So the value is a SHA-256 over a
/// monotonic counter, the clock, the process id and a per-process hash seed,
/// stamped with the version and variant nibbles.
pub fn fresh_uuid_v4() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(n);
    hasher.write_u64(nanos);
    let seed = hasher.finish();

    let mut material = Vec::with_capacity(32);
    material.extend_from_slice(&n.to_le_bytes());
    material.extend_from_slice(&nanos.to_le_bytes());
    material.extend_from_slice(&seed.to_le_bytes());
    material.extend_from_slice(&(std::process::id() as u64).to_le_bytes());
    let d = sha256(&material);

    let mut b = [0u8; 16];
    b.copy_from_slice(&d[..16]);
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
    format!("{}-{}-{}-{}-{}", hex(&b[0..4]), hex(&b[4..6]), hex(&b[6..8]), hex(&b[8..10]), hex(&b[10..16]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_are_v4_shaped_and_never_repeat() {
        let a = fresh_uuid_v4();
        let b = fresh_uuid_v4();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
    }
}
