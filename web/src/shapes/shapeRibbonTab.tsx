// THE shape-configuration ribbon tab of the office module — the « Forme » /
// "Outils de dessin" contextual tab, shared by every sub-editor that draws the
// catalogue shapes (spreadsheet, presentations, whiteboard, diagrams).
//
// It is DATA-DRIVEN: an editor hands over a small adapter (the selected shape's
// fill/outline, and callbacks to restyle / align / order / rotate / edit text /
// delete). This module knows nothing about any editor's selection model — that
// is what makes it reusable. The spreadsheet builds its Image/Chart/Equation
// tabs from the SAME helpers (arrangeGroup / objectGroup / styleGroup), so a
// shape is configured identically everywhere.

import type { ReactNode } from 'react'
import {
  RotateCcw, RotateCw, BringToFront, SendToBack, Trash2,
  Paintbrush, Settings2, Type, PaintBucket, Square,
  AlignStartVertical, AlignCenterVertical, AlignEndVertical,
  AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal,
  AlignHorizontalSpaceAround, AlignVerticalSpaceAround,
} from 'lucide-react'
import type { TFunction } from 'i18next'
import { ColorSwatchPicker } from '@ui'
import type { RibbonGroup, RibbonItem, RibbonTab } from '../ribbon/types'

/** Office blue — the accent of every object/shape contextual band. */
export const SHAPE_TAB_ACCENT = '#1a73e8'

/** Alignment / distribution of a multi-selection (relative to its bounding box). */
export type ShapeAlignMode =
  | 'left' | 'centerH' | 'right'
  | 'top' | 'middleV' | 'bottom'
  | 'distH' | 'distV'

/** Where in the paint order to move the selection. */
export type ShapeOrderOp = 'front' | 'forward' | 'backward' | 'back'

/** Fill / outline of a shape — the slice the style group reads and writes. */
export interface ShapeStyleLike {
  fill?: string
  border?: string
  borderWidth?: number
}

// ── Reusable groups (also consumed by the spreadsheet's object tabs) ──────────

/** « Organiser » — align / distribute / paint order / quarter-turns. */
export interface ArrangeToolArgs {
  t: TFunction
  /** How many objects are selected (align needs ≥2, distribute ≥3). */
  count: number
  align: (mode: ShapeAlignMode) => void
  order: (op: ShapeOrderOp) => void
  rotate: (deg: number, reset?: boolean) => void
}

/** « Objet » — format pane, size dialog, deletion. */
export interface ObjectToolArgs {
  t: TFunction
  count: number
  /** Optional: an editor without a format pane simply drops the button. */
  openFormat?: () => void
  openSizeProps?: () => void
  remove: () => void
}

/** « Style » — fill / outline swatches. */
export interface StyleToolArgs {
  t: TFunction
  style?: ShapeStyleLike
  setStyle?: (patch: ShapeStyleLike) => void
}

/** A colour button whose menu is the shared @ui swatch picker. */
export function colourItem(
  id: string, label: string, icon: ReactNode, current: string | undefined,
  t: TFunction, onPick: (hex: string) => void, onNone: () => void, noneLabel: string,
): RibbonItem {
  return {
    id, kind: 'menu', icon, label, size: 'small',
    menuRender: close => (
      <div className="p-1">
        <button
          type="button"
          className="flex items-center gap-2 w-full h-8 px-2 rounded hover:bg-hover text-left text-text-primary"
          onClick={() => { onNone(); close() }}
        >
          <span aria-hidden className="inline-block w-4 h-4 rounded-sm border border-border relative overflow-hidden">
            <span className="absolute inset-0 bg-[linear-gradient(to_top_right,transparent_45%,#d93025_45%,#d93025_55%,transparent_55%)]" />
          </span>
          {noneLabel}
        </button>
        <ColorSwatchPicker
          color={current && current !== 'none' ? current : '#ffffff'}
          t={t}
          onChange={hex => { onPick(hex); close() }}
          onClose={close}
        />
      </div>
    ),
  }
}

