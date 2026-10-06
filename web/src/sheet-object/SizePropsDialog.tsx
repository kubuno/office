// "Taille et propriétés…" — Excel's "Size & Properties" pane for a floating object
// (chart, picture, shape), as a two-tab dialog: Taille and Propriétés.
//
// Fully generic and controlled: it reads a geometry and hands a new one back. It knows
// nothing about the sheet, the editor or the object's kind.
//
// Geometry convention (shared by the three kinds, cf. sheet-chart/interaction.ts): a
// box in BASE pixels (zoom = 1) measured from the grid's data origin (top-left of A1),
// plus a rotation in degrees.

import { useRef, useState } from 'react'
import type { TFunction } from 'i18next'
import { Button, Checkbox, FloatingWindow, Radio, Tabs, type TabDef } from '@ui'
import { Ruler } from 'lucide-react'
import { DialogFooter, NumField, Section, norm360, r1, useDraft } from './dialogFields'

/** Box + rotation, in base px from the data origin. */
export interface SizePropsValue {
  bx: number
  by: number
  bw: number
  bh: number
  rot: number
}

/**
 * Excel's "object positioning" behaviour.
 *
 * NOT part of the sheet model: `SheetImage` / `SheetChart` / `SheetShape` carry an
 * explicit pixel box (plus, for imported objects, their original cell anchor), and no
 * per-object anchoring flag. The section is therefore OPT-IN — it only renders when
 * the host passes both `anchor` and `onAnchorChange` — so a host with nowhere to store
 * the choice shows no dead control. Should the model gain the field later, the values
 * map onto the xlsx anchor kinds: 'moveAndSize' → twoCellAnchor,
 * 'move' → oneCellAnchor, 'none' → absoluteAnchor.
 */
export type ObjAnchorMode = 'moveAndSize' | 'move' | 'none'

export interface SizePropsDialogProps {
  /** Current geometry; in uncontrolled mode (no `onChange`) it seeds the draft on mount. */
  value: SizePropsValue
  /** Provide to drive the dialog from the host (live resize). Omit for OK / Annuler. */
  onChange?: (next: SizePropsValue) => void
  /** Commit — the dialog does NOT close itself, the host unmounts it. */
  onSubmit: (next: SizePropsValue) => void
  onCancel: () => void
  /**
   * "Rétablir la taille d'origine" — the button only exists when this is given
   * (a chart has no intrinsic size; a picture and an imported shape do).
   * Return the object's restored geometry to keep the dialog open on the new values;
   * return nothing and the host is expected to close the dialog itself.
   */
  onResetSize?: () => SizePropsValue | void
  /** Initial state of "Conserver les proportions" (default true). */
  defaultLockRatio?: boolean
  /** Current anchoring behaviour — pass WITH `onAnchorChange` to show the section. */
  anchor?: ObjAnchorMode
  onAnchorChange?: (mode: ObjAnchorMode) => void
  /** Object name appended to the window title ("Image", "Graphique 2", "Forme 1"…). */
  title?: string
  t: TFunction
}

type TabId = 'size' | 'props'

