//! Sheet protection and workbook encryption, byte-compatible with the web editor.
//!
//! - [`create_protection`] / [`verify_protection`]: the web's `sheetProtect.ts` - the ECMA-376 password hash
//!   Excel writes in `<sheetProtection>` (SHA-512 over salt + UTF-16LE password, then `spinCount` rounds
//!   folding in the little-endian index). It guards editing; it encrypts nothing.
//! - [`create_workbook_key`] / [`unlock_workbook_key`] / [`encrypt_blob`] / [`decrypt_blob`]: the web's
//!   `workbookCrypto.ts` - MS-OFFCRYPTO « Agile » encryption (AES-256-CBC without padding, 4096-byte segments,
//!   HMAC-SHA-512 integrity), stored as the same JSON descriptor and blob. A workbook encrypted on either
//!   client opens on the other (tests: golden vectors produced by the web code).
//! - [`encrypt_cells`] / [`decrypt_cells`]: the per-sheet `enc` envelope of `wbEncryption.ts`.

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sha2::{Digest, Sha512};

/// The spin count Excel (and the web) use.
pub const EXCEL_SPIN_COUNT: u32 = 100_000;
const BLOCK: usize = 16;
const HASH_SIZE: usize = 64;
const SALT_SIZE: usize = 16;
const SEGMENT: usize = 4096;
const KEY_BYTES: usize = 32;

// MS-OFFCRYPTO block keys (§2.3.4.10–14).
const BK_VERIFIER_INPUT: [u8; 8] = [0xfe, 0xa7, 0xd2, 0x76, 0x3b, 0x4b, 0x9e, 0x79];
const BK_VERIFIER_VALUE: [u8; 8] = [0xd7, 0xaa, 0x0f, 0x6d, 0x30, 0x61, 0x34, 0x4e];
const BK_KEY_VALUE: [u8; 8] = [0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6];
const BK_HMAC_KEY: [u8; 8] = [0x5f, 0xb2, 0xad, 0x01, 0x0c, 0xb9, 0xe1, 0xf6];
const BK_HMAC_VALUE: [u8; 8] = [0xa0, 0x67, 0x7f, 0x02, 0xb2, 0x2c, 0x84, 0x33];

/// Why a password or a payload was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// The password does not open the workbook (`wrong-password`).
    WrongPassword,
    /// The payload was altered (`integrity-failed`) or is not one of ours.
    Integrity,
    /// No randomness available from the system.
    Random,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CryptoError::WrongPassword => "wrong-password",
            CryptoError::Integrity => "integrity-failed",
            CryptoError::Random => "no-random-source",
        })
    }
}

impl std::error::Error for CryptoError {}

fn random<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|_| CryptoError::Random)?;
    Ok(b)
}

/// The password as UTF-16LE code units (JavaScript `charCodeAt`: surrogates as they are, no BOM).
fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn sha512(parts: &[&[u8]]) -> [u8; HASH_SIZE] {
    let mut h = Sha512::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// H = Hash(salt + pwd), then `spin` × Hash(H + UInt32LE(i)).
fn spin_hash(password: &str, salt: &[u8], spin: u32) -> [u8; HASH_SIZE] {
    let mut h = sha512(&[salt, &utf16le(password)]);
    for i in 0..spin {
        h = sha512(&[&h, &i.to_le_bytes()]);
    }
    h
}

fn unb64(s: &str) -> Result<Vec<u8>, CryptoError> {
    B64.decode(s).map_err(|_| CryptoError::Integrity)
}

fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

// ── Sheet protection (sheetProtect.ts) ─────────────────────────────────────────

/// `SheetProtection`: maps 1:1 to the OOXML `<sheetProtection>` attributes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetProtection {
    pub algorithm_name: String,
    pub hash_value: String,
    pub salt_value: String,
    pub spin_count: u32,
}

/// `createSheetProtection`: a record for a fresh password (random 16-byte salt).
pub fn create_protection(password: &str, spin_count: u32) -> Result<SheetProtection, CryptoError> {
    let salt: [u8; SALT_SIZE] = random()?;
    let h = spin_hash(password, &salt, spin_count);
    Ok(SheetProtection { algorithm_name: "SHA-512".into(), hash_value: B64.encode(h), salt_value: B64.encode(salt), spin_count })
}

/// `verifySheetPassword` (a spin count of 0 means Excel's default, as on the web).
pub fn verify_protection(password: &str, prot: &SheetProtection) -> bool {
    let Ok(salt) = unb64(&prot.salt_value) else { return false };
    let spin = if prot.spin_count == 0 { EXCEL_SPIN_COUNT } else { prot.spin_count };
    let got = B64.encode(spin_hash(password, &salt, spin));
    timing_safe_eq(got.as_bytes(), prot.hash_value.as_bytes())
}

