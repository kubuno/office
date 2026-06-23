/**
 * Pure business logic for the Office multi-page editor.
 * All functions here are side-effect free and fully unit-testable.
 */

import type { JSONContent } from '@tiptap/react'

// ── Types (mirrors OfficeApp.tsx, kept in sync) ───────────────────────────────

export type Orientation = 'portrait' | 'landscape'

export interface SectionDef {
  id: string
  orientation: Orientation
  margins: { top: number; right: number; bottom: number; left: number }
  // Mise en page avancée (dialogue « Mise en page » façon Word) — optionnels pour
  // rétro-compat. gutter = reliure (px ajoutés au bord intérieur) ; headerDist/
  // footerDist = distance en-tête/pied au bord ; vAlign = alignement vertical du
  // contenu ; sectionStart = type de début de section.
  gutter?: number
  headerDist?: number
  footerDist?: number
  vAlign?: 'top' | 'center' | 'bottom' | 'both'
  sectionStart?: 'nextPage' | 'continuous' | 'evenPage' | 'oddPage'
}

export interface PageData {
  id: string
  sectionId: string
  content: JSONContent
}

export interface PageGeometry {
  pageW: number
  pageH: number
  contentW: number
  contentH: number
  marginH: number
  marginV: number
}

// ── Geometry ──────────────────────────────────────────────────────────────────

/** US Letter dimensions at 96 dpi */
const LETTER_PORTRAIT_W  = 816   // 8.5"
const LETTER_PORTRAIT_H  = 1056  // 11"

export function getGeometry(section: SectionDef): PageGeometry {
  const landscape = section.orientation === 'landscape'
  const pageW = landscape ? LETTER_PORTRAIT_H : LETTER_PORTRAIT_W
  const pageH = landscape ? LETTER_PORTRAIT_W : LETTER_PORTRAIT_H
  return {
    pageW,
    pageH,
    contentW: pageW - section.margins.left - section.margins.right,
    contentH: pageH - section.margins.top  - section.margins.bottom,
    marginH:  section.margins.left,
    marginV:  section.margins.top,
  }
}

// ── Defaults ──────────────────────────────────────────────────────────────────

export function defaultSection(id: string): SectionDef {
  return { id, orientation: 'portrait', margins: { top: 96, right: 96, bottom: 96, left: 96 } }
}

export function emptyDoc(): JSONContent {
  return { type: 'doc', content: [{ type: 'paragraph' }] }
}

// ── Serialization / deserialization ───────────────────────────────────────────

export interface MultiPageDoc {
  _type: 'multi-page'
  sections: SectionDef[]
  pages: PageData[]
}

/**
 * Parse any saved document format into the normalised { sections, pages } shape.
 * Handles:
 *   - null / undefined       → single empty page, default portrait section
 *   - MultiPageDoc (new)     → returned as-is
 *   - ProseMirror JSON (old) → wrapped in a single page/section
 */
export function parseDocContent(raw: object | null | undefined): {
  sections: SectionDef[]
  pages:    PageData[]
} {
  if (!raw) {
    const sid = 'default-section'
    return {
      sections: [defaultSection(sid)],
      pages:    [{ id: 'default-page', sectionId: sid, content: emptyDoc() }],
    }
  }

  const r = raw as Record<string, unknown>

  if (r._type === 'multi-page') {
    const m = r as unknown as MultiPageDoc
    return { sections: m.sections, pages: m.pages }
  }

  // Legacy single-editor ProseMirror JSON
  const sid = 'migrated-section'
  return {
    sections: [defaultSection(sid)],
    pages:    [{ id: 'migrated-page', sectionId: sid, content: raw as JSONContent }],
  }
}

export function serializeDoc(sections: SectionDef[], pages: PageData[]): MultiPageDoc {
  return { _type: 'multi-page', sections, pages }
}

// ── Content flow helpers ──────────────────────────────────────────────────────

/**
 * Given measured block bottom positions (relative to the editor top) and the
 * full list of ProseMirror JSON child nodes, determine which nodes fit within
 * contentH and which overflow.
 *
 * @param blockBottoms  - array where blockBottoms[i] is the bottom Y (px) of
 *                        the i-th top-level ProseMirror block, relative to the
 *                        editor's top edge (from getBoundingClientRect)
 * @param allNodes      - the full list of top-level nodes from editor.getJSON().content
 * @param contentH      - the maximum allowed content height (px, unscaled)
 */
export function splitContentAtHeight(
  blockBottoms: number[],
  allNodes: JSONContent[],
  contentH: number,
): { fittingNodes: JSONContent[]; overflowNodes: JSONContent[] } {
  if (allNodes.length === 0) {
    return { fittingNodes: [], overflowNodes: [] }
  }

  // Find the last block whose bottom is within the content area
  let lastFitIdx = -1
  for (let i = 0; i < blockBottoms.length; i++) {
    if (blockBottoms[i] <= contentH) {
      lastFitIdx = i
    } else {
      break
    }
  }

  // Always keep at least the first block on the page (even if it overflows)
  const keepUntil = Math.max(0, lastFitIdx)

  return {
    fittingNodes:  allNodes.slice(0, keepUntil + 1),
    overflowNodes: allNodes.slice(keepUntil + 1),
  }
}

/**
 * Merge overflow nodes onto the front of an existing next-page document.
 * Preserves existing content; overflow nodes appear before existing ones.
 */
export function prependNodesToDoc(
  overflowNodes: JSONContent[],
  existingDoc: JSONContent,
): JSONContent {
  return {
    type:    'doc',
    content: [...overflowNodes, ...(existingDoc.content ?? [])],
  }
}

/**
 * Try to pull the first block from `nextDoc` onto `currentDoc`.
 * Returns null if pulling would leave nextDoc empty (we never empty a page).
 */
export function tryPullFirstBlock(
  currentDoc: JSONContent,
  nextDoc:    JSONContent,
): { newCurrentDoc: JSONContent; newNextDoc: JSONContent } | null {
  const nextNodes = nextDoc.content ?? []
  if (nextNodes.length <= 1) return null  // would empty the next page

  const [pulled, ...remaining] = nextNodes
  return {
    newCurrentDoc: {
      type:    'doc',
      content: [...(currentDoc.content ?? []), pulled],
    },
    newNextDoc: {
      type:    'doc',
      content: remaining,
    },
  }
}
