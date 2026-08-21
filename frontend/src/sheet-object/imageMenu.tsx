// The PICTURE context menu, Excel-style, in two blocks:
//
//   • block 1 — `imageMiniControls()`: the floating mini bar Excel shows next to the
//     menu, [Style ▾][Rogner], fed to `MiniToolbar`. Its Style gallery is a set of
//     frame presets mapped onto the picture's `border` / `borderWidth` / `shadow`
//     fields (see IMAGE_FRAME_PRESETS), which `imageFrame.ts` paints;
//   • block 2 — `buildImageMenu()`: the `MenuDropdown` items, whose FIRST item is the
//     Windows-11 row of icon buttons (cut / copy / paste / duplicate / link / delete /
//     format). Those clipboard-family commands are therefore NOT text entries — they
//     live in the icon strip only, each with a @ui Tooltip carrying label + shortcut.
//
// Both are PURE functions of `ImageMenuArgs`: they read state and return descriptors,
// never touch state themselves. Everything type-agnostic (the object clipboard,
// ordering, the alt-text / size / format dialogs) comes from `sheet-object/`.
//
// Deliberate omissions vs. Excel (an entry that opens nothing is worse than no
// entry): the "search the menus" field, the "paste options" gallery and "paste
// special" (replaced by the icon row's Paste button) and "Assign macro…". A
// multi-object selection switches to the GROUP menu (sheet-object/groupMenu.tsx),
// so nothing here has to handle it.
//
// Adaptations: Excel's « Changer d'image » sources become « Depuis l'appareil… » /
// « Depuis Drive… » (both the core image picker, filtered by tab) and « Depuis le
// presse-papiers » (navigator.clipboard, disabled where the API is unavailable — an
// insecure HTTP origin has none). Excel has no picture Rotation submenu (rotation is
// on the ribbon); ours mirrors the other two object types.

import type { ReactNode } from 'react'
import {
  Scissors, Copy, Clipboard, CopyPlus, Link2, Trash2, Paintbrush,
  RefreshCw, Replace, Crop, RotateCw, BringToFront, SendToBack,
  Accessibility, Settings2, MonitorSmartphone, HardDrive, ClipboardPaste, Frame,
} from 'lucide-react'
import type { TFunction } from 'i18next'
import { openImagePicker } from '@kubuno/sdk'
import type { MenuItem, MenuTheme } from '@ui'
import { iconRowItems, type MenuIconAction } from './MenuIconRow'
import type { MiniControl } from './MiniToolbar'
import type { ObjActions, ObjOrderOp } from './types'
import type { SheetImage } from '../api'
import { appAlert } from '../macros/FormRuntime'

/** Where a replacement picture comes from. */
export type ImagePictureSource = 'device' | 'drive' | 'clipboard'

export interface ImageMenuArgs {
  /** Generic commands of the selected picture (built by the editor's socle layer). */
  actions: ObjActions
  /** The picture itself — read for state-dependent entries (active frame preset). */
  image: SheetImage
  t: TFunction
  /**
   * Persist a partial update of THIS picture. The single seam the frame presets and
   * the "change picture" flow write through (`shadow` is outside `ObjStyle`, so the
   * mini bar cannot go through `actions.setStyle` for it).
   */
  patchImage: (patch: Partial<SheetImage>) => void

