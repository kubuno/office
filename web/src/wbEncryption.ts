// Session-level glue between the Agile crypto (workbookCrypto.ts) and the spreadsheet API.
// Holds the recovered secret key for each unlocked workbook IN MEMORY ONLY (never persisted,
// never sent to the server), and encrypts/decrypts the per-sheet cell payload transparently
// in api.ts. Only cell VALUES/formulas/styles are encrypted; structural metadata (merges,
// column widths, conditional formats…) stays in clear — the sensitive content is the cells.
import type { CellData } from './api'
import {
  createWorkbookKey, unlockWorkbookKey, encryptBlob, decryptBlob,
  textToBytes, bytesToText,
  type WorkbookKeyDescriptor, type EncryptedBlob,
} from './workbookCrypto'

// The `enc` blob stored in a sheet's `data` when the workbook is encrypted.
export interface SheetEncEnvelope { key: WorkbookKeyDescriptor; blob: EncryptedBlob }

// ssId → recovered secret key + its descriptor (in-memory, per session/tab).
const unlocked = new Map<string, { secretKey: Uint8Array; descriptor: WorkbookKeyDescriptor }>()

export const isUnlocked = (ssId: string): boolean => unlocked.has(ssId)
export const workbookDescriptor = (ssId: string): WorkbookKeyDescriptor | undefined => unlocked.get(ssId)?.descriptor
export const forgetWorkbookKey = (ssId: string): void => { unlocked.delete(ssId) }

/** Begin encrypting a workbook: mint a fresh key from the password, remember it this session. */
export async function beginEncryption(ssId: string, password: string): Promise<WorkbookKeyDescriptor> {
  const { descriptor, secretKey } = await createWorkbookKey(password)
  unlocked.set(ssId, { secretKey, descriptor })
  return descriptor
}

/** Unlock an already-encrypted workbook with the password (throws on wrong password). */
export async function unlockWorkbook(ssId: string, descriptor: WorkbookKeyDescriptor, password: string): Promise<void> {
  const secretKey = await unlockWorkbookKey(descriptor, password)
  unlocked.set(ssId, { secretKey, descriptor })
}

/** Encrypt a cell map into an envelope (requires the workbook to be unlocked). */
export async function encryptCells(ssId: string, cells: Record<string, CellData>): Promise<SheetEncEnvelope> {
  const entry = unlocked.get(ssId)
  if (!entry) throw new Error('workbook-locked')
  const blob = await encryptBlob(entry.secretKey, textToBytes(JSON.stringify(cells)))
  return { key: entry.descriptor, blob }
}

/** Decrypt an envelope back into a cell map (requires the workbook to be unlocked). */
export async function decryptCells(ssId: string, env: SheetEncEnvelope): Promise<Record<string, CellData>> {
  const entry = unlocked.get(ssId)
  if (!entry) throw new Error('workbook-locked')
  const bytes = await decryptBlob(entry.secretKey, env.blob)
  return JSON.parse(bytesToText(bytes)) as Record<string, CellData>
}
