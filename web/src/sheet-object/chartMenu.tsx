// The CHART context menu, Excel-style, in two blocks:
//
//   • block 1 — `chartMiniControls()`: the floating mini bar Excel shows next to
//     the menu, [Remplissage ▾][Contour ▾], fed to `MiniToolbar`;
//   • block 2 — `buildChartMenu()`: the `MenuDropdown` items, whose FIRST item is
//     the Windows-11 row of icon buttons (cut / copy / paste / duplicate /
//     delete / format). Those five clipboard-family commands are therefore NOT
//     text entries — they live in the icon strip only.
//
// Both are PURE functions of `ChartMenuArgs`: they read state and return
// descriptors, never touch state themselves. Everything type-agnostic (the
// clipboard, ordering, the alt-text and size dialogs) comes from `sheet-object/`.
//
// Deliberate omissions vs. Excel (an entry that opens nothing is worse than no
// entry): the "search the menus" field, the "paste options" gallery and "paste
// special" (replaced by the icon row's Paste button), "Group" (the sheet has no
// multi-object selection), "Assign macro…", "3-D rotation…" (replaced by our 2-D
// Rotation submenu — free angles already come from the rotation knob) and
// "PivotChart options…".

import {
  Scissors, Copy, Clipboard, CopyPlus, Trash2, Paintbrush, Type,
  BarChart3, Table2, Save, LayoutTemplate, Move, RotateCw,
  BringToFront, SendToBack, RefreshCw, Accessibility, Settings2,
} from 'lucide-react'
import type { TFunction } from 'i18next'
import type { MenuItem, MenuTheme } from '@ui'
import { iconRowItems, type MenuIconAction } from './MenuIconRow'
import type { MiniControl } from './MiniToolbar'
import type { ObjActions, ObjOrderOp } from './types'
import type { SheetChart } from '../api'
import { appAlert, appPrompt } from '../macros/FormRuntime'
import { chartTemplatePatch, type ChartTemplate } from './chartTemplates'

/** Chart-area defaults the sheet paints when the fields are unset. */
const DEFAULT_CHART_FILL = '#ffffff'
const DEFAULT_CHART_BORDER = '#e2e4e6'

export interface ChartMenuArgs {
  /** Generic commands of the selected chart (built by the editor's socle layer). */
  actions: ObjActions
  /** The chart itself — read for state-dependent entries (style reset, templates). */
  chart: SheetChart
  t: TFunction
  /**
   * Persist a partial update of THIS chart. The single seam the style reset, the
   * template application and the mini bar write through.
   */
  patchChart: (patch: Partial<SheetChart>) => void

  // ── Commands that need UI the menu does not own (omitted when not provided) ──
  /** « Modifier le type de graphique… » — open the wizard on its type step. */
  onChangeType?: () => void
  /** « Sélectionner des données… » — open the wizard on its data step. */
  onSelectData?: () => void
  /** « Police… » — open the font chooser for every chart text. */
  onFont?: () => void
  /** « Déplacer le graphique… » — reposition / move to another sheet. */
  onMoveChart?: () => void

  // ── Templates (see chartTemplates.ts / useChartTemplates) ──
  /** Templates already saved by the user; empty ⇒ "Appliquer un modèle" disabled. */
  templates?: ChartTemplate[]
  /** Store the current look under `name`. Enables « Enregistrer comme modèle… ». */
  saveTemplate?: (name: string, chart: SheetChart) => void | Promise<void>
  /** Overrides the default "ask for a name, then save" flow. */
  onSaveTemplate?: () => void
  /** Overrides the default "patch the chart with the template's style". */
  onApplyTemplate?: (tpl: ChartTemplate) => void

  // ── Optional reuse of the editor's shared submenus (identical for the 3 types) ──
  /** Quarter-turn / reset; used to build the Rotation submenu when no `rotateItem`. */
  onRotate?: (deg: number, reset?: boolean) => void
  /** Ready-made « Rotation » submenu. */
  rotateItem?: MenuItem
  /** Ready-made « Premier plan » / « Arrière-plan » submenus. */
  orderItems?: MenuItem[]

  /** Palette of the icon row (dark editors); defaults to 'light'. */
  theme?: MenuTheme
}

