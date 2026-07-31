/**
 * Cross-module data sharing over the clipboard (JSON envelopes) — consumer side.
 *
 * VENDORED from core `@kubuno/sdk` (`DataTransferRegistry`): replace the local
 * copy with `import { … } from '@kubuno/sdk'` once `@kubuno/sdk >= 0.1.3` is
 * published on npm. The runtime contract (envelope shape, `data-kubuno` HTML
 * marker, `core.data-card` extension point) is shared with the host and all
 * producer modules, so the copies MUST stay in sync.
 *
 * A producer (maps, paintsharp…) copies `text/plain` (human summary) plus
 * `text/html` containing `<span data-kubuno="<base64 JSON>">`. The documents
 * editor detects the marker on paste, asks the PRODUCER to render the JSON
 * (`renderStatic` of its `core.data-card` entry) and inserts the result as an
 * image node that keeps the original envelope in its `alt` (`kbenvelope:…`).
 */
import { ExtensionRegistry, ModuleServiceRegistry } from '@kubuno/sdk'
import type React from 'react'

export interface KubunoDataEnvelope {
  kubuno: 1
  type: string
  module: string
  title?: string
  text?: string
  href?: string
  data: unknown
}

export const DATA_CARD_EXTENSION = 'core.data-card'

export interface DataCardProps { envelope: KubunoDataEnvelope }

export interface DataCardStaticRender {
  svg?: string
  dataUrl?: string
  width: number
  height: number
}

export interface DataCardRenderer {
  types: string[]
  Component?: React.ComponentType<DataCardProps>
  renderStatic?: (envelope: KubunoDataEnvelope) => Promise<DataCardStaticRender | null>
}

/** Full renderer entry registered by the producer module, or undefined. */
export function resolveDataCardEntry(type: string): DataCardRenderer | undefined {
  return ExtensionRegistry.getAll<DataCardRenderer>(DATA_CARD_EXTENSION)
    .find(r => Array.isArray(r.types) && r.types.includes(type))
}

/** Registers this module's card renderer on the `core.data-card` extension point. */
export function registerDataCardRenderer(moduleId: string, renderer: DataCardRenderer): void {
  ExtensionRegistry.register(DATA_CARD_EXTENSION, moduleId, renderer)
}

/** Writes an envelope to the system clipboard, through the core platform service. */
export function copyKubunoData(envelope: KubunoDataEnvelope): Promise<boolean> {
  return ModuleServiceRegistry.call<Promise<boolean>>('core', 'copyKubunoData', envelope)
    ?? Promise.resolve(false)
}

/**
 * Roaming CLIPBOARD HISTORY (core service, backed by `/api/v1/clipboard`).
 *
 * `copyKubunoData` already records what it copies, so a module only calls these
 * when it copies something the system clipboard cannot carry (the spreadsheet's
 * floating objects, for instance) or when it wants to offer the pane itself.
 * Every call degrades to a no-op on an older host that does not publish them.
 */
export function pushClipboard(envelope: KubunoDataEnvelope): Promise<unknown> {
  return ModuleServiceRegistry.call<Promise<unknown>>('core', 'pushClipboard', envelope)
    ?? Promise.resolve(null)
}

/** Opens the clipboard pane; resolves with the envelope the user picked, or null. */
export function openClipboardPane(types?: string[]): Promise<KubunoDataEnvelope | null> {
  return ModuleServiceRegistry.call<Promise<KubunoDataEnvelope | null>>('core', 'openClipboardPane', types)
    ?? Promise.resolve(null)
}

/** True when the host publishes the clipboard history service. */
export function hasClipboardHistory(): boolean {
  return typeof ModuleServiceRegistry.get('core', 'openClipboardPane') === 'function'
}

function decodeBase64Utf8(b64: string): string {
  const binary = atob(b64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return new TextDecoder().decode(bytes)
}

export function isKubunoDataEnvelope(value: unknown): value is KubunoDataEnvelope {
  if (typeof value !== 'object' || value === null) return false
  const v = value as Record<string, unknown>
  return v.kubuno === 1
    && typeof v.type === 'string' && v.type.includes('.')
    && typeof v.module === 'string' && v.module.length > 0
    && 'data' in v
}

/** Parses a raw string (pasted plain text) as an envelope, or null. */
export function parseKubunoData(raw: string): KubunoDataEnvelope | null {
  const trimmed = raw.trim()
  if (!trimmed.startsWith('{')) return null
  try {
    const parsed: unknown = JSON.parse(trimmed)
    return isKubunoDataEnvelope(parsed) ? parsed : null
  } catch {
    return null
  }
}

/** Extracts an envelope from a paste/drop `DataTransfer`, if any. */
export function readKubunoData(dt: DataTransfer | null): KubunoDataEnvelope | null {
  if (!dt) return null
  const html = dt.getData('text/html')
  const match = html ? /data-kubuno="([A-Za-z0-9+/=]+)"/.exec(html) : null
  if (match) {
    try {
      const parsed: unknown = JSON.parse(decodeBase64Utf8(match[1]))
      if (isKubunoDataEnvelope(parsed)) return parsed
    } catch { /* corrupt marker: fall through to plain text */ }
  }
  return parseKubunoData(dt.getData('text/plain') || '')
}
