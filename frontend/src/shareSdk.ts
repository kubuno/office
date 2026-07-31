// The core's share dialog ships in @kubuno/sdk. The published types this module
// compiles against do not expose it yet, while the HOST provides it at runtime
// through the import map — so we reach it with a narrow cast.
// Replace this file with a direct import once @kubuno/sdk is published & bumped.
import * as sdk from '@kubuno/sdk'

export interface ShareTarget { moduleId: string; id: string; kind?: string }

export interface ShareApi {
  list:   (id: string) => Promise<{ owner: unknown; collaborators: unknown[] }>
  add:    (id: string, userId: string, permission: string) => Promise<unknown>
  update: (id: string, userId: string, permission: string) => Promise<unknown>
  remove: (id: string, userId: string) => Promise<unknown>
  searchRecipients: (q: string) => Promise<unknown[]>
}

export interface OpenShareOptions {
  target: ShareTarget
  api: ShareApi
  title?: string
  permissions?: string[]
  permissionLabel?: (p: string) => string
  link?: string
}

export const openShare = (sdk as unknown as {
  openShare?: (o: OpenShareOptions) => Promise<void>
}).openShare

export interface ShareSection {
  id: string
  moduleId: string
  kind?: string
  order?: number
  slot?: 'general' | 'notice' | 'settings'
  label?: unknown
  Component: unknown
}

export const ShareRegistry = (sdk as unknown as {
  ShareRegistry?: { add: (s: ShareSection) => void; remove: (id: string) => void }
}).ShareRegistry
