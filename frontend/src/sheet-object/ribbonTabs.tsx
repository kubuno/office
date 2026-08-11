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

import { Crop, RefreshCw, Table2, PenLine, Sigma } from 'lucide-react'
import type { TFunction } from 'i18next'
import type { RibbonTab } from '../ribbon/types'
import type { ObjKind, ObjOrderOp, ObjStyle } from './types'
import type { AlignMode } from './selection'
// The « Forme » tab and its groups now live in the SHARED shapes package, so the
// spreadsheet, presentations, whiteboard and diagrams configure a shape the same
// way. The spreadsheet reuses those groups for its Image/Chart/Equation tabs too.
import { arrangeGroup, objectGroup, shapeRibbonTab, SHAPE_TAB_ACCENT as OBJ_ACCENT } from '../shapes/shapeRibbonTab'

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

  // The « Forme » tab is the SHARED one (shapes/shapeRibbonTab) — identical in every
  // sub-editor. The spreadsheet just feeds it its object-selection adapter.
  const shapeTab: RibbonTab = shapeRibbonTab({
    t, count: a.count, visible: homogeneous('shape'),
    style: a.style, setStyle: a.setStyle,
    align: a.align, order: a.order, rotate: a.rotate,
    onEditText: a.onEditShapeText,
    openFormat: a.openFormat, openSizeProps: a.openSizeProps, remove: a.remove,
    accent: OBJ_ACCENT,
  })

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
