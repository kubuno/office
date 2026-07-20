// Workbook password encryption using Microsoft's ECMA-376 "Agile Encryption" — the exact
// scheme Excel uses for "Encrypt with Password". Unlike sheetProtect.ts (a password *hash*
// that only guards editing), this REALLY encrypts the content: the derived AES key protects
// a random secret key, which encrypts the payload; a wrong password can't decrypt anything.
//
// This implements the cryptography (key derivation + AES-256-CBC + verifier + HMAC data
// integrity) exactly per MS-OFFCRYPTO §2.3.4. It stores the descriptor as JSON (the Kubuno
// content blob) rather than the CFB/EncryptionInfo container of a .xlsx file — the crypto is
// identical, only the outer packaging differs. All primitives use Web Crypto (crypto.subtle).

const HASH = 'SHA-512'
const KEY_BITS = 256
const BLOCK = 16          // AES block / IV size
const HASH_SIZE = 64      // SHA-512
const SALT_SIZE = 16
const SEGMENT = 4096      // package encryption segment size
const EXCEL_SPIN = 100000

// MS-OFFCRYPTO block keys (§2.3.4.10–14) — fixed constants folded into key derivation.
const BK_VERIFIER_INPUT = new Uint8Array([0xfe, 0xa7, 0xd2, 0x76, 0x3b, 0x4b, 0x9e, 0x79])
const BK_VERIFIER_VALUE = new Uint8Array([0xd7, 0xaa, 0x0f, 0x6d, 0x30, 0x61, 0x34, 0x4e])
const BK_KEY_VALUE      = new Uint8Array([0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6])
const BK_HMAC_KEY       = new Uint8Array([0x5f, 0xb2, 0xad, 0x01, 0x0c, 0xb9, 0xe1, 0xf6])
const BK_HMAC_VALUE     = new Uint8Array([0xa0, 0x67, 0x7f, 0x02, 0xb2, 0x2c, 0x84, 0x33])

// The keyEncryptor: binds the password to a random secret key (the whole-workbook key).
// Stored once per workbook; reveals nothing without the password.
export interface WorkbookKeyDescriptor {
  spinCount: number
  keyBits: number
  saltValue: string                    // b64 — password key salt
  encryptedVerifierHashInput: string   // b64
  encryptedVerifierHashValue: string   // b64
  encryptedKeyValue: string            // b64 — the secret key, wrapped by the password key
}

// One payload encrypted with the workbook's secret key (one per sheet).
export interface EncryptedBlob {
  keyDataSalt: string        // b64 — per-blob salt for segment IVs + HMAC
  package: string            // b64 — AES-256-CBC segmented ciphertext
  encryptedHmacKey: string   // b64
  encryptedHmacValue: string // b64
  size: number               // plaintext byte length
}

// ── byte helpers ─────────────────────────────────────────────────────────────
function b64(bytes: Uint8Array): string {
  let s = ''
  for (let i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i])
  return btoa(s)
}
function unb64(s: string): Uint8Array {
  const bin = atob(s)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}
function utf16le(str: string): Uint8Array {
  const out = new Uint8Array(str.length * 2)
  const dv = new DataView(out.buffer)
  for (let i = 0; i < str.length; i++) dv.setUint16(i * 2, str.charCodeAt(i), true)
  return out
}
function concat(...arrs: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const len = arrs.reduce((a, x) => a + x.length, 0)
  const out = new Uint8Array(len)
  let o = 0
  for (const a of arrs) { out.set(a, o); o += a.length }
  return out
}
function u32le(n: number): Uint8Array {
  const b = new Uint8Array(4)
  new DataView(b.buffer).setUint32(0, n >>> 0, true)
  return b
}
async function sha512(data: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
  return new Uint8Array(await crypto.subtle.digest(HASH, data as BufferSource))
}

// ── AES-256-CBC with NO padding (Excel pads with zeros; all buffers are 16-aligned) ──
// Web Crypto always applies PKCS#7, so we peel it off on encrypt and synthesise it on decrypt.
async function importAesKey(raw: Uint8Array): Promise<CryptoKey> {
  return crypto.subtle.importKey('raw', raw as BufferSource, 'AES-CBC', false, ['encrypt', 'decrypt'])
}
async function aesEncNoPad(keyRaw: Uint8Array, iv: Uint8Array, data: Uint8Array): Promise<Uint8Array> {
  const key = await importAesKey(keyRaw)
  const full = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-CBC', iv: iv as BufferSource }, key, data as BufferSource))
  return full.slice(0, data.length) // drop the trailing encrypted PKCS#7 block
}
async function aesDecNoPad(keyRaw: Uint8Array, iv: Uint8Array, data: Uint8Array): Promise<Uint8Array> {
  const key = await importAesKey(keyRaw)
  // Forge a valid encrypted PKCS#7 block so subtle.decrypt accepts and strips it.
  const lastBlock = data.slice(data.length - BLOCK)
  const padPlain = new Uint8Array(BLOCK).fill(BLOCK)
  const padCipher = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-CBC', iv: lastBlock as BufferSource }, key, padPlain as BufferSource)).slice(0, BLOCK)
  const full = concat(data, padCipher)
  const plain = new Uint8Array(await crypto.subtle.decrypt({ name: 'AES-CBC', iv: iv as BufferSource }, key, full as BufferSource))
  return plain
}

