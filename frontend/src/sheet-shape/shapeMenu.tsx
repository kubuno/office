// The SHAPE context menu, Excel-style, in two blocks:
//
//   • block 1 — `shapeMiniControls()`: the floating mini bar Excel shows next to
//     the menu, [Style ▾][Remplissage ▾][Contour ▾], fed to `MiniToolbar`;
//   • block 2 — `buildShapeMenu()`: the `MenuDropdown` items, whose FIRST item is
//     the Windows-11 row of icon buttons (cut / copy / paste / duplicate / link /
//     delete / format). Those commands are therefore NOT text entries — they live
//     in the icon strip only, each with a Tooltip carrying its label and shortcut.
//
// Both are PURE functions of `ShapeMenuArgs`: they read state and return
// descriptors, never touch state themselves. Everything type-agnostic (the object
// clipboard, the paint order, the alt-text and size dialogs) comes from
// `sheet-object/` through `ObjActions`.
//
// Deliberate omissions vs. Excel (an entry that opens nothing is worse than no
// entry): the "search the menus" field, the "paste options" gallery and "paste
// special" (replaced by the icon row's Paste button), "Group" (the sheet has no
// multi-object selection), "Assign macro…" and "Edit points".

import {
  Scissors, Copy, Clipboard, CopyPlus, Link2, Trash2, Paintbrush,
  Type, ExternalLink, Palette, Accessibility, Settings2,
  BringToFront, SendToBack, FlipHorizontal2, FlipVertical2, Anchor, AlignHorizontalJustifyCenter,
  Minus, PaintBucket, Tag, LinkIcon, Unlink, Maximize2,
} from 'lucide-react'
import type { TFunction } from 'i18next'
import type { MenuItem, MenuTheme } from '@ui'
import { iconRowItems, type MenuIconAction } from '../sheet-object/MenuIconRow'
import type { MiniControl } from '../sheet-object/MiniToolbar'
import type { ObjActions, ObjOrderOp } from '../sheet-object/types'
import type { SheetShape } from '../api'
import { appAlert } from '../macros/FormRuntime'
import { ShapeThumb } from './gallery'
import {
  DEFAULT_SHAPE_STYLE, SHAPE_STYLE_PRESETS,
  type DefaultShapeStyle,
} from './defaults'
import {
  SHAPE_PRESETS, STROKE_PRESETS, LINE_WIDTHS, ANCHOR_MODES,
  presetLabel, sameColour,
} from './palettes'

export interface ShapeMenuArgs {
  /** Generic commands of the selected shape (built by the editor's socle layer). */
  actions: ObjActions
  /** The shape itself — read for state-dependent entries (its look, its text). */
  shape: SheetShape
  t: TFunction

  /**
   * « Modifier le texte » — put the inline `ShapeTextEditor` on the shape. Omitted
   * when not provided (rather than shown doing nothing).
   */
  onEditText?: () => void

  /**
   * « Définir comme style de forme par défaut » — persist the shape's current
   * fill / outline as the user's default (see `useDefaultShapeStyle`).
   */
  saveDefaultStyle?: (style: DefaultShapeStyle) => void | Promise<void>
  /** Overrides the default "save, then confirm" flow. */
  onSaveDefaultStyle?: () => void
  /** « Rétablir le style par défaut » — back to the factory pair (white / black). */
  onResetDefaultStyle?: () => void

  /** Ready-made « Rotation » submenu, when the host wants to offer it. */
  rotateItem?: MenuItem
  /** Ready-made « Premier plan » / « Arrière-plan » submenus. */
  orderItems?: MenuItem[]

