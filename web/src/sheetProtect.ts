// Sheet password protection using the EXACT OOXML (ECMA-376) algorithm Excel uses for
// <sheetProtection> — SHA-512 over the salt + UTF-16LE password, then `spinCount` more
// SHA-512 rounds each folding in the little-endian iteration index. The stored shape
// (algorithmName / hashValue / saltValue / spinCount) maps 1:1 to the OOXML attributes,
// so a future .xlsx export/import round-trips natively. This guards editing; like Excel's
// sheet protection it is a hash check, not content encryption.

export interface SheetProtection {
  algorithmName: string   // 'SHA-512'
  hashValue:     string   // base64(final hash)
  saltValue:     string   // base64(random salt)
  spinCount:     number   // iteration count (Excel default: 100000)
}

const EXCEL_SPIN_COUNT = 100000

function bytesToB64(bytes: Uint8Array): string {
  let bin = ''
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i])
  return btoa(bin)
}
function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

// UTF-16LE encode (Excel hashes the password as little-endian UTF-16, no BOM).
function utf16le(str: string): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(str.length * 2)
  const dv = new DataView(out.buffer)
  for (let i = 0; i < str.length; i++) dv.setUint16(i * 2, str.charCodeAt(i), true)
  return out
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(a.length + b.length)
  out.set(a); out.set(b, a.length)
  return out
}

/** ECMA-376 password hash: H = Hash(salt + pwd); repeat spinCount× folding in UInt32LE(i). */
async function excelPasswordHash(password: string, salt: Uint8Array, spinCount: number): Promise<Uint8Array<ArrayBuffer>> {
  let h = new Uint8Array(await crypto.subtle.digest('SHA-512', concat(salt, utf16le(password))))
  const iter = new Uint8Array(4)
  const dv = new DataView(iter.buffer)
  for (let i = 0; i < spinCount; i++) {
    dv.setUint32(0, i, true)
    h = new Uint8Array(await crypto.subtle.digest('SHA-512', concat(h, iter)))
  }
  return h
}

/** Create a protection record for a fresh password (random 16-byte salt). */
export async function createSheetProtection(password: string, spinCount = EXCEL_SPIN_COUNT): Promise<SheetProtection> {
  const salt = new Uint8Array(16)
  crypto.getRandomValues(salt)
  const h = await excelPasswordHash(password, salt, spinCount)
  return { algorithmName: 'SHA-512', hashValue: bytesToB64(h), saltValue: bytesToB64(salt), spinCount }
}

/** Constant-time-ish verification of a password against a stored protection record. */
export async function verifySheetPassword(password: string, prot: SheetProtection): Promise<boolean> {
  try {
    const salt = b64ToBytes(prot.saltValue)
    const h = await excelPasswordHash(password, salt, prot.spinCount || EXCEL_SPIN_COUNT)
    const got = bytesToB64(h)
    if (got.length !== prot.hashValue.length) return false
    let diff = 0
    for (let i = 0; i < got.length; i++) diff |= got.charCodeAt(i) ^ prot.hashValue.charCodeAt(i)
    return diff === 0
  } catch {
    return false
  }
}