  // ── Commands that need editor state the menu does not own ────────────────────
  /** « Rogner » — enter crop mode on this picture. Entry omitted when absent. */
  onCrop?: () => void
  /** True while this picture is already being cropped (highlights the mini bar button). */
  cropping?: boolean
  /** « Rétablir la taille de l'image » — drop the manual box back to the anchor. */
  onResetSize?: () => void
  /**
   * Overrides the default « Changer d'image » flow (pick a source, then
   * `patchImage({ src, crop… })`). Rarely needed; provide it to upload instead of
   * embedding, for instance.
   */
  onChangePicture?: (source: ImagePictureSource) => void
  /**
   * Show a one-button notice ("the clipboard holds no image"). Provide it whenever the
   * host has its own dialog: the fallback, `macros/FormRuntime.appAlert`, needs
   * `DialogHost` — which only `MacrosMenu` mounts, i.e. only while the ribbon's
   * Affichage tab is rendered — so from any other tab the message would be swallowed
   * and the command would look like it did nothing.
   */
  notify?: (message: string) => void

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

/* ── Frame presets (block 1's Style gallery) ──────────────────────────────────── */

export type ImageFramePresetId = 'none' | 'thin' | 'thick' | 'shadow'

export interface ImageFramePreset {
  id: ImageFramePresetId
  labelKey: string
  labelDefault: string
  /** Written straight through `patchImage`. */
  patch: Partial<SheetImage>
}

/** Neutral frame colour — the grey Excel's own picture styles use. */
const FRAME_COLOUR = '#3c4043'
const THIN_WIDTH = 1
const THICK_WIDTH = 4
/** A frame at least this thick reads as the "thick" preset. */
const THICK_THRESHOLD = 3

/**
 * The four looks the mini bar offers, mapped 1:1 onto model fields so they survive a
 * save and an xlsx round-trip (`a:ln` for the outline, an outer shadow effect for the
 * shadow). Anything finer — a custom colour, a custom width — lives in
 * « Format de l'objet… », which is why this stays a short, opinionated list.
 */
export const IMAGE_FRAME_PRESETS: readonly ImageFramePreset[] = [
  { id: 'none', labelKey: 'sheet_obj_img_frame_none', labelDefault: 'Aucun cadre', patch: { border: 'none', borderWidth: undefined, shadow: false } },
  { id: 'thin', labelKey: 'sheet_obj_img_frame_thin', labelDefault: 'Cadre fin', patch: { border: FRAME_COLOUR, borderWidth: THIN_WIDTH, shadow: false } },
  { id: 'thick', labelKey: 'sheet_obj_img_frame_thick', labelDefault: 'Cadre épais', patch: { border: FRAME_COLOUR, borderWidth: THICK_WIDTH, shadow: false } },
  { id: 'shadow', labelKey: 'sheet_obj_img_frame_shadow', labelDefault: 'Ombre portée', patch: { border: 'none', borderWidth: undefined, shadow: true } },
]

/**
 * Which preset the picture currently matches, or null when its formatting is a
 * combination none of them describes (a framed picture WITH a shadow, a custom
 * colour set from the format pane…). Used to tick the gallery, never to rewrite.
 */
export function activeImageFramePreset(image: SheetImage): ImageFramePresetId | null {
  const framed = !!image.border && image.border !== 'none'
  if (image.shadow) return framed ? null : 'shadow'
  if (!framed) return 'none'
  if (image.border !== FRAME_COLOUR) return null
  return (image.borderWidth ?? THIN_WIDTH) >= THICK_THRESHOLD ? 'thick' : 'thin'
}

/** Small live preview of one preset: a picture stand-in wearing the frame. */
function framePreview(id: ImageFramePresetId): ReactNode {
  const border = id === 'thin' ? `${THIN_WIDTH}px solid ${FRAME_COLOUR}`
    : id === 'thick' ? `${THICK_WIDTH}px solid ${FRAME_COLOUR}`
    : '1px solid transparent'
  return (
    <span
      aria-hidden
      className="inline-block shrink-0"
      style={{
        width: 34, height: 22, border, borderRadius: 2,
        background: 'linear-gradient(135deg, #cfd8e3 0%, #eceff3 100%)',
        boxShadow: id === 'shadow' ? '2px 3px 5px rgba(0,0,0,0.35)' : undefined,
      }}
    />
  )
}

/* ── « Changer d'image » sources ──────────────────────────────────────────────── */

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(new Error('clipboard image read failed'))
    reader.readAsDataURL(blob)
  })
}

/**
 * True when the browser can hand us the clipboard's binary items. `clipboard.read` is
 * missing outside a secure context (plain HTTP on anything but localhost — the very
 * setup `uid()` exists for), so the menu entry is disabled rather than throwing.
 */
export function clipboardImageSupported(): boolean {
  return typeof navigator !== 'undefined'
    && typeof navigator.clipboard?.read === 'function'
    && (typeof window === 'undefined' || window.isSecureContext)
}

/**
 * The clipboard's first image, as a data URL, or null when it holds none. Throws only
 * if the read itself is refused (permission denied) — callers report that as "no
 * image found", which is what the user perceives.
 */
export async function readClipboardImageSrc(): Promise<string | null> {
  if (!clipboardImageSupported()) return null
  const items = await navigator.clipboard.read()
  for (const item of items) {
    const type = item.types.find(ty => ty.startsWith('image/'))
    if (!type) continue
    return await blobToDataUrl(await item.getType(type))
  }
  return null
}

/**
 * Opens the core image picker on the tabs matching `source` and resolves with a `src`
 * usable as-is. Filtering happens through the picker's `exclude` option (source ids:
 * 'url', 'upload', 'webcam' from the core, 'drive' and 'photos' contributed by their
 * modules), so the module never builds a file input of its own:
 *   • 'device' → Importer / Webcam;
 *   • 'drive'  → Drive / Photos / À partir d'une URL.
 * A picked URL is kept as a reference, a picked file is embedded as a data URL — the
 * same rule as `imagePicker.pickImageSrc`, which cannot forward `exclude`.
 */