export function SizePropsDialog({
  value, onChange, onSubmit, onCancel, onResetSize,
  defaultLockRatio = true, anchor, onAnchorChange, title, t,
}: SizePropsDialogProps) {
  const [draft, patch] = useDraft(value, onChange)
  const [tab, setTab] = useState<TabId>('size')
  const [lock, setLock] = useState(defaultLockRatio)
  // Aspect captured at open time, so repeated edits keep the ORIGINAL proportions
  // instead of drifting with each rounding.
  const ratio = useRef(value.bh > 0 ? value.bw / value.bh : 1)

  const head = t('sheet_obj_size_title', { defaultValue: 'Taille et propriétés' })
  const showAnchor = anchor !== undefined && !!onAnchorChange

  const setWidth = (n: number) => {
    const w = Math.max(1, n)
    patch(lock && ratio.current > 0 ? { bw: w, bh: r1(w / ratio.current) } : { bw: w })
  }
  const setHeight = (n: number) => {
    const h = Math.max(1, n)
    patch(lock && ratio.current > 0 ? { bh: h, bw: r1(h * ratio.current) } : { bh: h })
  }

  const reset = () => {
    const restored = onResetSize?.()
    if (!restored) return
    ratio.current = restored.bh > 0 ? restored.bw / restored.bh : 1
    patch(restored)
  }

  const submit = () => onSubmit({
    bx: draft.bx,
    by: draft.by,
    bw: Math.max(1, draft.bw),
    bh: Math.max(1, draft.bh),
    rot: norm360(draft.rot),
  })

  const tabs: TabDef<TabId>[] = [
    { id: 'size', label: t('sheet_obj_size_group', { defaultValue: 'Taille' }) },
    { id: 'props', label: t('sheet_obj_props_group', { defaultValue: 'Propriétés' }) },
  ]

  return (
    <FloatingWindow
      title={title ? `${head} — ${title}` : head}
      icon={<Ruler size={16} />}
      onClose={onCancel}
      defaultWidth={440}
      backdrop
    >
      <div className="flex flex-col p-4 gap-3" data-module="office">
        <Tabs tabs={tabs} value={tab} onChange={setTab} size="sm" />

        {tab === 'size' ? (
          <Section>
            <NumField label={t('sheet_obj_width', { defaultValue: 'Largeur' })} value={r1(draft.bw)} onChange={setWidth} suffix="px" min={1} />
            <NumField label={t('sheet_obj_height', { defaultValue: 'Hauteur' })} value={r1(draft.bh)} onChange={setHeight} suffix="px" min={1} />
            <NumField label={t('sheet_obj_rotation', { defaultValue: 'Rotation' })} value={Math.round(draft.rot)} onChange={n => patch({ rot: n })} suffix="°" />
            <Checkbox
              checked={lock}
              onChange={setLock}
              label={t('sheet_obj_lock_ratio', { defaultValue: 'Conserver les proportions' })}
            />
          </Section>
        ) : (
          <div className="space-y-4">
            <Section title={t('sheet_obj_pos_group', { defaultValue: 'Position' })}>
              <NumField label={t('sheet_obj_pos_x', { defaultValue: 'Horizontale' })} value={r1(draft.bx)} onChange={n => patch({ bx: n })} suffix="px" />
              <NumField label={t('sheet_obj_pos_y', { defaultValue: 'Verticale' })} value={r1(draft.by)} onChange={n => patch({ by: n })} suffix="px" />
              <div className="text-text-secondary">
                {t('sheet_obj_pos_help', { defaultValue: 'Mesurée depuis le coin supérieur gauche de la zone de données (A1).' })}
              </div>
            </Section>

            {showAnchor && (
              <Section title={t('sheet_obj_anchor_group', { defaultValue: 'Comportement d’ancrage' })}>
                <Radio
                  checked={anchor === 'moveAndSize'}
                  onChange={() => onAnchorChange?.('moveAndSize')}
                  label={t('sheet_obj_anchor_move_size', { defaultValue: 'Déplacer et dimensionner avec les cellules' })}
                />
                <Radio
                  checked={anchor === 'move'}
                  onChange={() => onAnchorChange?.('move')}
                  label={t('sheet_obj_anchor_move', { defaultValue: 'Déplacer sans dimensionner avec les cellules' })}
                />
                <Radio
                  checked={anchor === 'none'}
                  onChange={() => onAnchorChange?.('none')}
                  label={t('sheet_obj_anchor_none', { defaultValue: 'Ne pas déplacer ni dimensionner avec les cellules' })}
                />
              </Section>
            )}
          </div>
        )}

        <DialogFooter
          onOk={submit}
          onCancel={onCancel}
          t={t}
          left={onResetSize && tab === 'size' ? (
            <Button variant="ghost" onClick={reset}>
              {t('sheet_obj_reset_size', { defaultValue: 'Rétablir la taille d’origine' })}
            </Button>
          ) : undefined}
        />
      </div>
    </FloatingWindow>
  )
}