// ── Agile key derivation: H = Hash(salt+pwd), spinCount× Hash(H+UInt32LE(i)), then Hash(H+blockKey) ──
async function deriveKey(password: string, salt: Uint8Array, spinCount: number, blockKey: Uint8Array): Promise<Uint8Array> {
  let h = await sha512(concat(salt, utf16le(password)))
  for (let i = 0; i < spinCount; i++) h = await sha512(concat(h, u32le(i)))
  const final = await sha512(concat(h, blockKey))
  // Truncate/pad to keyBits/8 (32 for AES-256). SHA-512 output (64) ≥ 32, so just slice.
  const out = new Uint8Array(KEY_BITS / 8)
  out.set(final.slice(0, KEY_BITS / 8))
  return out
}

/** IV for a keyEncryptor block = the password salt (Excel uses the salt directly). */
async function ivFromSalt(salt: Uint8Array): Promise<Uint8Array> {
  return salt.slice(0, BLOCK)
}
/** IV for the package/HMAC = Hash(keyDataSalt + blockKeyOrSegmentIndex), truncated to blockSize. */
async function ivFor(keyDataSalt: Uint8Array, block: Uint8Array): Promise<Uint8Array> {
  const h = await sha512(concat(keyDataSalt, block))
  return h.slice(0, BLOCK)
}

/** Encrypt the package in 4096-byte segments, each with IV = Hash(keyDataSalt + UInt32LE(segIndex)). */
async function encryptPackage(secretKey: Uint8Array, keyDataSalt: Uint8Array, data: Uint8Array): Promise<Uint8Array> {
  const out: Uint8Array[] = []
  for (let i = 0, seg = 0; i < data.length; i += SEGMENT, seg++) {
    let chunk = data.slice(i, i + SEGMENT)
    if (chunk.length % BLOCK !== 0) {
      const padded = new Uint8Array(Math.ceil(chunk.length / BLOCK) * BLOCK)
      padded.set(chunk); chunk = padded
    }
    const iv = await ivFor(keyDataSalt, u32le(seg))
    out.push(await aesEncNoPad(secretKey, iv, chunk))
  }
  return concat(...out)
}
async function decryptPackage(secretKey: Uint8Array, keyDataSalt: Uint8Array, data: Uint8Array, plainLen: number): Promise<Uint8Array> {
  const out: Uint8Array[] = []
  for (let i = 0, seg = 0; i < data.length; i += SEGMENT, seg++) {
    const chunk = data.slice(i, i + SEGMENT)
    const iv = await ivFor(keyDataSalt, u32le(seg))
    out.push(await aesDecNoPad(secretKey, iv, chunk))
  }
  return concat(...out).slice(0, plainLen)
}

/** Create a workbook key: a random secret key wrapped by the password (keyEncryptor). */
export async function createWorkbookKey(password: string, spinCount = EXCEL_SPIN): Promise<{ descriptor: WorkbookKeyDescriptor; secretKey: Uint8Array }> {
  const pwdSalt = crypto.getRandomValues(new Uint8Array(SALT_SIZE))
  const secretKey = crypto.getRandomValues(new Uint8Array(KEY_BITS / 8))

  const verifierInput = crypto.getRandomValues(new Uint8Array(BLOCK))
  const verifierHash = await sha512(verifierInput)
  const kVerIn = await deriveKey(password, pwdSalt, spinCount, BK_VERIFIER_INPUT)
  const kVerVal = await deriveKey(password, pwdSalt, spinCount, BK_VERIFIER_VALUE)
  const kKeyVal = await deriveKey(password, pwdSalt, spinCount, BK_KEY_VALUE)
  const iv = await ivFromSalt(pwdSalt)

  return {
    descriptor: {
      spinCount, keyBits: KEY_BITS,
      saltValue: b64(pwdSalt),
      encryptedVerifierHashInput: b64(await aesEncNoPad(kVerIn, iv, verifierInput)),
      encryptedVerifierHashValue: b64(await aesEncNoPad(kVerVal, iv, verifierHash)),
      encryptedKeyValue: b64(await aesEncNoPad(kKeyVal, iv, secretKey)),
    },
    secretKey,
  }
}

