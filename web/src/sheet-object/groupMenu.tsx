// The context menu of a MULTI-SELECTION — what Excel shows when several floating
// objects are selected at once.
//
// The single-object menus (chartMenu / imageMenu / shapeMenu) stay untouched: they
// are shown as soon as exactly ONE object is selected. Beyond that, this builder
// takes over and offers only what can be applied to EVERY selected object:
//
//   icon row (couper · copier · dupliquer · supprimer)
//   ┊ Aligner ▸ (6 alignements + 2 répartitions)
//   ┊ Premier plan ▸ · Arrière-plan ▸ · Rotation ▸
//   ┊ Format de l'objet… / Taille et propriétés… — of the ACTIVE object only, and
//     therefore labelled with it, never silently applied to the whole group.
//
// When the selection is HOMOGENEOUS the kind's own quick formatting comes back in
// block 1 (fill / outline swatches write through to every object of the group);
// when the natures DIFFER those swatches are dropped — a picture has no fill, and a
// mixed group has no common style to show.

import {
  Scissors, Copy, Clipboard, CopyPlus, Trash2, Paintbrush, Settings2,
  RotateCw, BringToFront, SendToBack,
  AlignStartVertical, AlignCenterVertical, AlignEndVertical,
  AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal,
  AlignHorizontalSpaceAround, AlignVerticalSpaceAround, PaintBucket, Square,
} from 'lucide-react'
import type { ReactNode } from 'react'
import type { TFunction } from 'i18next'
import type { MenuItem, MenuTheme } from '@ui'
import { iconRowItems, type MenuIconAction } from './MenuIconRow'
import type { MiniControl } from './MiniToolbar'
import type { ObjKind, ObjOrderOp, ObjStyle } from './types'
import type { AlignMode } from './selection'

export interface GroupMenuArgs {
  /** How many objects are selected (always ≥ 2 when this menu is used). */
  count: number
  /** The natures present in the selection, in a stable order. */
  kinds: ObjKind[]
  /** Label of the ACTIVE object ("Forme 2") — the one the single-object dialogs open on. */
  activeLabel: string
  t: TFunction

  cut: () => void
  copy: () => void
  /** Paste whatever the object clipboard holds (possibly several objects). */
  paste: () => void
  /** True when the object clipboard is non-empty. */
  canPaste: boolean
  duplicate?: () => void
  remove: () => void
  /** Apply an alignment / distribution to the whole group. */
  align: (mode: AlignMode) => void
  /** Move every selected object inside its own type's paint order. */
  order: (op: ObjOrderOp) => void
  /** Quarter-turn / reset, applied to each object around its own centre. */
  rotate: (deg: number, reset?: boolean) => void
  /** « Format de l'objet… » of the active object. */
  openFormat: () => void
  /** « Taille et propriétés… » of the active object. */
  openSizeProps: () => void

  /**
   * Common fill / outline of the group, and the writer that applies a change to
   * ALL of them. Provided only for a homogeneous selection whose kind has them
   * (charts and shapes); pictures contribute an outline but no fill.
   */
  style?: ObjStyle
  setStyle?: (patch: ObjStyle) => void
  /** True when the kind supports a fill (false for pictures). */
  fillable?: boolean

  theme?: MenuTheme
}