// ── Workbook encryption (workbookCrypto.ts) ────────────────────────────────────

/// `WorkbookKeyDescriptor`: the keyEncryptor, stored once per workbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookKeyDescriptor {
    pub spin_count: u32,
    pub key_bits: u32,
    pub salt_value: String,
    pub encrypted_verifier_hash_input: String,
    pub encrypted_verifier_hash_value: String,
    pub encrypted_key_value: String,
}

/// `EncryptedBlob`: one payload encrypted with the workbook's secret key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedBlob {
    pub key_data_salt: String,
    pub package: String,
    pub encrypted_hmac_key: String,
    pub encrypted_hmac_value: String,
    pub size: usize,
}

/// The `enc` envelope a sheet's data holds when the workbook is encrypted (`SheetEncEnvelope`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetEncEnvelope {
    pub key: WorkbookKeyDescriptor,
    pub blob: EncryptedBlob,
}

/// The recovered secret key of an unlocked workbook (kept in memory only, never written or sent).
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey(Vec<u8>);

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(…)")
    }
}

type Enc = cbc::Encryptor<aes::Aes256>;
type Dec = cbc::Decryptor<aes::Aes256>;

/// AES-256-CBC without padding over a 16-aligned buffer (Excel zero-pads beforehand).
fn aes_enc(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut buf = data.to_vec();
    let len = buf.len();
    Enc::new_from_slices(key, iv).map_err(|_| CryptoError::Integrity)?.encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf, len).map_err(|_| CryptoError::Integrity)?;
    Ok(buf)
}

fn aes_dec(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut buf = data.to_vec();
    let n = Dec::new_from_slices(key, iv).map_err(|_| CryptoError::Integrity)?.decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf).map_err(|_| CryptoError::Integrity)?.len();
    buf.truncate(n);
    Ok(buf)
}

/// `deriveKey`: the spun hash, then Hash(H + blockKey), truncated to 32 bytes ([`password_keys`] computes the three
/// at once; kept for the conformance test against the web's `__deriveKeyForTest`).
#[cfg(test)]
fn derive_key(password: &str, salt: &[u8], spin: u32, block_key: &[u8]) -> [u8; KEY_BYTES] {
    let h = spin_hash(password, salt, spin);
    let f = sha512(&[&h, block_key]);
    let mut out = [0u8; KEY_BYTES];
    out.copy_from_slice(&f[..KEY_BYTES]);
    out
}

/// `ivFor`: Hash(keyDataSalt + block) truncated to the block size.
fn iv_for(salt: &[u8], block: &[u8]) -> [u8; BLOCK] {
    let h = sha512(&[salt, block]);
    let mut iv = [0u8; BLOCK];
    iv.copy_from_slice(&h[..BLOCK]);
    iv
}

fn hmac512(key: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut m = <Hmac<Sha512> as Mac>::new_from_slice(key).map_err(|_| CryptoError::Integrity)?;
    m.update(data);
    Ok(m.finalize().into_bytes().to_vec())
}

/// The keyEncryptor's derived keys from the password and its salt (verifier input, verifier value, key value).
fn password_keys(password: &str, salt: &[u8], spin: u32) -> ([u8; KEY_BYTES], [u8; KEY_BYTES], [u8; KEY_BYTES]) {
    // The spun hash is the costly part: computed once for the three block keys.
    let h = spin_hash(password, salt, spin);
    let k = |bk: &[u8]| {
        let f = sha512(&[&h, bk]);
        let mut out = [0u8; KEY_BYTES];
        out.copy_from_slice(&f[..KEY_BYTES]);
        out
    };
    (k(&BK_VERIFIER_INPUT), k(&BK_VERIFIER_VALUE), k(&BK_KEY_VALUE))
}

fn salt_iv(salt: &[u8]) -> Result<&[u8], CryptoError> {
    salt.get(..BLOCK).ok_or(CryptoError::Integrity)
}

