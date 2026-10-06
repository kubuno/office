// The documents editor's shape ribbon surfaces, both built on the SHARED ones.
//
//  • « Illustrations » — the shared `shapesIllustrationGroup`, i.e. the same
//    dropdown and the same gallery as the spreadsheet, the slides and the board.
//    Documents adds its own « Image » and « Zone de texte » buttons around it.
//    Picking a geometry ARMS the draw tool; nothing is inserted until the drag.
//  • « Format de la forme » — the shared `shapeRibbonTab`, tailored through its
//    override contract with what only documents has: text wrapping, page
//    alignment and an outline thickness expressed the way our SVG renderer takes
//    it (a fraction of the shape's smaller side, so it survives a resize).

import type { TFunction } from 'i18next'
import { WrapText, AlignLeft, AlignCenter, AlignRight } from 'lucide-react'
import type { RibbonGroup, RibbonItem, RibbonTab } from '../../ribbon/types'
import { shapesIllustrationGroup } from '../../shapes/ShapesInsertButton'
import { shapeRibbonTab, SHAPE_TAB_GROUP, type ShapeOrderOp } from '../../shapes/shapeRibbonTab'
import type { ShapeKind } from '../../shapes/catalog'
import { shapeAlt, shapeSrcOf, type DocShapeParams } from './params'

// ── Insertion ▸ Illustrations ────────────────────────────────────────────────

export interface DocIllustrationsArgs {
  t: TFunction
  /** Geometry currently armed (highlighted in the gallery), if any. */
  armed: ShapeKind | null
  /** Arm the draw tool with that geometry. */
  onArm: (kind: ShapeKind) => void
  /** « Image » button — placed before the shapes dropdown. */
  imageItem: RibbonItem
  /** « Zone de texte » button — placed after it (the gallery has no text box). */
  textBoxItem: RibbonItem
  groupLabel: string
}

export function docIllustrationsGroup(a: DocIllustrationsArgs): RibbonGroup {
  return shapesIllustrationGroup({
    t: a.t,
    onPick: a.onArm,
    current: a.armed ?? undefined,
    groupLabel: a.groupLabel,
    leadingItems: [a.imageItem],
    trailingItems: [a.textBoxItem],
  })
}

// ── Onglet contextuel « Format de la forme » ─────────────────────────────────

/**
 * Outline thicknesses offered by the tab. Our renderer takes the stroke width as
 * a FRACTION of the shape's smaller side (`shapeSvg`'s `swFrac`), so the outline
 * keeps its look when the shape is resized — as it does in Office, where the
 * width is a property of the line and not of the box.
 */
const OUTLINE_WIDTHS: Array<[number, string]> = [
  [0.004, 'Fin'],
  [0.0075, 'Normal'],
  [0.015, 'Épais'],
  [0.025, 'Très épais'],
]
/** Default of `shapeSvg` when the payload carries no width. */
const OUTLINE_DEFAULT = 0.0075

/** Wrap modes a shape can take — the same list as the image tab. */
const WRAP_MODES: Array<[string, string]> = [
  ['inline', 'Aligné texte'],
  ['square', 'Carré'],
  ['topBottom', 'Haut et bas'],
  ['behind', 'Derrière'],
  ['front', 'Devant'],
]

export interface DocShapeTabCtx {
  t: TFunction
  /** Payload of the selected shape, or null when the selection is not a shape. */
  params: DocShapeParams | null
  /** Node size in document px — the bitmap is regenerated at that size. */
  size: { w: number; h: number }
  rotation: number
  wrap: string
  /** The shape rides in the text flow (`inlineImage`): no wrapping, no alignment. */
  inline: boolean
  /** Patch the selected node's attributes. */
  update: (attrs: Record<string, unknown>) => void
  setWrap: (wrap: string) => void
  order: (op: ShapeOrderOp) => void
  remove: () => void
  /** « Taille et propriétés » — le dialogue Position/Habillage/Taille façon Word. */
  openLayout: () => void
}

