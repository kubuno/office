// CONTEXTUAL RIBBON TABS of the floating objects — Office's "Outils Image" /
// "Outils de dessin" / "Outils de graphique", driven by the MULTI-SELECTION.
//
// One rule decides which tab appears:
//
//   • the selection is homogeneous  → that kind's tab ("Image", "Forme",
//     "Graphique"), whatever the number of objects. Commands that only make sense
//     on ONE object (crop a picture, edit a chart's data, type a shape's caption)
//     are disabled — not hidden — as soon as the selection holds several, exactly
//     as Excel greys them out;
//   • the natures DIFFER → the kind tabs all disappear and a single generic
//     « Format » tab takes their place, holding what applies to any object:
//     alignment, distribution, paint order, rotation, size, deletion.
//
// The tabs are built here rather than inline in the editor so the rule lives in one
// place and the 8000-line host does not grow another 200 lines.

import type { ReactNode } from 'react'
import {
  Crop, RefreshCw, RotateCcw, RotateCw, BringToFront, SendToBack, Trash2,
  Paintbrush, Settings2, Type, PenLine, Table2, PaintBucket, Square, Sigma,
  AlignStartVertical, AlignCenterVertical, AlignEndVertical,
  AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal,
  AlignHorizontalSpaceAround, AlignVerticalSpaceAround,
} from 'lucide-react'
import type { TFunction } from 'i18next'
import { ColorSwatchPicker } from '@ui'
import type { RibbonGroup, RibbonItem, RibbonTab } from '../ribbon/types'
import type { ObjKind, ObjOrderOp, ObjStyle } from './types'
import type { AlignMode } from './selection'

/** Accent of the contextual band — the office blue every object tab shares. */
const OBJ_ACCENT = '#1a73e8'

export interface ObjectRibbonArgs {
  t: TFunction
  /** Number of selected objects (0 = no tab at all). */
  count: number
  /** Natures present in the selection. */
  kinds: ObjKind[]
  /** The only nature, or null when the selection is empty or mixed. */
  soleKind: ObjKind | null

  // ── Generic commands (always applied to the WHOLE selection) ────────────────
  align: (mode: AlignMode) => void
  order: (op: ObjOrderOp) => void
  rotate: (deg: number, reset?: boolean) => void
  remove: () => void
  /** Of the ACTIVE object — a group has no single format pane. */
  openFormat: () => void
  openSizeProps: () => void

  // ── Picture-only ───────────────────────────────────────────────────────────
  onCrop?: () => void
  cropping?: boolean
  onResetImage?: () => void

  // ── Shape-only ─────────────────────────────────────────────────────────────
  onEditShapeText?: () => void
  /** Fill / outline of the group, written to EVERY selected object. */
  style?: ObjStyle
  setStyle?: (patch: ObjStyle) => void

  // ── Chart-only ─────────────────────────────────────────────────────────────
  onEditChartData?: () => void
  onEditChartType?: () => void

  // ── Equation-only ──────────────────────────────────────────────────────────
  onEditEquation?: () => void
}