/// `createWorkbookKey`: a random secret key wrapped by the password.
pub fn create_workbook_key(password: &str, spin_count: u32) -> Result<(WorkbookKeyDescriptor, SecretKey), CryptoError> {
    let salt: [u8; SALT_SIZE] = random()?;
    let secret: [u8; KEY_BYTES] = random()?;
    let verifier: [u8; BLOCK] = random()?;
    let verifier_hash = sha512(&[&verifier]);
    let (k_in, k_val, k_key) = password_keys(password, &salt, spin_count);
    let iv = salt_iv(&salt)?;
    let d = WorkbookKeyDescriptor {
        spin_count,
        key_bits: (KEY_BYTES * 8) as u32,
        salt_value: B64.encode(salt),
        encrypted_verifier_hash_input: B64.encode(aes_enc(&k_in, iv, &verifier)?),
        encrypted_verifier_hash_value: B64.encode(aes_enc(&k_val, iv, &verifier_hash)?),
        encrypted_key_value: B64.encode(aes_enc(&k_key, iv, &secret)?),
    };
    Ok((d, SecretKey(secret.to_vec())))
}

/// `verifyPassword`: the password against the verifier, without decrypting anything else.
pub fn verify_password(d: &WorkbookKeyDescriptor, password: &str) -> bool {
    (|| -> Result<bool, CryptoError> {
        let salt = unb64(&d.salt_value)?;
        let iv = salt_iv(&salt)?;
        let (k_in, k_val, _) = password_keys(password, &salt, d.spin_count);
        let input = aes_dec(&k_in, iv, &unb64(&d.encrypted_verifier_hash_input)?)?;
        let hash = aes_dec(&k_val, iv, &unb64(&d.encrypted_verifier_hash_value)?)?;
        Ok(hash.len() >= HASH_SIZE && timing_safe_eq(&hash[..HASH_SIZE], &sha512(&[&input])))
    })()
    .unwrap_or(false)
}

/// `unlockWorkbookKey`: the secret key, or [`CryptoError::WrongPassword`].
pub fn unlock_workbook_key(d: &WorkbookKeyDescriptor, password: &str) -> Result<SecretKey, CryptoError> {
    let salt = unb64(&d.salt_value)?;
    let iv = salt_iv(&salt)?;
    let (k_in, k_val, k_key) = password_keys(password, &salt, d.spin_count);
    let input = aes_dec(&k_in, iv, &unb64(&d.encrypted_verifier_hash_input)?)?;
    let hash = aes_dec(&k_val, iv, &unb64(&d.encrypted_verifier_hash_value)?)?;
    if hash.len() < HASH_SIZE || !timing_safe_eq(&hash[..HASH_SIZE], &sha512(&[&input])) {
        return Err(CryptoError::WrongPassword);
    }
    let mut key = aes_dec(&k_key, iv, &unb64(&d.encrypted_key_value)?)?;
    key.truncate(KEY_BYTES);
    Ok(SecretKey(key))
}

/// `encryptBlob`.
pub fn encrypt_blob(key: &SecretKey, plain: &[u8]) -> Result<EncryptedBlob, CryptoError> {
    let salt: [u8; SALT_SIZE] = random()?;
    let mut package = Vec::with_capacity(plain.len() + BLOCK);
    for (seg, chunk) in plain.chunks(SEGMENT).enumerate() {
        let mut c = chunk.to_vec();
        c.resize(c.len().div_ceil(BLOCK) * BLOCK, 0);
        package.extend(aes_enc(&key.0, &iv_for(&salt, &(seg as u32).to_le_bytes()), &c)?);
    }
    let hmac_key: [u8; HASH_SIZE] = random()?;
    let mac = hmac512(&hmac_key, &package)?;
    Ok(EncryptedBlob {
        key_data_salt: B64.encode(salt),
        package: B64.encode(&package),
        encrypted_hmac_key: B64.encode(aes_enc(&key.0, &iv_for(&salt, &BK_HMAC_KEY), &hmac_key)?),
        encrypted_hmac_value: B64.encode(aes_enc(&key.0, &iv_for(&salt, &BK_HMAC_VALUE), &mac)?),
        size: plain.len(),
    })
}

/// `decryptBlob`: refuses a tampered payload (HMAC mismatch).
pub fn decrypt_blob(key: &SecretKey, blob: &EncryptedBlob) -> Result<Vec<u8>, CryptoError> {
    let salt = unb64(&blob.key_data_salt)?;
    let package = unb64(&blob.package)?;
    let hmac_key = aes_dec(&key.0, &iv_for(&salt, &BK_HMAC_KEY), &unb64(&blob.encrypted_hmac_key)?)?;
    let stored = aes_dec(&key.0, &iv_for(&salt, &BK_HMAC_VALUE), &unb64(&blob.encrypted_hmac_value)?)?;
    let (Some(hk), Some(sm)) = (hmac_key.get(..HASH_SIZE), stored.get(..HASH_SIZE)) else { return Err(CryptoError::Integrity) };
    if !timing_safe_eq(&hmac512(hk, &package)?, sm) {
        return Err(CryptoError::Integrity);
    }
    let mut out = Vec::with_capacity(package.len());
    for (seg, chunk) in package.chunks(SEGMENT).enumerate() {
        out.extend(aes_dec(&key.0, &iv_for(&salt, &(seg as u32).to_le_bytes()), chunk)?);
    }
    out.truncate(blob.size);
    Ok(out)
}