/** The icon strip at the top of the card — clipboard family + format. */
function chartIconRow(args: ChartMenuArgs): MenuIconAction[] {
  const { actions: a, t } = args
  return [
    { id: 'cut', label: t('sheet_obj_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', icon: <Scissors size={16} />, onClick: a.cut },
    { id: 'copy', label: t('sheet_obj_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', icon: <Copy size={16} />, onClick: a.copy },
    { id: 'paste', label: t('sheet_obj_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', icon: <Clipboard size={16} />, disabled: !a.canPaste, onClick: a.paste },
    { id: 'dup', label: t('sheet_obj_duplicate', { defaultValue: 'Dupliquer' }), icon: <CopyPlus size={16} />, disabled: !a.duplicate, onClick: () => a.duplicate?.() },
    { id: 'del', label: t('sheet_obj_delete', { defaultValue: 'Supprimer' }), shortcut: 'Suppr', icon: <Trash2 size={16} />, danger: true, onClick: a.remove },
    { id: 'format', label: t('sheet_obj_chart_format', { defaultValue: 'Format de la zone de graphique…' }), icon: <Paintbrush size={16} />, onClick: a.openFormat },
  ]
}

/**
 * « Rétablir le style d'origine » — drop every manual formatting layer and let
 * the chart fall back to the sheet's defaults: chart-area fill/outline, font,
 * explicit palette, and the per-series colour overrides. Data is untouched
 * (series keep their name/range, only their `color` goes).
 */
function resetChartStyle(args: ChartMenuArgs): void {
  const { chart } = args
  const patch: Partial<SheetChart> = {
    fill: undefined, border: undefined, borderWidth: undefined,
    font: undefined, colors: undefined,
  }
  if (chart.series) {
    patch.series = chart.series.map(({ color: _color, ...rest }) => rest)
  }
  args.patchChart(patch)
}

/** Default flow of « Enregistrer comme modèle… »: ask a name, then persist. */
function saveTemplateFlow(args: ChartMenuArgs): void {
  const { t, chart, templates = [], saveTemplate } = args
  if (!saveTemplate) return
  const suggestion = chart.title?.trim()
    || t('sheet_obj_chart_template_default', { defaultValue: 'Modèle {{n}}', n: templates.length + 1 })
  void (async () => {
    const name = await appPrompt(t('sheet_obj_chart_template_name', { defaultValue: 'Nom du modèle' }), suggestion)
    const trimmed = name?.trim()
    if (!trimmed) return
    await saveTemplate(trimmed, chart)
    await appAlert(t('sheet_obj_template_saved', { defaultValue: 'Modèle enregistré.' }))
  })()
}

/** « Appliquer un modèle » — disabled while the user has saved none. */
function applyTemplateItem(args: ChartMenuArgs): MenuItem {
  const { t, templates = [] } = args
  const label = t('sheet_obj_chart_apply_template', { defaultValue: 'Appliquer un modèle' })
  if (!templates.length) {
    return { type: 'submenu', label, icon: <LayoutTemplate size={15} />, disabled: true, items: [] }
  }
  // Most recent first — the list is stored oldest-first.
  const items: MenuItem[] = [...templates].reverse().map(tpl => ({
    type: 'action' as const,
    label: tpl.name,
    onClick: () => {
      if (args.onApplyTemplate) { args.onApplyTemplate(tpl); return }
      args.patchChart(chartTemplatePatch(tpl))
    },
  }))
  return { type: 'submenu', label, icon: <LayoutTemplate size={15} />, items }
}

/** « Rotation » — our 2-D stand-in for Excel's greyed-out "Rotation 3D…". */
function rotationItem(args: ChartMenuArgs): MenuItem | null {
  if (args.rotateItem) return args.rotateItem
  const rotate = args.onRotate
  if (!rotate) return null
  const { t } = args
  return {
    type: 'submenu', label: t('sheet_obj_rotate_menu', { defaultValue: 'Rotation' }), icon: <RotateCw size={15} />,
    items: [
      { type: 'action', label: t('sheet_obj_rotate_right', { defaultValue: 'Pivoter à droite 90°' }), onClick: () => rotate(90) },
      { type: 'action', label: t('sheet_obj_rotate_left', { defaultValue: 'Pivoter à gauche 90°' }), onClick: () => rotate(-90) },
      { type: 'action', label: t('sheet_obj_rotate_reset', { defaultValue: 'Réinitialiser la rotation' }), onClick: () => rotate(0, true) },
    ],
  }
}

/**
 * « Premier plan » / « Arrière-plan ». Paint order is per TYPE (charts are DOM
 * overlays in their own z-index band, pictures are painted on the grid canvas):
 * these move the chart inside the chart array only, which is exactly what
 * `ObjActions.order` does.
 */
function orderSubmenus(args: ChartMenuArgs): MenuItem[] {
  if (args.orderItems) return args.orderItems
  const { actions: a, t } = args
  const entry = (key: string, def: string, op: ObjOrderOp): MenuItem =>
    ({ type: 'action', label: t(key, { defaultValue: def }), disabled: !a.canOrder(op), onClick: () => a.order(op) })
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

/**
 * Block 2 — the chart's context menu.
 *
 * Order (Excel's, minus the omissions listed at the top of this file):
 * icon row · Rétablir le style d'origine · Police… ┊ Modifier le type de
 * graphique… · Sélectionner des données… · Enregistrer comme modèle… ·
 * Appliquer un modèle ┊ Déplacer le graphique… · Rotation · Premier plan ·
 * Arrière-plan ┊ Afficher le texte de remplacement… · Taille et propriétés… ·
 * Format de la zone de graphique… ┊ Supprimer le graphique.
 */
export function buildChartMenu(args: ChartMenuArgs): MenuItem[] {
  const { actions: a, t } = args
  const rotate = rotationItem(args)

  return [
    ...iconRowItems(chartIconRow(args), args.theme ?? 'light'),

    { type: 'action', label: t('sheet_obj_chart_reset_style', { defaultValue: 'Rétablir le style d’origine' }), icon: <RefreshCw size={15} />, onClick: () => resetChartStyle(args) },
    ...(args.onFont
      ? [{ type: 'action' as const, label: t('sheet_obj_chart_font', { defaultValue: 'Police…' }), icon: <Type size={15} />, onClick: args.onFont }]
      : []),

    { type: 'separator' },
    ...(args.onChangeType
      ? [{ type: 'action' as const, label: t('sheet_obj_chart_change_type', { defaultValue: 'Modifier le type de graphique…' }), icon: <BarChart3 size={15} />, onClick: args.onChangeType }]
      : []),
    ...(args.onSelectData
      ? [{ type: 'action' as const, label: t('sheet_obj_chart_select_data', { defaultValue: 'Sélectionner des données…' }), icon: <Table2 size={15} />, onClick: args.onSelectData }]
      : []),
    ...(args.onSaveTemplate || args.saveTemplate
      ? [{
          type: 'action' as const,
          label: t('sheet_obj_chart_save_template', { defaultValue: 'Enregistrer comme modèle…' }),
          icon: <Save size={15} />,
          onClick: args.onSaveTemplate ?? (() => saveTemplateFlow(args)),
        }]
      : []),
    applyTemplateItem(args),

    { type: 'separator' },
    ...(args.onMoveChart
      ? [{ type: 'action' as const, label: t('sheet_obj_chart_move', { defaultValue: 'Déplacer le graphique…' }), icon: <Move size={15} />, onClick: args.onMoveChart }]
      : []),
    ...(rotate ? [rotate] : []),
    ...orderSubmenus(args),

    { type: 'separator' },
    { type: 'action', label: t('sheet_obj_alt_text', { defaultValue: 'Afficher le texte de remplacement…' }), icon: <Accessibility size={15} />, onClick: a.openAltText },
    { type: 'action', label: t('sheet_obj_size_props', { defaultValue: 'Taille et propriétés…' }), icon: <Settings2 size={15} />, onClick: a.openSizeProps },
    { type: 'action', label: t('sheet_obj_chart_format', { defaultValue: 'Format de la zone de graphique…' }), icon: <Paintbrush size={15} />, onClick: a.openFormat },

    { type: 'separator' },
    { type: 'action', label: t('sheet_chart_delete', { defaultValue: 'Supprimer le graphique' }), icon: <Trash2 size={15} />, danger: true, shortcut: 'Suppr', onClick: a.remove },
  ]
}

/**
 * Block 1 — the mini bar Excel floats next to the chart menu:
 * [Remplissage ▾][Contour ▾], writing the chart-area `fill` / `border` /
 * `borderWidth` fields the DOM overlay honours.
 */
export function chartMiniControls(args: ChartMenuArgs): MiniControl[] {
  const { actions: a, t } = args
  return [
    {
      id: 'fill', kind: 'color',
      label: t('sheet_obj_fill', { defaultValue: 'Remplissage' }),
      color: a.style.fill ?? DEFAULT_CHART_FILL,
      onPick: hex => a.setStyle({ fill: hex }),
      noneLabel: t('sheet_obj_no_fill', { defaultValue: 'Aucun remplissage' }),
      onNone: () => a.setStyle({ fill: 'none' }),
    },
    {
      id: 'outline', kind: 'color',
      label: t('sheet_obj_outline', { defaultValue: 'Contour' }),
      color: a.style.border ?? DEFAULT_CHART_BORDER,
      onPick: hex => a.setStyle({ border: hex, borderWidth: a.style.borderWidth ?? 1 }),
      noneLabel: t('sheet_obj_no_outline', { defaultValue: 'Aucun contour' }),
      onNone: () => a.setStyle({ border: 'none' }),
    },
  ]
}