/** A colour button whose menu is the shared @ui swatch picker. */
function colourItem(
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

/** « Organiser » — alignment, distribution, paint order and rotation of the group. */
function arrangeGroup(a: ObjectRibbonArgs): RibbonGroup {
  const { t } = a
  return {
    id: 'obj_arrange',
    label: t('sheet_grp_arrange', { defaultValue: 'Organiser' }),
    items: [
      {
        id: 'align', kind: 'menu', size: 'large',
        icon: <AlignCenterHorizontal size={18} />,
        label: t('sheet_obj_align_menu', { defaultValue: 'Aligner' }),
        // Alignment is relative to the selection's bounding box, so it is only
        // meaningful from two objects up.
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
            ] as [AlignMode, string, string, ReactNode][]).map(([mode, key, def, icon]) => (
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
            ] as [AlignMode, string, string, ReactNode][]).map(([mode, key, def, icon]) => (
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

/** « Objet » — format pane, size dialog, deletion. Common to every tab. */
function objectGroup(a: ObjectRibbonArgs): RibbonGroup {
  const { t } = a
  const many = a.count > 1
  return {
    id: 'obj_common',
    label: t('sheet_grp_object', { defaultValue: 'Objet' }),
    items: [
      {
        id: 'format', kind: 'button', size: 'large', icon: <Paintbrush size={18} />,
        label: t('sheet_obj_format_short', { defaultValue: 'Format' }),
        // Both dialogs edit ONE object; with a group selected they act on the
        // active one, which the tooltip says out loud.
        tooltip: many ? t('sheet_obj_active_only', { defaultValue: 'S’applique à l’objet actif' }) : undefined,
        onClick: a.openFormat,
      },
      {
        id: 'sizeprops', kind: 'button', icon: <Settings2 size={15} />,
        label: t('sheet_obj_size_props_short', { defaultValue: 'Taille et propriétés' }),
        tooltip: many ? t('sheet_obj_active_only', { defaultValue: 'S’applique à l’objet actif' }) : undefined,
        onClick: a.openSizeProps,
      },
      {
        id: 'del', kind: 'button', icon: <Trash2 size={15} />,
        label: many
          ? t('sheet_obj_delete_group', { defaultValue: 'Supprimer les objets' })
          : t('sheet_obj_delete', { defaultValue: 'Supprimer' }),
        onClick: a.remove,
      },
    ],
  }
}

/** Fill / outline swatches — shapes (and, without fill, pictures). */
function styleGroup(a: ObjectRibbonArgs, fillable: boolean): RibbonGroup | null {
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

/**
 * The contextual tabs for the current selection. Always returns the four tabs (the
 * ribbon keys on `visible`), so the tab strip never remounts as the selection
 * changes — only the visible flags flip.
 */
export function objectRibbonTabs(a: ObjectRibbonArgs): RibbonTab[] {
  const { t } = a
  const single = a.count === 1
  const homogeneous = (k: ObjKind) => a.count > 0 && a.soleKind === k
  const mixed = a.count > 1 && a.kinds.length > 1

  const imageTab: RibbonTab = {
    id: 'image',
    label: t('sheet_tab_image', { defaultValue: 'Image' }),
    contextual: { accent: OBJ_ACCENT, groupLabel: t('sheet_img_tools', { defaultValue: 'Outils Image' }) },
    visible: homogeneous('image'),
    groups: [
      {
        id: 'img_crop', label: t('sheet_grp_crop', { defaultValue: 'Rogner' }),
        items: [
          {
            id: 'crop', kind: 'toggle', icon: <Crop size={15} />, size: 'large',
            label: t('sheet_img_crop', { defaultValue: 'Rogner' }),
            active: !!a.cropping,
            // Cropping is a per-picture gesture: it needs one picture, not a group.
            disabled: !single || !a.onCrop,
            onClick: () => a.onCrop?.(),
          },
          {
            id: 'reset', kind: 'button', icon: <RefreshCw size={15} />,
            label: t('sheet_img_reset', { defaultValue: 'Réinitialiser' }),
            disabled: !a.onResetImage,
            onClick: () => a.onResetImage?.(),
          },
        ],
      },
      arrangeGroup(a),
      objectGroup(a),
    ],
  }

  const styleG = styleGroup(a, true)
  const shapeTab: RibbonTab = {
    id: 'shape',
    label: t('sheet_tab_shape', { defaultValue: 'Forme' }),
    contextual: { accent: OBJ_ACCENT, groupLabel: t('sheet_shape_tools', { defaultValue: 'Outils de dessin' }) },
    visible: homogeneous('shape'),
    groups: [
      ...(styleG ? [styleG] : []),
      {
        id: 'shape_text', label: t('sheet_grp_shape_text', { defaultValue: 'Texte' }),
        items: [
          {
            id: 'edittext', kind: 'button', icon: <Type size={15} />, size: 'large',
            label: t('sheet_shape_edit_text', { defaultValue: 'Modifier le texte' }),
            disabled: !single || !a.onEditShapeText,
            onClick: () => a.onEditShapeText?.(),
          },
        ],
      },
      arrangeGroup(a),
      objectGroup(a),
    ],
  }

  const chartTab: RibbonTab = {
    id: 'chart',
    label: t('sheet_tab_chart', { defaultValue: 'Graphique' }),
    contextual: { accent: OBJ_ACCENT, groupLabel: t('sheet_chart_tools', { defaultValue: 'Outils de graphique' }) },
    visible: homogeneous('chart'),
    groups: [
      {
        id: 'chart_edit', label: t('sheet_grp_chart_data', { defaultValue: 'Données' }),
        items: [
          {
            id: 'cdata', kind: 'button', icon: <Table2 size={18} />, size: 'large',
            label: t('sheet_chart_edit_data', { defaultValue: 'Modifier les données' }),
            disabled: !single || !a.onEditChartData,
            onClick: () => a.onEditChartData?.(),
          },
          {
            id: 'ctype', kind: 'button', icon: <PenLine size={15} />,
            label: t('sheet_chart_edit_type', { defaultValue: 'Type de graphique' }),
            disabled: !single || !a.onEditChartType,
            onClick: () => a.onEditChartType?.(),
          },
        ],
      },
      arrangeGroup(a),
      objectGroup(a),
    ],
  }

  const equationTab: RibbonTab = {
    id: 'equation',
    label: t('sheet_tab_equation', { defaultValue: 'Équation' }),
    contextual: { accent: OBJ_ACCENT, groupLabel: t('sheet_eq_tools', { defaultValue: 'Outils d’équation' }) },
    visible: homogeneous('equation'),
    groups: [
      {
        id: 'eq_edit', label: t('sheet_grp_formula', { defaultValue: 'Formule' }),
        items: [
          {
            id: 'eqedit', kind: 'button', icon: <Sigma size={18} />, size: 'large',
            label: t('sheet_eq_edit', { defaultValue: 'Modifier la formule' }),
            disabled: !single || !a.onEditEquation,
            onClick: () => a.onEditEquation?.(),
          },
        ],
      },
      arrangeGroup(a),
      objectGroup(a),
    ],
  }

  // Mixed natures: no kind tab is truthful any more, so a single generic one
  // replaces them — with strictly the commands every nature understands.
  const mixedTab: RibbonTab = {
    id: 'objects',
    label: t('sheet_tab_objects', { defaultValue: 'Format' }),
    contextual: { accent: OBJ_ACCENT, groupLabel: t('sheet_obj_tools', { defaultValue: 'Outils objets' }) },
    visible: mixed,
    groups: [arrangeGroup(a), objectGroup(a)],
  }

  return [imageTab, shapeTab, chartTab, equationTab, mixedTab]
}