/// `encryptCells`: a sheet's cell map into its `enc` envelope.
pub fn encrypt_cells(d: &WorkbookKeyDescriptor, key: &SecretKey, cells: &Json) -> Result<SheetEncEnvelope, CryptoError> {
    let text = serde_json::to_string(cells).map_err(|_| CryptoError::Integrity)?;
    Ok(SheetEncEnvelope { key: d.clone(), blob: encrypt_blob(key, text.as_bytes())? })
}

/// `decryptCells`.
pub fn decrypt_cells(key: &SecretKey, env: &SheetEncEnvelope) -> Result<Json, CryptoError> {
    let bytes = decrypt_blob(key, &env.blob)?;
    serde_json::from_slice(&bytes).map_err(|_| CryptoError::Integrity)
}

// ── The workbook in memory (wbEncryption.ts + the editor's gate) ───────────────

/// The descriptor of an encrypted sheet's envelope (`data.enc.key`).
fn sheet_descriptor(s: &crate::model::sheet::Sheet) -> Option<WorkbookKeyDescriptor> {
    s.data.get("enc").and_then(|e| e.get("key")).and_then(|k| serde_json::from_value(k.clone()).ok())
}

/// Decrypts `sheet`'s envelope into its cells (the envelope stays: it marks the sheet encrypted).
pub fn unlock_sheet(sheet: &mut crate::model::sheet::Sheet, key: &SecretKey) -> Result<bool, CryptoError> {
    let Some(env) = sheet.data.get("enc").filter(|e| e.get("blob").is_some()).and_then(|e| serde_json::from_value::<SheetEncEnvelope>(e.clone()).ok()) else { return Ok(false) };
    let cells = decrypt_cells(key, &env)?;
    if let Some(o) = cells.as_object() {
        sheet.cells = o.iter().map(|(k, v)| (k.clone(), crate::model::cell::Cell::from_json(v))).collect();
    }
    Ok(true)
}

impl crate::editor::Editor {
    /// The workbook's key descriptor when it is encrypted (`wbEnc`).
    pub fn encryption(&self) -> Option<WorkbookKeyDescriptor> {
        self.wb.sheets.iter().find_map(sheet_descriptor)
    }

    /// Whether the workbook is encrypted (any sheet carries an envelope).
    pub fn is_encrypted(&self) -> bool {
        self.wb.sheets.iter().any(|s| s.is_encrypted())
    }

    /// `unlockWorkbookNow`: the cells of every encrypted sheet, decrypted (a wrong key changes nothing).
    pub fn unlock_workbook(&mut self, key: &SecretKey) -> Result<(), CryptoError> {
        let mut sheets = self.wb.sheets.clone();
        for s in &mut sheets {
            unlock_sheet(s, key)?;
        }
        self.wb.sheets = sheets;
        self.touch();
        Ok(())
    }

    /// `encryptWorkbook` in memory: every sheet is marked encrypted (`enc: { key }`): the collaboration stops
    /// and the save encrypts the cells (the envelope itself is made by the server host).
    pub fn mark_encrypted(&mut self, d: &WorkbookKeyDescriptor) {
        let marker = serde_json::json!({ "key": d });
        for s in &mut self.wb.sheets {
            s.data.insert("enc".into(), marker.clone());
        }
        self.touch();
    }

    /// `decryptWorkbook` in memory: the envelopes go; the cells stay as they are (clear).
    pub fn mark_decrypted(&mut self) {
        for s in &mut self.wb.sheets {
            s.data.remove("enc");
        }
        self.touch();
    }
}