  // ── LibreOffice Calc's shape commands (all optional: an entry whose callback is
  // missing is omitted, never shown inert) ───────────────────────────────────────
  /** Patch the shape (fill / outline / width / flip / anchor / name). */
  patch?: (patch: Partial<SheetShape>) => void
  /** « Adapter à la taille de la cellule » (Calc's FitCellSize). */
  onFitCell?: () => void
  /** « Aligner » — position inside the anchoring cell / against the sheet. */
  onAlign?: (how: 'left' | 'centerH' | 'right' | 'top' | 'centerV' | 'bottom') => void
  /** « Renommer l'objet… » (Calc's RenameObject). */
  onRename?: () => void
  /** Hyperlink commands beyond insert/open: copy the address, remove the link. */
  onCopyLink?: () => void
  onRemoveLink?: () => void

  /** Palette of the icon row (dark editors); defaults to 'light'. */
  theme?: MenuTheme
}

/** The shape's look, with the factory defaults filled in for the unset fields. */
function styleOf(shape: SheetShape): DefaultShapeStyle {
  return {
    fill: shape.fill ?? DEFAULT_SHAPE_STYLE.fill,
    border: shape.border ?? DEFAULT_SHAPE_STYLE.border,
    borderWidth: shape.borderWidth ?? DEFAULT_SHAPE_STYLE.borderWidth,
  }
}