async function pickPictureSrc(source: 'device' | 'drive', title: string): Promise<string | null> {
  const exclude = source === 'device' ? ['drive', 'photos', 'url'] : ['upload', 'webcam']
  const picked = await openImagePicker({ title, exclude })
  if (!picked) return null
  return picked.kind === 'url' ? picked.url : await blobToDataUrl(picked.file)
}

/**
 * Default « Changer d'image » flow: get a new `src`, keep the picture's box, frame and
 * rotation, and drop the crop insets — they are fractions of the OLD bitmap and would
 * cut the new one at meaningless places.
 */
function changePicture(args: ImageMenuArgs, source: ImagePictureSource): void {
  if (args.onChangePicture) { args.onChangePicture(source); return }
  const { t } = args
  void (async () => {
    let src: string | null = null
    try {
      src = source === 'clipboard'
        ? await readClipboardImageSrc()
        : await pickPictureSrc(source, t('sheet_obj_img_change', { defaultValue: 'Changer d’image' }))
    } catch {
      src = null
    }
    if (!src) {
      if (source === 'clipboard') {
        const msg = t('sheet_obj_img_no_clipboard', { defaultValue: 'Le presse-papiers ne contient aucune image.' })
        if (args.notify) args.notify(msg)
        else await appAlert(msg)
      }
      return
    }
    args.patchImage({ src, cropL: undefined, cropT: undefined, cropR: undefined, cropB: undefined })
  })()
}

/** « Changer d'image » submenu — Excel's five sources, reduced to the three we have. */
function changePictureItem(args: ImageMenuArgs): MenuItem {
  const { t } = args
  const clip = clipboardImageSupported()
  return {
    type: 'submenu',
    label: t('sheet_obj_img_change', { defaultValue: 'Changer d’image' }),
    icon: <Replace size={15} />,
    items: [
      { type: 'action', label: t('sheet_obj_img_from_device', { defaultValue: 'Depuis l’appareil…' }), icon: <MonitorSmartphone size={15} />, onClick: () => changePicture(args, 'device') },
      { type: 'action', label: t('sheet_obj_img_from_drive', { defaultValue: 'Depuis Drive…' }), icon: <HardDrive size={15} />, onClick: () => changePicture(args, 'drive') },
      { type: 'action', label: t('sheet_obj_img_from_clipboard', { defaultValue: 'Depuis le presse-papiers' }), icon: <ClipboardPaste size={15} />, disabled: !clip, onClick: () => changePicture(args, 'clipboard') },
    ],
  }
}

/* ── Shared blocks (identical wording for the three object types) ─────────────── */