export function docShapeRibbonTab(c: DocShapeTabCtx): RibbonTab {
  const { t, params } = c

  // Restyling regenerates the bitmap AND the payload in one edit: the two must
  // never disagree (see params.ts).
  const restyle = (patch: { fill?: string; border?: string; borderWidth?: number }) => {
    if (!params) return
    const next: DocShapeParams = {
      ...params,
      ...(patch.fill !== undefined ? { fill: patch.fill } : {}),
      ...(patch.border !== undefined ? { stroke: patch.border } : {}),
      ...(patch.borderWidth !== undefined ? { sw: patch.borderWidth } : {}),
    }
    c.update({ src: shapeSrcOf(next, c.size.w, c.size.h), alt: shapeAlt(next) })
  }

  const outlineWidth: RibbonItem = {
    id: 'shape_sw', kind: 'dropdown', width: 116,
    value: String(params?.sw ?? OUTLINE_DEFAULT),
    options: OUTLINE_WIDTHS.map(([v, l]) => ({
      value: String(v), label: t(`doc_shape_sw_${String(v).replace('.', '_')}`, { defaultValue: l }),
    })),
    onChange: v => restyle({ borderWidth: parseFloat(v) }),
  }

  // Wrapping and page alignment are what a shape in a TEXT means; the shared tab
  // has no notion of either, so documents adds them to « Organiser ».
  const arrangeExtras: RibbonItem[] = c.inline ? [] : [
    { id: 'shape_sep', kind: 'separator' },
    {
      id: 'shape_wrap', kind: 'dropdown', width: 128, value: c.wrap || 'inline',
      options: WRAP_MODES.map(([v, l]) => ({ value: v, label: t(`doc_wrap_${v}`, { defaultValue: l }) })),
      onChange: v => c.setWrap(v),
    },
    ...(([
      ['left', AlignLeft, 'doc_align_left', 'Aligner à gauche'],
      ['center', AlignCenter, 'doc_align_center', 'Centrer'],
      ['right', AlignRight, 'doc_align_right', 'Aligner à droite'],
    ] as Array<[string, typeof AlignLeft, string, string]>).map(([v, Icon, key, def]) => ({
      id: `shape_align_${v}`, kind: 'button' as const, icon: <Icon size={15} />,
      label: t(key, { defaultValue: def }),
      // `align` drives the non-floating case, `alignH` the floating one (Word's
      // `wp:align`, which wins over the manual offset).
      onClick: () => c.update({ align: v, alignH: v }),
    }))),
  ]

  return shapeRibbonTab(
    {
      t,
      id: 'ctx-shape',
      visible: !!params,
      // Documents selects ONE object at a time: multi-object align/distribute
      // cannot apply, so the shared button is dropped rather than shown disabled.
      count: params ? 1 : 0,
      style: params ? { fill: params.fill, border: params.stroke, borderWidth: params.sw } : undefined,
      setStyle: patch => restyle(patch),
      align: () => {},
      order: c.order,
      rotate: (deg, reset) => c.update({ rotation: reset ? 0 : Math.round(((c.rotation + deg) % 360 + 360) % 360) }),
      remove: c.remove,
      openSizeProps: c.openLayout,
    },
    {
      label: t('doc_tab_shape', { defaultValue: 'Format de la forme' }),
      groupLabel: t('doc_shape_tools', { defaultValue: 'Outils de dessin' }),
      hideItems: ['align'],
      addItems: {
        [SHAPE_TAB_GROUP.style]: [outlineWidth],
        [SHAPE_TAB_GROUP.arrange]: arrangeExtras,
      },
      // « Habillage » deserves its own icon in the group heading area; nothing to
      // transform otherwise.
      transform: groups => groups.map(g => g.id === SHAPE_TAB_GROUP.arrange
        ? { ...g, items: g.items.map(it => it.id === 'shape_wrap' ? { ...it, icon: <WrapText size={15} /> } : it) }
        : g),
    },
  )
}