/** Icon strip: the clipboard family, applied to the whole selection. */
function groupIconRow(a: GroupMenuArgs): MenuIconAction[] {
  const { t } = a
  return [
    { id: 'cut', label: t('sheet_obj_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', icon: <Scissors size={16} />, onClick: a.cut },
    { id: 'copy', label: t('sheet_obj_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', icon: <Copy size={16} />, onClick: a.copy },
    { id: 'paste', label: t('sheet_obj_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', icon: <Clipboard size={16} />, disabled: !a.canPaste, onClick: a.paste },
    ...(a.duplicate
      ? [{ id: 'dup', label: t('sheet_obj_duplicate', { defaultValue: 'Dupliquer' }), icon: <CopyPlus size={16} />, onClick: () => a.duplicate?.() }]
      : []),
    { id: 'del', label: t('sheet_obj_delete', { defaultValue: 'Supprimer' }), shortcut: 'Suppr', icon: <Trash2 size={16} />, danger: true, onClick: a.remove },
  ]
}

/** « Aligner » — the submenu Excel puts under Format ▸ Aligner for a group. */
export function alignItem(a: GroupMenuArgs): MenuItem {
  const { t } = a
  const entry = (mode: AlignMode, key: string, def: string, icon: ReactNode): MenuItem =>
    ({ type: 'action', label: t(key, { defaultValue: def }), icon, onClick: () => a.align(mode) })
  return {
    type: 'submenu',
    label: t('sheet_obj_align_menu', { defaultValue: 'Aligner' }),
    icon: <AlignCenterHorizontal size={15} />,
    items: [
      entry('left', 'sheet_obj_align_left', 'Aligner à gauche', <AlignStartVertical size={15} />),
      entry('centerH', 'sheet_obj_align_center', 'Centrer', <AlignCenterVertical size={15} />),
      entry('right', 'sheet_obj_align_right', 'Aligner à droite', <AlignEndVertical size={15} />),
      { type: 'separator' },
      entry('top', 'sheet_obj_align_top', 'Aligner en haut', <AlignStartHorizontal size={15} />),
      entry('middleV', 'sheet_obj_align_middle', 'Centrer verticalement', <AlignCenterHorizontal size={15} />),
      entry('bottom', 'sheet_obj_align_bottom', 'Aligner en bas', <AlignEndHorizontal size={15} />),
      { type: 'separator' },
      // Distribution needs three objects to have any effect; below that Excel
      // greys the entries out rather than hiding them.
      {
        type: 'action', label: t('sheet_obj_dist_h', { defaultValue: 'Distribuer horizontalement' }),
        icon: <AlignHorizontalSpaceAround size={15} />, disabled: a.count < 3, onClick: () => a.align('distH'),
      },
      {
        type: 'action', label: t('sheet_obj_dist_v', { defaultValue: 'Distribuer verticalement' }),
        icon: <AlignVerticalSpaceAround size={15} />, disabled: a.count < 3, onClick: () => a.align('distV'),
      },
    ],
  }
}

/** « Premier plan » / « Arrière-plan », applied to every object of the group. */
function orderItems(a: GroupMenuArgs): MenuItem[] {
  const { t } = a
  const entry = (key: string, def: string, op: ObjOrderOp): MenuItem =>
    ({ type: 'action', label: t(key, { defaultValue: def }), onClick: () => a.order(op) })
  return [
    { type: 'submenu', label: t('sheet_obj_front_menu', { defaultValue: 'Premier plan' }), icon: <BringToFront size={15} />, items: [
      entry('sheet_obj_order_front', 'Mettre au premier plan', 'front'),
      entry('sheet_obj_order_forward', 'Avancer d’un plan', 'forward'),
    ] },
    { type: 'submenu', label: t('sheet_obj_back_menu', { defaultValue: 'Arrière-plan' }), icon: <SendToBack size={15} />, items: [
      entry('sheet_obj_order_back', 'Mettre en arrière-plan', 'back'),
      entry('sheet_obj_order_backward', 'Reculer d’un plan', 'backward'),
    ] },
  ]
}

/** Human summary of the selection, shown as the menu's first line. */
export function groupHeaderLabel(a: GroupMenuArgs): string {
  const { t, count, kinds } = a
  if (kinds.length > 1) {
    return t('sheet_obj_group_mixed', { defaultValue: '{{count}} objets sélectionnés', count })
  }
  const key = kinds[0] === 'chart' ? 'sheet_obj_group_charts'
    : kinds[0] === 'image' ? 'sheet_obj_group_images'
      : 'sheet_obj_group_shapes'
  const def = kinds[0] === 'chart' ? '{{count}} graphiques sélectionnés'
    : kinds[0] === 'image' ? '{{count}} images sélectionnées'
      : '{{count}} formes sélectionnées'
  return t(key, { defaultValue: def, count })
}

/** Block 2 — the group's context menu. */
export function buildGroupMenu(a: GroupMenuArgs): MenuItem[] {
  const { t } = a
  return [
    ...iconRowItems(groupIconRow(a), a.theme ?? 'light'),
    // Non-clickable heading: which objects these commands are about.
    { type: 'action', label: groupHeaderLabel(a), disabled: true, onClick: () => {} },

    { type: 'separator' },
    alignItem(a),
    ...orderItems(a),
    {
      type: 'submenu', label: t('sheet_obj_rotate_menu', { defaultValue: 'Rotation' }), icon: <RotateCw size={15} />,
      items: [
        { type: 'action', label: t('sheet_obj_rotate_right', { defaultValue: 'Pivoter à droite 90°' }), onClick: () => a.rotate(90) },
        { type: 'action', label: t('sheet_obj_rotate_left', { defaultValue: 'Pivoter à gauche 90°' }), onClick: () => a.rotate(-90) },
        { type: 'action', label: t('sheet_obj_rotate_reset', { defaultValue: 'Réinitialiser la rotation' }), onClick: () => a.rotate(0, true) },
      ],
    },

    { type: 'separator' },
    // Explicitly scoped to the active object — a group has no single size to edit.
    {
      type: 'action',
      label: t('sheet_obj_size_props_of', { defaultValue: 'Taille et propriétés… ({{name}})', name: a.activeLabel }),
      icon: <Settings2 size={15} />, onClick: a.openSizeProps,
    },
    {
      type: 'action',
      label: t('sheet_obj_format_of', { defaultValue: 'Format de l’objet… ({{name}})', name: a.activeLabel }),
      icon: <Paintbrush size={15} />, onClick: a.openFormat,
    },

    { type: 'separator' },
    {
      type: 'action', label: t('sheet_obj_delete_group', { defaultValue: 'Supprimer les objets' }),
      icon: <Trash2 size={15} />, danger: true, shortcut: 'Suppr', onClick: a.remove,
    },
  ]
}

/**
 * Block 1 — the mini bar of a group. Quick formatting only survives a HOMOGENEOUS
 * selection: `setStyle` is what the host provides in that case, and it writes to
 * every selected object. Mixed natures get alignment instead, which always applies.
 */
export function groupMiniControls(a: GroupMenuArgs): MiniControl[] {
  const { t } = a
  const controls: MiniControl[] = []
  if (a.setStyle) {
    if (a.fillable) {
      controls.push({
        id: 'fill', kind: 'color',
        label: t('sheet_obj_fill', { defaultValue: 'Remplissage' }),
        icon: <PaintBucket size={14} />,
        color: a.style?.fill ?? '',
        onPick: hex => a.setStyle?.({ fill: hex }),
        noneLabel: t('sheet_obj_no_fill', { defaultValue: 'Aucun remplissage' }),
        onNone: () => a.setStyle?.({ fill: 'none' }),
      })
    }
    controls.push({
      id: 'border', kind: 'color',
      label: t('sheet_obj_outline', { defaultValue: 'Contour' }),
      icon: <Square size={14} />,
      color: a.style?.border ?? '',
      onPick: hex => a.setStyle?.({ border: hex }),
      noneLabel: t('sheet_obj_no_outline', { defaultValue: 'Aucun contour' }),
      onNone: () => a.setStyle?.({ border: 'none' }),
    })
  }
  // Alignment is the one group command worth a single click, whatever the natures.
  controls.push({
    id: 'align', kind: 'menu',
    label: t('sheet_obj_align_menu', { defaultValue: 'Aligner' }),
    icon: <AlignCenterHorizontal size={14} />,
    render: close => (
      <div className="flex flex-col min-w-[196px]" role="group">
        {([
          ['left', 'sheet_obj_align_left', 'Aligner à gauche', <AlignStartVertical key="i" size={15} />],
          ['centerH', 'sheet_obj_align_center', 'Centrer', <AlignCenterVertical key="i" size={15} />],
          ['right', 'sheet_obj_align_right', 'Aligner à droite', <AlignEndVertical key="i" size={15} />],
          ['top', 'sheet_obj_align_top', 'Aligner en haut', <AlignStartHorizontal key="i" size={15} />],
          ['middleV', 'sheet_obj_align_middle', 'Centrer verticalement', <AlignCenterHorizontal key="i" size={15} />],
          ['bottom', 'sheet_obj_align_bottom', 'Aligner en bas', <AlignEndHorizontal key="i" size={15} />],
        ] as [AlignMode, string, string, React.ReactNode][]).map(([mode, key, def, icon]) => (
          <button
            key={mode}
            type="button"
            className="flex items-center gap-2 h-8 px-2 rounded hover:bg-hover text-left text-text-primary"
            onClick={() => { a.align(mode); close() }}
          >
            {icon}
            <span className="whitespace-nowrap">{t(key, { defaultValue: def })}</span>
          </button>
        ))}
      </div>
    ),
  })
  return controls
}