/** The icon strip at the top of the card — clipboard family, link, delete, format. */
function imageIconRow(args: ImageMenuArgs): MenuIconAction[] {
  const { actions: a, t } = args
  return [
    { id: 'cut', label: t('sheet_obj_cut', { defaultValue: 'Couper' }), shortcut: 'Ctrl+X', icon: <Scissors size={16} />, onClick: a.cut },
    { id: 'copy', label: t('sheet_obj_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', icon: <Copy size={16} />, onClick: a.copy },
    { id: 'paste', label: t('sheet_obj_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', icon: <Clipboard size={16} />, disabled: !a.canPaste, onClick: a.paste },
    { id: 'dup', label: t('sheet_obj_duplicate', { defaultValue: 'Dupliquer' }), icon: <CopyPlus size={16} />, disabled: !a.duplicate, onClick: () => a.duplicate?.() },
    ...(a.openLink
      ? [{ id: 'link', label: t('sheet_obj_link', { defaultValue: 'Lien' }), icon: <Link2 size={16} />, onClick: a.openLink }]
      : []),
    { id: 'del', label: t('sheet_obj_delete', { defaultValue: 'Supprimer' }), shortcut: 'Suppr', icon: <Trash2 size={16} />, danger: true, onClick: a.remove },
    { id: 'format', label: t('sheet_obj_img_format', { defaultValue: 'Format de l’objet…' }), icon: <Paintbrush size={16} />, onClick: a.openFormat },
  ]
}

/** « Rotation » — 2-D quarter turns; free angles already come from the rotation knob. */
function rotationItem(args: ImageMenuArgs): MenuItem | null {
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
 * « Premier plan » / « Arrière-plan ». Paint order is per TYPE: pictures are painted
 * on the grid canvas while charts and shapes are DOM overlays in their own z-index
 * bands, so these move the picture inside the PICTURE array only — a picture can
 * never be lifted above a chart. Documented behaviour, not a bug.
 */
function orderSubmenus(args: ImageMenuArgs): MenuItem[] {
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
 * Block 2 — the picture's context menu.
 *
 * Order (Excel's, minus the omissions listed at the top of this file):
 * icon row ┊ Rétablir la taille de l'image · Changer d'image ▸ · Rogner · Rotation ▸ ┊
 * Premier plan ▸ · Arrière-plan ▸ ┊ Lien… · Ouvrir le lien ┊ Afficher le texte de
 * remplacement… · Taille et propriétés… · Format de l'objet… ┊ Supprimer l'image.
 */
export function buildImageMenu(args: ImageMenuArgs): MenuItem[] {
  const { actions: a, t } = args
  const rotate = rotationItem(args)

  return [
    ...iconRowItems(imageIconRow(args), args.theme ?? 'light'),

    ...(args.onResetSize
      ? [{ type: 'action' as const, label: t('sheet_obj_img_reset_size', { defaultValue: 'Rétablir la taille de l’image' }), icon: <RefreshCw size={15} />, onClick: args.onResetSize }]
      : []),
    changePictureItem(args),
    ...(args.onCrop
      ? [{ type: 'action' as const, label: t('sheet_obj_img_crop', { defaultValue: 'Rogner' }), icon: <Crop size={15} />, checked: !!args.cropping, onClick: args.onCrop }]
      : []),
    ...(rotate ? [rotate] : []),

    { type: 'separator' },
    ...orderSubmenus(args),

    { type: 'separator' },
    ...(a.openLink
      ? [{ type: 'action' as const, label: t('sheet_obj_link', { defaultValue: 'Lien' }), icon: <Link2 size={15} />, onClick: a.openLink }]
      : []),
    // Always shown, greyed out without a link — Excel's own behaviour.
    { type: 'action', label: t('sheet_obj_open_link', { defaultValue: 'Ouvrir le lien' }), disabled: !a.hasLink || !a.followLink, onClick: () => a.followLink?.() },

    { type: 'separator' },
    { type: 'action', label: t('sheet_obj_alt_text', { defaultValue: 'Afficher le texte de remplacement…' }), icon: <Accessibility size={15} />, onClick: a.openAltText },
    { type: 'action', label: t('sheet_obj_size_props', { defaultValue: 'Taille et propriétés…' }), icon: <Settings2 size={15} />, onClick: a.openSizeProps },
    { type: 'action', label: t('sheet_obj_img_format', { defaultValue: 'Format de l’objet…' }), icon: <Paintbrush size={15} />, onClick: a.openFormat },

    { type: 'separator' },
    { type: 'action', label: t('sheet_obj_img_delete', { defaultValue: 'Supprimer l’image' }), icon: <Trash2 size={15} />, danger: true, shortcut: 'Suppr', onClick: a.remove },
  ]
}

/**
 * Block 1 — the mini bar Excel floats next to the picture menu: [Style ▾][Rogner].
 *
 * « Style » is a gallery of frame presets (IMAGE_FRAME_PRESETS) written straight to
 * `border` / `borderWidth` / `shadow`; the canvas renderer paints them through
 * `imageFrame.ts`. A picture has no fill, hence no [Remplissage] here — free colours
 * and widths belong to « Format de l'objet… ».
 */
export function imageMiniControls(args: ImageMenuArgs): MiniControl[] {
  const { t } = args
  const active = activeImageFramePreset(args.image)
  const controls: MiniControl[] = [
    {
      id: 'style', kind: 'menu',
      label: t('sheet_obj_style', { defaultValue: 'Style' }),
      icon: <Frame size={14} />,
      render: close => (
        <div className="flex flex-col min-w-[168px]" role="group">
          {IMAGE_FRAME_PRESETS.map(p => (
            <button
              key={p.id}
              type="button"
              className="flex items-center gap-2 h-9 px-2 rounded hover:bg-hover text-left text-text-primary"
              style={{ background: p.id === active ? 'var(--kb-black-08, rgba(0,0,0,0.08))' : undefined }}
              onClick={() => { args.patchImage(p.patch); close() }}
            >
              {framePreview(p.id)}
              <span className="whitespace-nowrap">{t(p.labelKey, { defaultValue: p.labelDefault })}</span>
            </button>
          ))}
        </div>
      ),
    },
  ]
  if (args.onCrop) {
    controls.push({
      id: 'crop', kind: 'button',
      label: t('sheet_obj_img_crop', { defaultValue: 'Rogner' }),
      icon: <Crop size={14} />,
      active: !!args.cropping,
      onClick: args.onCrop,
    })
  }
  return controls
}