export function arrangeGroup(a: ArrangeToolArgs): RibbonGroup {
  const { t } = a
  return {
    id: 'obj_arrange',
    label: t('sheet_grp_arrange', { defaultValue: 'Organiser' }),
    items: [
      {
        id: 'align', kind: 'menu', size: 'large',
        icon: <AlignCenterHorizontal size={18} />,
        label: t('sheet_obj_align_menu', { defaultValue: 'Aligner' }),
        // Alignment is relative to the selection's bounding box: from two objects up.
        disabled: a.count < 2,
        menuRender: close => (
          <div className="flex flex-col min-w-[220px] p-1">
            {([
              ['left', 'sheet_obj_align_left', 'Aligner à gauche', <AlignStartVertical key="i" size={15} />],
              ['centerH', 'sheet_obj_align_center', 'Centrer', <AlignCenterVertical key="i" size={15} />],
              ['right', 'sheet_obj_align_right', 'Aligner à droite', <AlignEndVertical key="i" size={15} />],
              ['top', 'sheet_obj_align_top', 'Aligner en haut', <AlignStartHorizontal key="i" size={15} />],
              ['middleV', 'sheet_obj_align_middle', 'Centrer verticalement', <AlignCenterHorizontal key="i" size={15} />],
              ['bottom', 'sheet_obj_align_bottom', 'Aligner en bas', <AlignEndHorizontal key="i" size={15} />],
            ] as [ShapeAlignMode, string, string, ReactNode][]).map(([mode, key, def, icon]) => (
              <button
                key={mode} type="button"
                className="flex items-center gap-2 h-8 px-2 rounded hover:bg-hover text-left text-text-primary"
                onClick={() => { a.align(mode); close() }}
              >
                {icon}{t(key, { defaultValue: def })}
              </button>
            ))}
            <div className="my-1 h-px bg-border" />
            {([
              ['distH', 'sheet_obj_dist_h', 'Distribuer horizontalement', <AlignHorizontalSpaceAround key="i" size={15} />],
              ['distV', 'sheet_obj_dist_v', 'Distribuer verticalement', <AlignVerticalSpaceAround key="i" size={15} />],
            ] as [ShapeAlignMode, string, string, ReactNode][]).map(([mode, key, def, icon]) => (
              <button
                key={mode} type="button" disabled={a.count < 3}
                className="flex items-center gap-2 h-8 px-2 rounded hover:bg-hover text-left text-text-primary disabled:opacity-50"
                onClick={() => { a.align(mode); close() }}
              >
                {icon}{t(key, { defaultValue: def })}
              </button>
            ))}
          </div>
        ),
      },
      { id: 'front', kind: 'button', icon: <BringToFront size={15} />, label: t('sheet_img_front', { defaultValue: 'Avancer' }), onClick: () => a.order('front') },
      { id: 'back', kind: 'button', icon: <SendToBack size={15} />, label: t('sheet_img_back', { defaultValue: 'Reculer' }), onClick: () => a.order('back') },
      { id: 'sep1', kind: 'separator' },
      { id: 'rotl', kind: 'button', icon: <RotateCcw size={15} />, label: t('sheet_img_rot_left', { defaultValue: 'Gauche 90°' }), onClick: () => a.rotate(-90) },
      { id: 'rotr', kind: 'button', icon: <RotateCw size={15} />, label: t('sheet_img_rot_right', { defaultValue: 'Droite 90°' }), onClick: () => a.rotate(90) },
    ],
  }
}

export function objectGroup(a: ObjectToolArgs): RibbonGroup {
  const { t } = a
  const many = a.count > 1
  const items: RibbonItem[] = []
  if (a.openFormat) {
    items.push({
      id: 'format', kind: 'button', size: 'large', icon: <Paintbrush size={18} />,
      label: t('sheet_obj_format_short', { defaultValue: 'Format' }),
      // The dialog edits ONE object; with a group selected it acts on the active one.
      tooltip: many ? t('sheet_obj_active_only', { defaultValue: 'S’applique à l’objet actif' }) : undefined,
      onClick: a.openFormat,
    })
  }
  if (a.openSizeProps) {
    items.push({
      id: 'sizeprops', kind: 'button', icon: <Settings2 size={15} />,
      label: t('sheet_obj_size_props_short', { defaultValue: 'Taille et propriétés' }),
      tooltip: many ? t('sheet_obj_active_only', { defaultValue: 'S’applique à l’objet actif' }) : undefined,
      onClick: a.openSizeProps,
    })
  }
  items.push({
    id: 'del', kind: 'button', icon: <Trash2 size={15} />,
    label: many
      ? t('sheet_obj_delete_group', { defaultValue: 'Supprimer les objets' })
      : t('sheet_obj_delete', { defaultValue: 'Supprimer' }),
    onClick: a.remove,
  })
  return { id: 'obj_common', label: t('sheet_grp_object', { defaultValue: 'Objet' }), items }
}

export function styleGroup(a: StyleToolArgs, fillable: boolean): RibbonGroup | null {
  if (!a.setStyle) return null
  const { t } = a
  const items: RibbonItem[] = []
  if (fillable) {
    items.push(colourItem(
      'fill', t('sheet_obj_fill', { defaultValue: 'Remplissage' }), <PaintBucket size={15} />, a.style?.fill, t,
      hex => a.setStyle?.({ fill: hex }), () => a.setStyle?.({ fill: 'none' }),
      t('sheet_obj_no_fill', { defaultValue: 'Aucun remplissage' }),
    ))
  }
  items.push(colourItem(
    'outline', t('sheet_obj_outline', { defaultValue: 'Contour' }), <Square size={15} />, a.style?.border, t,
    hex => a.setStyle?.({ border: hex }), () => a.setStyle?.({ border: 'none' }),
    t('sheet_obj_no_outline', { defaultValue: 'Aucun contour' }),
  ))
  return { id: 'obj_style', label: t('sheet_grp_style', { defaultValue: 'Style' }), items }
}