/** Recover the workbook secret key from the password, or throw on a wrong password. */
export async function unlockWorkbookKey(desc: WorkbookKeyDescriptor, password: string): Promise<Uint8Array> {
  if (!(await verifyPassword(desc, password))) throw new Error('wrong-password')
  const pwdSalt = unb64(desc.saltValue)
  const iv = await ivFromSalt(pwdSalt)
  const kKeyVal = await deriveKey(password, pwdSalt, desc.spinCount, BK_KEY_VALUE)
  return aesDecNoPad(kKeyVal, iv, unb64(desc.encryptedKeyValue))
}

/** Encrypt a blob with an already-recovered secret key (per sheet). */
export async function encryptBlob(secretKey: Uint8Array, plaintext: Uint8Array): Promise<EncryptedBlob> {
  const keyDataSalt = crypto.getRandomValues(new Uint8Array(SALT_SIZE))
  const encPackage = await encryptPackage(secretKey, keyDataSalt, plaintext)
  const hmacKey = crypto.getRandomValues(new Uint8Array(HASH_SIZE))
  const encHmacKey = await aesEncNoPad(secretKey, await ivFor(keyDataSalt, BK_HMAC_KEY), hmacKey)
  const mac = await hmacSha512(hmacKey, encPackage)
  const encHmacVal = await aesEncNoPad(secretKey, await ivFor(keyDataSalt, BK_HMAC_VALUE), mac)
  return {
    keyDataSalt: b64(keyDataSalt), package: b64(encPackage),
    encryptedHmacKey: b64(encHmacKey), encryptedHmacValue: b64(encHmacVal),
    size: plaintext.length,
  }
}

/** Decrypt a blob with the secret key. Throws on tampering (HMAC mismatch). */
export async function decryptBlob(secretKey: Uint8Array, blob: EncryptedBlob): Promise<Uint8Array> {
  const keyDataSalt = unb64(blob.keyDataSalt)
  const encPackage = unb64(blob.package)
  const hmacKey = (await aesDecNoPad(secretKey, await ivFor(keyDataSalt, BK_HMAC_KEY), unb64(blob.encryptedHmacKey))).slice(0, HASH_SIZE)
  const storedMac = (await aesDecNoPad(secretKey, await ivFor(keyDataSalt, BK_HMAC_VALUE), unb64(blob.encryptedHmacValue))).slice(0, HASH_SIZE)
  const mac = await hmacSha512(hmacKey, encPackage)
  if (!timingSafeEqual(mac, storedMac)) throw new Error('integrity-failed')
  return decryptPackage(secretKey, keyDataSalt, encPackage, blob.size)
}

async function hmacSha512(keyRaw: Uint8Array, data: Uint8Array): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey('raw', keyRaw as BufferSource, { name: 'HMAC', hash: HASH }, false, ['sign'])
  return new Uint8Array(await crypto.subtle.sign('HMAC', key, data as BufferSource))
}

function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false
  let d = 0
  for (let i = 0; i < a.length; i++) d |= a[i] ^ b[i]
  return d === 0
}

/** Check a password against the verifier without decrypting the package. */
export async function verifyPassword(desc: WorkbookKeyDescriptor, password: string): Promise<boolean> {
  try {
    const pwdSalt = unb64(desc.saltValue)
    const iv = await ivFromSalt(pwdSalt)
    const kVerIn = await deriveKey(password, pwdSalt, desc.spinCount, BK_VERIFIER_INPUT)
    const kVerVal = await deriveKey(password, pwdSalt, desc.spinCount, BK_VERIFIER_VALUE)
    const verInput = await aesDecNoPad(kVerIn, iv, unb64(desc.encryptedVerifierHashInput))
    const verHash = await aesDecNoPad(kVerVal, iv, unb64(desc.encryptedVerifierHashValue))
    const expect = await sha512(verInput)
    return timingSafeEqual(verHash.slice(0, HASH_SIZE), expect)
  } catch {
    return false
  }
}

export const textToBytes = (s: string): Uint8Array => new TextEncoder().encode(s)
export const bytesToText = (b: Uint8Array): string => new TextDecoder().decode(b)

// Test-only hook: exposes the raw Agile key derivation for conformance checks.
export const __deriveKeyForTest = deriveKey
