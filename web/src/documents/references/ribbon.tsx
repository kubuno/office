// The « Références » ribbon tab, laid out like Word's: Table des matières,
// Notes de bas de page, Citations et bibliographie, Légendes, Index, Table des
// références.
import type { ReactNode } from 'react'
import {
  BookMarked, BookOpen, FileText, Hash, ListTree, Quote, RotateCw,
  Superscript, Tag, TextQuote,
} from 'lucide-react'

import type { RibbonItem, RibbonTab } from '../../ribbon/types'
import type { NoteDirection, NoteKind } from './notes-nav'
import { TocGallery, type TocPreset } from './TocGallery'

export type { TocPreset }

export interface ReferencesRibbonCtx {
  t: (k: string, o?: Record<string, unknown>) => string
  // ── Table des matières ───────────────────────────────────────────────────
  onTocPreset: (preset: TocPreset) => void
  onTocCustom: () => void
  onTocRemove: () => void
  onTocUpdate: () => void
  hasToc: boolean
  outlineLevel: number
  onSetOutlineLevel: (level: number) => void
  // ── Notes ────────────────────────────────────────────────────────────────
  onInsertFootnote: () => void
  onInsertEndnote: () => void
  onGotoNote: (kind: NoteKind, dir: NoteDirection) => void
  onShowNotes: () => void
  hasNotes: boolean
  // ── Citations et bibliographie ───────────────────────────────────────────
  citationStyle: string
  onCitationStyle: (style: string) => void
  onInsertCitation: () => void
  onAddSource: () => void
  onManageSources: () => void
  onBibliography: (heading: 'bibliography' | 'references' | 'works') => void
  // ── Légendes ─────────────────────────────────────────────────────────────
  onInsertCaption: () => void
  onInsertFigures: () => void
  onUpdateFigures: () => void
  onCrossRef: () => void
  // ── Index ────────────────────────────────────────────────────────────────
  onMarkIndex: () => void
  onInsertIndex: () => void
  onUpdateIndex: () => void
  hasIndex: boolean
  // ── Table des références ─────────────────────────────────────────────────
  onMarkCitation: () => void
  onInsertAuthorities: () => void
  onUpdateAuthorities: () => void
  hasAuthorities: boolean
}

const CITATION_STYLES = ['APA', 'MLA', 'Chicago', 'ISO 690', 'IEEE', 'Harvard']

