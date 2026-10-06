// Word's « Formats » presets. One source of truth: the preview panes of the
// dialog and the paragraphs actually generated read the SAME function, so what
// the user previews is what lands in the document.
import type { LeaderKind, TableFormat } from './types'
import { LEADER_CHAR } from './types'

export interface EntryStyle {
  bold?: boolean
  italic?: boolean
  uppercase?: boolean
  fontFamily?: string
  fontSizePt?: number
  color?: string
  /** Extra left indent per level, in "indent steps" (the paragraph attr). */
  indentPerLevel: number
}

/**
 * Look of one entry, by format and level (1-based).
 * `template` deliberately returns almost nothing: it means "follow the
 * document's own styles", which is Word's default and the only format that
 * keeps an imported table looking like its original.
 */
export function entryStyle(format: TableFormat, level: number): EntryStyle {
  const lvl = Math.max(1, level)
  switch (format) {
    case 'classic':
      return {
        bold: lvl === 1,
        uppercase: lvl === 1,
        fontFamily: 'Times New Roman',
        fontSizePt: lvl === 1 ? 12 : 11,
        indentPerLevel: 1,
      }
    case 'distinctive':
      return {
        bold: lvl === 1,
        italic: lvl > 1,
        fontFamily: 'Arial',
        fontSizePt: 11,
        indentPerLevel: 1,
      }
    case 'fancy':
      return {
        italic: true,
        fontFamily: 'Georgia',
        fontSizePt: lvl === 1 ? 12 : 10.5,
        color: lvl === 1 ? '#1a1a1a' : '#444444',
        indentPerLevel: 1,
      }
    case 'modern':
      return {
        bold: lvl === 1,
        fontFamily: 'Arial',
        fontSizePt: lvl === 1 ? 11 : 10,
        color: lvl === 1 ? '#1a73e8' : '#3c4043',
        indentPerLevel: 1,
      }
    case 'formal':
      return {
        fontFamily: 'Times New Roman',
        fontSizePt: 10,
        color: '#202124',
        indentPerLevel: 1,
      }
    case 'simple':
      return { bold: lvl === 1, indentPerLevel: 1 }
    case 'template':
    default:
      return { bold: lvl === 1, indentPerLevel: 1 }
  }
}

/** Human label of a format, for the dialog's dropdown. */
export const FORMAT_LABELS: Record<TableFormat, string> = {
  template: 'Depuis modèle',
  classic: 'Classique',
  distinctive: 'Distinctif',
  fancy: 'Recherché',
  modern: 'Moderne',
  formal: 'Officiel',
  simple: 'Simple',
}

export const LEADER_LABELS: Record<LeaderKind, string> = {
  none: '(aucun)',
  dots: '.......',
  dashes: '-------',
  underline: '_______',
}

/** A short run of leader characters, for the preview panes. */
export function leaderSample(kind: LeaderKind, width = 24): string {
  const ch = LEADER_CHAR[kind]
  return ch ? ch.repeat(width) : ''
}
