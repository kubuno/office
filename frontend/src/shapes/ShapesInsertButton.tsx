// THE shapes INSERTION control of the office module — the ribbon dropdown that
// opens the shared gallery and arms/inserts the chosen geometry.
//
// Generalised from the spreadsheet's Insertion ▸ Illustrations ▸ Formes button so
// EVERY sub-editor (slides, board, diagrams…) inserts shapes the same way, with
// the same gallery and the same draw-to-create behaviour, instead of each one
// reimplementing a picker. The editor only supplies `onPick(kind)` — how it arms
// its own canvas (draw the box) is its business.

import { useRef, useState } from 'react'
import { Shapes, ChevronDown } from 'lucide-react'
import type { TFunction } from 'i18next'
import { AnchoredPopover } from '@ui'
import type { RibbonGroup, RibbonItem } from '../ribbon/types'
import { ShapeGallery } from './ShapeGallery'
import type { ShapeKind } from './catalog'

export interface ShapesInsertButtonProps {
  /** Arm / insert the chosen geometry in the host editor. */
  onPick: (kind: ShapeKind) => void
  t?: TFunction
  /** Currently armed kind — highlighted in the gallery. */
  current?: string
  /** Button label (default « Formes »). */
  label?: string
}

/**
 * The « Formes » ribbon button: a large split-style trigger that drops the shared
 * `ShapeGallery`. Self-contained (owns the popover state), so it drops straight
 * into a ribbon `custom` item.
 */
export function ShapesInsertButton({ onPick, t, current, label }: ShapesInsertButtonProps) {
  const ref = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const text = label ?? (t ? t('sheet_shapes', { defaultValue: 'Formes' }) : 'Formes')
  return (
    <div>
      <button
        ref={ref}
        onClick={() => setOpen(o => !o)}
        title={text}
        className={`h-[52px] px-2 flex flex-col items-center justify-center gap-1 rounded hover:bg-[#e8eaed] ${
          open ? 'bg-[#e8f0fe] text-primary' : ''
        }`}
      >
        <Shapes size={16} />
        <span className="text-[10px] flex items-center">{text} <ChevronDown size={9} /></span>
      </button>
      <AnchoredPopover anchorRef={ref} open={open} onClose={() => setOpen(false)}>
        <ShapeGallery current={current} t={t} onPick={k => { onPick(k); setOpen(false) }} />
      </AnchoredPopover>
    </div>
  )
}

export interface ShapesIllustrationArgs extends ShapesInsertButtonProps {
  /** Heading of the group (default « Illustrations »). */
  groupLabel?: string
  /** Items placed BEFORE the shapes button (e.g. « Images »). */
  leadingItems?: RibbonItem[]
  /** Items placed AFTER the shapes button (e.g. « Icônes », « SmartArt »). */
  trailingItems?: RibbonItem[]
}

/**
 * The « Illustrations » ribbon group, holding the shapes dropdown (and whatever
 * the editor adds around it). Every editor that draws shapes puts this group in
 * its Insertion tab, so the shapes button lives in the same place everywhere.
 */
export function shapesIllustrationGroup(a: ShapesIllustrationArgs): RibbonGroup {
  // « Formes » = gros bouton à MENU STANDARD (`kind: 'menu' size: 'large'`), PAS un
  // `custom` maison : il passe ainsi par le rendu partagé `LargeButtonContent` et
  // suit exactement les règles des gros boutons (icône 24px, 2 lignes réservées,
  // chevron sous le libellé, hauteur = 3 positions) comme « Tableau ». La galerie
  // reste bespoke via `menuRender`.
  return {
    id: 'illustrations',
    label: a.groupLabel ?? (a.t ? a.t('sheet_grp_illus', { defaultValue: 'Illustrations' }) : 'Illustrations'),
    items: [
      ...(a.leadingItems ?? []),
      {
        id: 'shapes',
        kind: 'menu',
        size: 'large',
        icon: <Shapes size={22} />,
        label: a.label ?? (a.t ? a.t('sheet_shapes', { defaultValue: 'Formes' }) : 'Formes'),
        active: !!a.current,
        menuRender: (close: () => void) => (
          <ShapeGallery current={a.current} t={a.t} onPick={k => { a.onPick(k); close() }} />
        ),
      },
      ...(a.trailingItems ?? []),
    ],
  }
}