export function buildReferencesTab(c: ReferencesRibbonCtx): RibbonTab {
  const t = c.t
  const label = (k: string, d: string) => t(k, { defaultValue: d })

  const tocMenu: RibbonItem = {
    id: 'toc', kind: 'menu', size: 'large', icon: <ListTree size={22} />,
    label: label('doc_toc', 'Table des matières'),
    menuRender: close => (
      <TocGallery t={t} close={close} hasToc={c.hasToc}
        onPreset={c.onTocPreset} onRemove={c.onTocRemove} onCustom={c.onTocCustom} />
    ),
  }

  const addTextMenu: RibbonItem = {
    id: 'toc-addtext', kind: 'menu', icon: <Tag size={15} />,
    label: label('doc_toc_add_text', 'Ajouter le texte'),
    splitItems: [
      {
        id: 'ol-0', kind: 'button' as const,
        label: (c.outlineLevel === 0 ? '✓ ' : '') + label('doc_toc_no_show', 'Ne pas afficher dans la table des matières'),
        onClick: () => c.onSetOutlineLevel(0),
      },
      ...[1, 2, 3].map(n => ({
        id: 'ol-' + n, kind: 'button' as const,
        label: (c.outlineLevel === n ? '✓ ' : '') + label(`doc_toc_level_${n}`, `Niveau ${n}`),
        onClick: () => c.onSetOutlineLevel(n),
      })),
    ],
  }

  return {
    id: 'references', label: label('doc_tab_references', 'Références'),
    groups: [
      { id: 'toc', label: label('doc_toc', 'Table des matières'), items: [
        tocMenu,
        addTextMenu,
        { id: 'toc-upd', kind: 'button', icon: <RotateCw size={15} />, disabled: !c.hasToc,
          label: label('doc_toc_update', 'Mettre à jour la table'), onClick: c.onTocUpdate },
      ] },

      { id: 'notes', label: label('doc_grp_footnotes', 'Notes de bas de page'), items: [
        { id: 'fn', kind: 'button', size: 'large', icon: <Superscript size={22} />,
          label: label('doc_footnote_insert', 'Insérer une note de bas de page'), onClick: c.onInsertFootnote },
        { id: 'en', kind: 'button', icon: <BookOpen size={15} />,
          label: label('doc_endnote_insert', 'Insérer une note de fin'), onClick: c.onInsertEndnote },
        { id: 'nextnote', kind: 'split', icon: <Superscript size={15} />,
          tooltip: label('doc_note_next', 'Note de bas de page suivante'),
          splitItems: ([
            ['footnote', 'next', label('doc_note_next', 'Note de bas de page suivante')],
            ['footnote', 'prev', label('doc_note_prev', 'Note de bas de page précédente')],
            ['endnote', 'next', label('doc_endnote_next', 'Note de fin suivante')],
            ['endnote', 'prev', label('doc_endnote_prev', 'Note de fin précédente')],
          ] as Array<[NoteKind, NoteDirection, string]>).map(([kind, dir, lbl]) => ({
            id: `note-${kind}-${dir}`, kind: 'button' as const, label: lbl,
            onClick: () => c.onGotoNote(kind, dir),
          })) },
        { id: 'shownotes', kind: 'button', icon: <FileText size={15} />, disabled: !c.hasNotes,
          label: label('doc_note_show', 'Afficher les notes'), onClick: c.onShowNotes },
      ] },

      { id: 'citations', label: label('doc_grp_citations', 'Citations et bibliographie'), items: [
        { id: 'cite', kind: 'menu', size: 'large', icon: <TextQuote size={22} />,
          label: label('doc_cite_insert', 'Insérer une citation'),
          splitItems: [
            { id: 'cite-new', kind: 'button' as const, label: label('doc_cite_new_source', 'Ajouter une nouvelle source…'), onClick: c.onAddSource },
            { id: 'cite-pick', kind: 'button' as const, label: label('doc_cite_pick', 'Choisir une source existante…'), onClick: c.onInsertCitation },
          ] },
        { id: 'sources', kind: 'button', icon: <BookMarked size={15} />,
          label: label('doc_cite_manage', 'Gérer les sources'), onClick: c.onManageSources },
        { id: 'citestyle', kind: 'dropdown', width: 120, value: c.citationStyle,
          options: CITATION_STYLES.map(s => ({ value: s, label: s })), onChange: c.onCitationStyle },
        { id: 'biblio', kind: 'menu', icon: <BookOpen size={15} />,
          label: label('doc_biblio', 'Bibliographie'),
          splitItems: ([
            ['bibliography', label('doc_biblio_h1', 'Bibliographie')],
            ['references', label('doc_biblio_h2', 'Références')],
            ['works', label('doc_biblio_h3', 'Ouvrages cités')],
          ] as Array<['bibliography' | 'references' | 'works', string]>).map(([k, lbl]) => ({
            id: 'biblio-' + k, kind: 'button' as const, label: lbl, onClick: () => c.onBibliography(k),
          })) },
      ] },

      { id: 'captions', label: label('doc_grp_captions', 'Légendes'), items: [
        { id: 'caption', kind: 'button', size: 'large', icon: <Quote size={22} />,
          label: label('doc_caption_insert', 'Insérer une légende'), onClick: c.onInsertCaption },
        { id: 'tof', kind: 'button', icon: <ListTree size={15} />,
          label: label('doc_tof_insert', 'Insérer une table des illustrations'), onClick: c.onInsertFigures },
        { id: 'tof-upd', kind: 'button', icon: <RotateCw size={15} />,
          label: label('doc_toc_update', 'Mettre à jour la table'), onClick: c.onUpdateFigures },
        { id: 'xref', kind: 'button', icon: <Hash size={15} />,
          label: label('doc_crossref', 'Renvoi'), onClick: c.onCrossRef },
      ] },

      { id: 'index', label: label('doc_grp_index', 'Index'), items: [
        { id: 'xe', kind: 'button', size: 'large', icon: <Tag size={22} />,
          label: label('doc_index_mark', 'Marquer entrée'), onClick: c.onMarkIndex },
        { id: 'idx-ins', kind: 'button', icon: <ListTree size={15} />,
          label: label('doc_index_insert', "Insérer l'index"), onClick: c.onInsertIndex },
        { id: 'idx-upd', kind: 'button', icon: <RotateCw size={15} />, disabled: !c.hasIndex,
          label: label('doc_index_update', "Mettre à jour l'index"), onClick: c.onUpdateIndex },
      ] },

      { id: 'authorities', label: label('doc_grp_authorities', 'Table des références'), items: [
        { id: 'ta', kind: 'button', size: 'large', icon: <BookMarked size={22} />,
          label: label('doc_toa_mark', 'Marquer citation'), onClick: c.onMarkCitation },
        { id: 'toa-ins', kind: 'button', icon: <ListTree size={15} />,
          label: label('doc_toa_insert', 'Insérer une table des références'), onClick: c.onInsertAuthorities },
        { id: 'toa-upd', kind: 'button', icon: <RotateCw size={15} />, disabled: !c.hasAuthorities,
          label: label('doc_toc_update', 'Mettre à jour la table'), onClick: c.onUpdateAuthorities },
      ] },
    ],
  }
}