// ── The standalone « Forme » tab ──────────────────────────────────────────────

export interface ShapeRibbonAdapter extends ArrangeToolArgs, ObjectToolArgs, StyleToolArgs {
  /** Whether the tab is currently shown (a shape is selected). */
  visible: boolean
  /** Enter caption editing on the active shape; omitted → no « Texte » group. */
  onEditText?: () => void
  /** Accent of the contextual band (default office blue). */
  accent?: string
  /** Tab id — override to avoid a clash with another contextual tab. */
  id?: string
}

/** Ids of the tab's default groups — the handles an editor overrides by. */
export const SHAPE_TAB_GROUP = {
  style: 'obj_style',
  text: 'shape_text',
  arrange: 'obj_arrange',
  object: 'obj_common',
} as const

/**
 * How a module tailors the shared « Forme » tab to its own needs. Every field is
 * optional: with none, the caller gets the canonical Office tab. This is what lets
 * the tab be THE default configuration surface for shapes while each editor still
 * adds its specifics (a diagram's label styling, a slide's opacity/shadow…),
 * replaces a group, patches a button, or hides what it does not support.
 */
export interface ShapeTabOverride {
  /** Rename the tab and its contextual band. */
  label?: string
  groupLabel?: string
  /** Drop default groups by id (see SHAPE_TAB_GROUP). */
  hideGroups?: string[]
  /** Drop items ANYWHERE by id (e.g. 'edittext', 'sizeprops'). */
  hideItems?: string[]
  /** Replace a default group wholesale, keyed by the default group's id. */
  replaceGroups?: Record<string, RibbonGroup>
  /** Append items to a default group, keyed by the group's id. */
  addItems?: Record<string, RibbonItem[]>
  /** Patch a default item by id (shallow-merged over the default). */
  itemOverrides?: Record<string, Partial<RibbonItem>>
  /** Extra groups appended after the defaults (an editor's own commands). */
  extraGroups?: RibbonGroup[]
  /** Last-resort full control: receives the assembled groups, returns the final list. */
  transform?: (groups: RibbonGroup[]) => RibbonGroup[]
}

/** Apply an override to the assembled default groups. Pure, order-deterministic. */
function applyOverride(groups: RibbonGroup[], o: ShapeTabOverride): RibbonGroup[] {
  let out = groups.map(g => (o.replaceGroups?.[g.id] ?? g))
  if (o.hideGroups?.length) {
    const hidden = new Set(o.hideGroups)
    out = out.filter(g => !hidden.has(g.id))
  }
  const hiddenItems = new Set(o.hideItems ?? [])
  out = out.map(g => {
    let items = g.items
    const extra = o.addItems?.[g.id]
    if (extra?.length) items = [...items, ...extra]
    items = items
      .filter(it => !hiddenItems.has(it.id))
      .map(it => {
        const patch = o.itemOverrides?.[it.id]
        return patch ? ({ ...it, ...patch } as RibbonItem) : it
      })
    return items === g.items ? g : { ...g, items }
  })
  if (o.extraGroups?.length) out = [...out, ...o.extraGroups]
  if (o.transform) out = o.transform(out)
  return out
}

/**
 * The « Forme » contextual ribbon tab: Style (fill / outline), Texte (edit the
 * caption), Organiser (align / order / rotate), Objet (format / size / delete).
 * `visible` is controlled by the caller so the tab strip never remounts.
 *
 * `override` lets a module ADD / REPLACE / PATCH / HIDE groups and items so the
 * SAME tab carries each editor's specifics without forking it (see ShapeTabOverride).
 */
export function shapeRibbonTab(a: ShapeRibbonAdapter, override?: ShapeTabOverride): RibbonTab {
  const { t } = a
  const groups: RibbonGroup[] = []
  const style = styleGroup(a, true)
  if (style) groups.push(style)
  if (a.onEditText) {
    groups.push({
      id: SHAPE_TAB_GROUP.text,
      label: t('sheet_grp_shape_text', { defaultValue: 'Texte' }),
      items: [
        {
          id: 'edittext', kind: 'button', icon: <Type size={15} />, size: 'large',
          label: t('sheet_shape_edit_text', { defaultValue: 'Modifier le texte' }),
          // Captioning is a per-shape gesture: one shape, not a group.
          disabled: a.count !== 1,
          onClick: () => a.onEditText?.(),
        },
      ],
    })
  }
  groups.push(arrangeGroup(a), objectGroup(a))

  const finalGroups = override ? applyOverride(groups, override) : groups

  return {
    id: a.id ?? 'shape',
    label: override?.label ?? t('sheet_tab_shape', { defaultValue: 'Forme' }),
    contextual: {
      accent: a.accent ?? SHAPE_TAB_ACCENT,
      groupLabel: override?.groupLabel ?? t('sheet_shape_tools', { defaultValue: 'Outils de dessin' }),
    },
    visible: a.visible,
    groups: finalGroups,
  }
}