#[cfg(test)]
pub(crate) fn derive_key_for_test(password: &str, salt: &[u8], spin: u32, block_key: &[u8]) -> [u8; KEY_BYTES] {
    derive_key(password, salt, spin, block_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Produced by the web's own `workbookCrypto.ts` / `sheetProtect.ts` (Node, Web Crypto).
    fn vectors() -> Json {
        serde_json::from_str(include_str!("../tests/fixtures/web_crypto_vectors.json")).expect("the fixture is JSON")
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn the_key_derivation_matches_the_web() {
        let salt: Vec<u8> = (1..=16).collect();
        let k = derive_key_for_test("Pâssw0rd€", &salt, 1000, &BK_KEY_VALUE);
        assert_eq!(hex(&k), vectors()["deriveKey"]);
    }

    #[test]
    fn a_workbook_encrypted_by_the_web_opens_here() {
        let v = vectors();
        let d: WorkbookKeyDescriptor = serde_json::from_value(v["descriptor"].clone()).expect("descriptor");
        assert!(verify_password(&d, "Pâssw0rd€"));
        assert!(!verify_password(&d, "passw0rd"));
        assert_eq!(unlock_workbook_key(&d, "nope"), Err(CryptoError::WrongPassword));
        let key = unlock_workbook_key(&d, "Pâssw0rd€").expect("the right password");
        assert_eq!(hex(&key.0), v["secretKey"]);
        let blob: EncryptedBlob = serde_json::from_value(v["blob"].clone()).expect("blob");
        let plain = decrypt_blob(&key, &blob).expect("decrypts");
        assert_eq!(String::from_utf8(plain).expect("utf-8"), v["plain"]);
        let mut bad = blob.clone();
        bad.package = B64.encode({
            let mut p = unb64(&blob.package).expect("b64");
            p[100] ^= 1;
            p
        });
        assert_eq!(decrypt_blob(&key, &bad), Err(CryptoError::Integrity), "a tampered package is refused");
    }

    #[test]
    fn what_is_encrypted_here_round_trips_in_the_web_format() {
        let (d, key) = create_workbook_key("mot de passe", 50).expect("random");
        let json = serde_json::to_value(&d).expect("serialises");
        assert!(json.get("encryptedVerifierHashInput").is_some() && json.get("keyBits") == Some(&Json::from(256)));
        let again = unlock_workbook_key(&d, "mot de passe").expect("unlocks");
        let cells = serde_json::json!({ "A1": { "v": "x".repeat(9000) }, "B1": { "v": 3 } });
        let env = encrypt_cells(&d, &again, &cells).expect("encrypts");
        assert_eq!(env.blob.size, serde_json::to_string(&cells).expect("json").len());
        assert_eq!(decrypt_cells(&key, &env).expect("decrypts"), cells);
    }

    #[test]
    fn a_sheet_protected_by_the_web_unlocks_here() {
        let p: SheetProtection = serde_json::from_value(vectors()["protection"].clone()).expect("protection");
        assert_eq!(p.algorithm_name, "SHA-512");
        assert!(verify_protection("secret", &p));
        assert!(!verify_protection("Secret", &p));
        let mine = create_protection("ça", 10).expect("random");
        assert!(verify_protection("ça", &mine) && !verify_protection("ca", &mine));
        assert_eq!(serde_json::to_value(&mine).expect("json")["spinCount"], 10);
    }
}

#[cfg(test)]
mod emit {
    /// `cargo test -- --ignored emit_vector`: a workbook encrypted here, for the web to open (cross-check).
    #[test]
    #[ignore]
    fn emit_vector() {
        let (d, key) = super::create_workbook_key("Pâssw0rd€", 1000).expect("random");
        let env = super::encrypt_cells(&d, &key, &serde_json::json!({ "A1": { "v": "desktop é 😀" } })).expect("encrypts");
        let prot = super::create_protection("secret", 1000).expect("random");
        println!("VECTOR {}", serde_json::json!({ "env": env, "protection": prot }));
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    use crate::editor::tests::editor;

    #[test]
    fn an_encrypted_workbook_unlocks_in_the_editor() {
        let (d, key) = create_workbook_key("pw", 5).expect("random");
        let env = encrypt_cells(&d, &key, &serde_json::json!({ "A1": { "v": "secret" } })).expect("encrypts");
        let mut ed = editor(serde_json::json!({}));
        ed.wb.sheets[0].data.insert("enc".into(), serde_json::to_value(&env).expect("json"));
        assert!(ed.is_encrypted());
        assert_eq!(ed.encryption(), Some(d.clone()));
        assert_eq!(ed.cell_text(0, 1), "", "locked: nothing shows");
        let wrong = create_workbook_key("x", 5).expect("random").1;
        assert!(ed.unlock_workbook(&wrong).is_err());
        ed.unlock_workbook(&key).expect("unlocks");
        assert_eq!(ed.cell_text(0, 1), "secret");
        assert!(ed.is_encrypted(), "the envelope stays as the marker");
        ed.mark_decrypted();
        assert!(!ed.is_encrypted());
        ed.mark_encrypted(&d);
        assert!(ed.wb.sheets.iter().all(|s| s.is_encrypted()));
    }
}