/** The icon strip at the top of the card — clipboard family, link, delete, format. */
function shapeIconRow(args: ShapeMenuArgs): MenuIconAction[] {
  const { actions: a, t } = args
  return [
    { id: 'cut', label: t('sheet_obj_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', icon: <Scissors size={16} />, onClick: a.cut },
    { id: 'copy', label: t('sheet_obj_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', icon: <Copy size={16} />, onClick: a.copy },
    { id: 'paste', label: t('sheet_obj_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', icon: <Clipboard size={16} />, disabled: !a.canPaste, onClick: a.paste },
    { id: 'dup', label: t('sheet_obj_duplicate', { defaultValue: 'Dupliquer' }), icon: <CopyPlus size={16} />, disabled: !a.duplicate, onClick: () => a.duplicate?.() },
    { id: 'link', label: t('sheet_obj_link', { defaultValue: 'Lien' }), shortcut: 'Ctrl+K', icon: <Link2 size={16} />, disabled: !a.openLink, onClick: () => a.openLink?.() },
    { id: 'del', label: t('sheet_obj_delete', { defaultValue: 'Supprimer' }), shortcut: 'Suppr', icon: <Trash2 size={16} />, danger: true, onClick: a.remove },
    { id: 'format', label: t('sheet_obj_shape_format', { defaultValue: 'Format de la forme…' }), icon: <Paintbrush size={16} />, onClick: a.openFormat },
  ]
}

/**
 * « Premier plan » / « Arrière-plan ». Paint order is per TYPE (shapes are DOM
 * overlays in their own z-index band, pictures are painted on the grid canvas):
 * these move the shape inside the shape array only, which is exactly what
 * `ObjActions.order` does — a shape can never be sent behind a picture this way.
 */
function orderSubmenus(args: ShapeMenuArgs): MenuItem[] {
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

/** Default flow of « Définir comme style de forme par défaut »: save, then confirm. */
function saveDefaultStyleFlow(args: ShapeMenuArgs): void {
  const { t, shape, saveDefaultStyle } = args
  if (!saveDefaultStyle) return
  void (async () => {
    await saveDefaultStyle(styleOf(shape))
    await appAlert(t('sheet_obj_default_style_saved', { defaultValue: 'Style de forme par défaut enregistré.' }))
  })()
}


// ── Formatting submenus, presented the way the documents editor does it: NAMED
// colours with a check mark on the one in use, instead of an anonymous grid ─────

function fillSubmenu(args: ShapeMenuArgs): MenuItem | null {
  const { t, shape, patch } = args
  if (!patch) return null
  return {
    type: 'submenu',
    label: t('sheet_obj_fill', { defaultValue: 'Remplissage' }),
    icon: <PaintBucket size={15} />,
    items: [
      ...SHAPE_PRESETS.map(p => ({
        type: 'action' as const,
        label: presetLabel(t, p),
        checked: sameColour(shape.fill, p.fill),
        // Applying a fill also carries its matching outline, like the documents
        // editor: the pair is what makes the shape look designed rather than random.
        onClick: () => patch({ fill: p.fill, border: p.stroke }),
      })),
      { type: 'separator' as const },
      {
        type: 'action' as const,
        label: t('sheet_obj_no_fill', { defaultValue: 'Aucun remplissage' }),
        checked: shape.fill === 'none',
        onClick: () => patch({ fill: 'none' }),
      },
    ],
  }
}

function lineSubmenu(args: ShapeMenuArgs): MenuItem | null {
  const { t, shape, patch } = args
  if (!patch) return null
  return {
    type: 'submenu',
    label: t('sheet_shape_line', { defaultValue: 'Ligne' }),
    icon: <Minus size={15} />,
    items: [
      ...STROKE_PRESETS.map(p => ({
        type: 'action' as const,
        label: presetLabel(t, p),
        checked: sameColour(shape.border, p.stroke),
        onClick: () => patch({ border: p.stroke }),
      })),
      { type: 'separator' as const },
      ...LINE_WIDTHS.map(w => ({
        type: 'action' as const,
        label: t(w.key, { defaultValue: w.fallback }),
        checked: (shape.borderWidth ?? 1) === w.w,
        onClick: () => patch({ borderWidth: w.w }),
      })),
      { type: 'separator' as const },
      {
        type: 'action' as const,
        label: t('sheet_obj_no_outline', { defaultValue: 'Aucun contour' }),
        checked: shape.border === 'none',
        onClick: () => patch({ border: 'none' }),
      },
    ],
  }
}

/** « Refléter » — Calc's FlipMenu, mirroring the shape on either axis. */
function flipSubmenu(args: ShapeMenuArgs): MenuItem | null {
  const { t, shape, patch } = args
  if (!patch) return null
  return {
    type: 'submenu',
    label: t('sheet_shape_flip', { defaultValue: 'Refléter' }),
    icon: <FlipHorizontal2 size={15} />,
    items: [
      {
        type: 'action', label: t('sheet_shape_flip_v', { defaultValue: 'Verticalement' }),
        icon: <FlipVertical2 size={15} />, checked: !!shape.flipV,
        onClick: () => patch({ flipV: !shape.flipV }),
      },
      {
        type: 'action', label: t('sheet_shape_flip_h', { defaultValue: 'Horizontalement' }),
        icon: <FlipHorizontal2 size={15} />, checked: !!shape.flipH,
        onClick: () => patch({ flipH: !shape.flipH }),
      },
    ],
  }
}

/** « Ancrage » — how the shape follows the cells (Calc's AnchorMenu). */
function anchorSubmenu(args: ShapeMenuArgs): MenuItem | null {
  const { t, shape, patch } = args
  if (!patch) return null
  const cur = shape.anchor ?? 'cell'
  return {
    type: 'submenu',
    label: t('sheet_shape_anchor', { defaultValue: 'Ancrage' }),
    icon: <Anchor size={15} />,
    items: ANCHOR_MODES.map(m => ({
      type: 'action' as const,
      label: t(m.key, { defaultValue: m.fallback }),
      checked: cur === m.mode,
      onClick: () => patch({ anchor: m.mode }),
    })),
  }
}

/** « Aligner » — Calc's ObjectAlign, both axes in one submenu. */
function alignSubmenu(args: ShapeMenuArgs): MenuItem | null {
  const { t, onAlign } = args
  if (!onAlign) return null
  return {
    type: 'submenu',
    label: t('sheet_shape_align', { defaultValue: 'Aligner' }),
    icon: <AlignHorizontalJustifyCenter size={15} />,
    items: [
      { type: 'action', label: t('sheet_shape_align_left', { defaultValue: 'À gauche' }), onClick: () => onAlign('left') },
      { type: 'action', label: t('sheet_shape_align_centerh', { defaultValue: 'Centré' }), onClick: () => onAlign('centerH') },
      { type: 'action', label: t('sheet_shape_align_right', { defaultValue: 'À droite' }), onClick: () => onAlign('right') },
      { type: 'separator' },
      { type: 'action', label: t('sheet_shape_align_top', { defaultValue: 'En haut' }), onClick: () => onAlign('top') },
      { type: 'action', label: t('sheet_shape_align_centerv', { defaultValue: 'Au milieu' }), onClick: () => onAlign('centerV') },
      { type: 'action', label: t('sheet_shape_align_bottom', { defaultValue: 'En bas' }), onClick: () => onAlign('bottom') },
    ],
  }
}

/** Hyperlink group — Calc offers the whole set, not just "insert". */
function hyperlinkItems(args: ShapeMenuArgs): MenuItem[] {
  const { actions: a, t, onCopyLink, onRemoveLink } = args
  const items: MenuItem[] = [
    { type: 'action', label: t('sheet_obj_link', { defaultValue: 'Lien' }), icon: <Link2 size={15} />, disabled: !a.openLink, onClick: () => a.openLink?.() },
    { type: 'action', label: t('sheet_obj_open_link', { defaultValue: 'Ouvrir le lien' }), icon: <ExternalLink size={15} />, disabled: !a.hasLink || !a.followLink, onClick: () => a.followLink?.() },
  ]
  if (onCopyLink) items.push({ type: 'action', label: t('sheet_shape_copy_link', { defaultValue: 'Copier l’adresse du lien' }), icon: <LinkIcon size={15} />, disabled: !a.hasLink, onClick: onCopyLink })
  if (onRemoveLink) items.push({ type: 'action', label: t('sheet_shape_remove_link', { defaultValue: 'Supprimer le lien' }), icon: <Unlink size={15} />, disabled: !a.hasLink, onClick: onRemoveLink })
  return items
}

/**
 * Block 2 — the shape's context menu.
 *
 * Order (Excel's, minus the omissions listed at the top of this file):
 * icon row ┊ Modifier le texte ┊ Premier plan · Arrière-plan ┊ Lien… · Ouvrir le
 * lien ┊ Afficher le texte de remplacement… · Définir comme style de forme par
 * défaut · Taille et propriétés… · Format de la forme… ┊ Supprimer la forme.
 */
export function buildShapeMenu(args: ShapeMenuArgs): MenuItem[] {
  const { actions: a, t } = args
  const canSaveDefault = !!(args.onSaveDefaultStyle || args.saveDefaultStyle)
  const keep = (items: (MenuItem | null)[]): MenuItem[] => items.filter((i): i is MenuItem => i != null)

  return [
    ...iconRowItems(shapeIconRow(args), args.theme ?? 'light'),

    // ── Edit ──
    ...(args.onEditText
      ? [{ type: 'action' as const, label: t('sheet_obj_shape_edit_text', { defaultValue: 'Modifier le texte' }), icon: <Type size={15} />, onClick: args.onEditText }]
      : []),
    ...(args.rotateItem ? [args.rotateItem] : []),

    { type: 'separator' },
    // ── Format (Calc's FitCellSize · TransformDialog · FormatLine · FormatArea) ──
    ...keep([
      args.onFitCell
        ? { type: 'action', label: t('sheet_shape_fit_cell', { defaultValue: 'Adapter à la taille de la cellule' }), icon: <Maximize2 size={15} />, onClick: args.onFitCell }
        : null,
      fillSubmenu(args),
      lineSubmenu(args),
    ]),
    { type: 'action', label: t('sheet_shape_position_size', { defaultValue: 'Position et taille…' }), shortcut: 'F4', icon: <Settings2 size={15} />, onClick: a.openSizeProps },
    { type: 'action', label: t('sheet_obj_shape_format', { defaultValue: 'Format de la forme…' }), icon: <Paintbrush size={15} />, onClick: a.openFormat },

    { type: 'separator' },
    // ── Place (Calc's AnchorMenu · ObjectAlign · ArrangeMenu · FlipMenu) ──
    ...keep([anchorSubmenu(args), alignSubmenu(args)]),
    ...orderSubmenus(args),
    ...keep([flipSubmenu(args)]),

    { type: 'separator' },
    // ── Identify (Calc's RenameObject · ObjectTitleDescription) ──
    ...keep([
      args.onRename
        ? { type: 'action', label: t('sheet_shape_rename', { defaultValue: 'Renommer l’objet…' }), icon: <Tag size={15} />, onClick: args.onRename }
        : null,
    ]),
    { type: 'action', label: t('sheet_obj_alt_text', { defaultValue: 'Afficher le texte de remplacement…' }), icon: <Accessibility size={15} />, onClick: a.openAltText },
    ...(canSaveDefault
      ? [{
          type: 'action' as const,
          label: t('sheet_obj_shape_default_style', { defaultValue: 'Définir comme style de forme par défaut' }),
          icon: <Palette size={15} />,
          onClick: args.onSaveDefaultStyle ?? (() => saveDefaultStyleFlow(args)),
        }]
      : []),
    ...(args.onResetDefaultStyle
      ? [{
          type: 'action' as const,
          label: t('sheet_shape_reset_default_style', { defaultValue: 'Rétablir le style de forme par défaut' }),
          icon: <Palette size={15} />,
          onClick: args.onResetDefaultStyle,
        }]
      : []),

    { type: 'separator' },
    ...hyperlinkItems(args),

    { type: 'separator' },
    { type: 'action', label: t('sheet_shape_delete', { defaultValue: 'Supprimer la forme' }), icon: <Trash2 size={15} />, danger: true, onClick: a.remove },
  ]
}

/**
 * Block 1 — the mini bar Excel floats next to the shape menu:
 * [Style ▾][Remplissage ▾][Contour ▾].
 *
 * « Style » is a small gallery of fill/outline pairs drawn with THIS shape's own
 * geometry, so the preview is the result; the two colour controls write the shape's
 * `fill` / `border` / `borderWidth` straight through `ObjActions.setStyle`.
 */
export function shapeMiniControls(args: ShapeMenuArgs): MiniControl[] {
  const { actions: a, t, shape } = args
  const current = styleOf(shape)

  return [
    {
      id: 'style', kind: 'menu',
      label: t('sheet_obj_style', { defaultValue: 'Style' }),
      render: close => (
        <div className="grid grid-cols-4 gap-1" style={{ width: 176 }}>
          {SHAPE_STYLE_PRESETS.map(p => {
            const active = p.fill === current.fill && p.border === current.border
            return (
              <button
                key={p.id}
                type="button"
                title={t(p.labelKey, { defaultValue: p.labelDefault })}
                aria-label={t(p.labelKey, { defaultValue: p.labelDefault })}
                onClick={() => { a.setStyle({ fill: p.fill, border: p.border, borderWidth: p.borderWidth }); close() }}
                className={`p-1 rounded border hover:border-primary hover:bg-primary-light ${active ? 'border-primary' : 'border-transparent'}`}
              >
                <ShapeThumb kind={shape.kind} width={32} height={24} style={p} />
              </button>
            )
          })}
        </div>
      ),
    },
    {
      id: 'fill', kind: 'color',
      label: t('sheet_obj_fill', { defaultValue: 'Remplissage' }),
      color: current.fill,
      onPick: hex => a.setStyle({ fill: hex }),
      noneLabel: t('sheet_obj_no_fill', { defaultValue: 'Aucun remplissage' }),
      onNone: () => a.setStyle({ fill: 'none' }),
    },
    {
      id: 'outline', kind: 'color',
      label: t('sheet_obj_outline', { defaultValue: 'Contour' }),
      color: current.border,
      onPick: hex => a.setStyle({ border: hex, borderWidth: current.borderWidth || 1 }),
      noneLabel: t('sheet_obj_no_outline', { defaultValue: 'Aucun contour' }),
      onNone: () => a.setStyle({ border: 'none' }),
    },
  ]
}
