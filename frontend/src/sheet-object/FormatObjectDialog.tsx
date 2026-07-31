// "Format de l'objet… / de la forme… / de la zone de graphique…" — the per-type
// format pane, as one generic dialog: Remplissage, Trait, and Texte.
//
// Fully generic and controlled: it reads a style and hands a new one back. It knows
// nothing about the sheet, the editor or the object's kind; the caller passes the
// window title and decides which sections exist:
//   • chart   → fill + line + text  (fonts given)
//   • shape   → fill + line + text  (fonts given)
//   • picture → line only           (showFill={false}, no fonts)
//
// Colour values follow the shared `ObjStyle` convention: "#RRGGBB", or the string
// 'none' for no fill / no outline.
//
// Why `ColorSwatchPicker` and not `ColorField`: `ColorField` opens its picker through
// a portal pinned at z-index 200, while `FloatingWindow` stacks from 1001 upwards —
// inside a dialog its palette is painted UNDERNEATH the dialog and cannot be reached.
// `ColorSwatchPicker` is the very same palette (and reaches the same full picker via
// its "Personnalisé" row) rendered INLINE, so it is laid out by the dialog itself.

import { useState } from 'react'
import type { TFunction } from 'i18next'
import { ColorSwatchPicker, FloatingWindow, FontPicker, Radio, Tabs, type TabDef } from '@ui'
import { Palette } from 'lucide-react'
import { DialogFooter, NumField, Section, SwatchDot, useDraft } from './dialogFields'

/** Fill / outline / text style of a floating object, as the object menus read it. */
export interface FormatObjectValue {
  /** "#RRGGBB" or 'none'. Undefined = the object's own default. */
  fill?: string
  /** Outline colour, "#RRGGBB" or 'none'. Undefined = the object's own default. */
  border?: string
  /** Outline width in px. */
  borderWidth?: number
  /** Text font family — only meaningful when the Texte section is shown. */
  font?: string
}

export interface FormatObjectDialogProps {
  /** Current style; in uncontrolled mode (no `onChange`) it seeds the draft on mount. */
  value: FormatObjectValue
  /** Provide to drive the dialog from the host (live restyle). Omit for OK / Annuler. */
  onChange?: (next: FormatObjectValue) => void
  /** Commit — the dialog does NOT close itself, the host unmounts it. */
  onSubmit: (next: FormatObjectValue) => void
  onCancel: () => void
  /**
   * Font families for the Texte section. The section is rendered ONLY when this is
   * given (charts and shapes); pictures pass nothing. Feed it the editor's list
   * (`useSystemFonts(SHEET_FONTS)`) so System/Fonts families are offered too.
   */
  fonts?: readonly string[]
  /** False hides the Remplissage section — a picture has no fill. Default true. */
  showFill?: boolean
  /** Colour proposed when switching a section from "aucun" to "couleur unie". */
  defaultFill?: string
  defaultBorder?: string
  /** Full window title, e.g. t('sheet_obj_chart_format'). */
  title?: string
  /**
   * Tab to land on. Defaults to Remplissage (Contour when there is no fill); the chart
   * menu's « Police… » entry opens the very same pane straight on 'text' rather than
   * duplicating a font-only dialog. Ignored when that tab is not rendered.
   */
  initialTab?: 'fill' | 'line' | 'text'
  /** Sample string drawn in the live preview (default "AaBb"). */
  previewText?: string
  t: TFunction
}

type TabId = 'fill' | 'line' | 'text'

/** A section is either off ('none') or a plain colour. */
const modeOf = (v: string | undefined) => (v === 'none' ? 'none' : 'solid')
/** Colour to show / edit for a section whose value may be absent or 'none'. */
const colorOf = (v: string | undefined, fallback: string) => (v && v !== 'none' ? v : fallback)

export function FormatObjectDialog({
  value, onChange, onSubmit, onCancel,
  fonts, showFill = true, defaultFill = '#ffffff', defaultBorder = '#80868b',
  title, initialTab, previewText = 'AaBb', t,
}: FormatObjectDialogProps) {
  const [draft, patch] = useDraft(value, onChange)
  const showText = !!fonts && fonts.length > 0
  const [tab, setTab] = useState<TabId>(() => {
    // A requested tab that this object does not render falls back to the default one.
    if (initialTab === 'fill' && showFill) return 'fill'
    if (initialTab === 'line') return 'line'
    if (initialTab === 'text' && showText) return 'text'
    return showFill ? 'fill' : 'line'
  })

  const fillMode = modeOf(draft.fill)
  const lineMode = modeOf(draft.border)
  const fillColor = colorOf(draft.fill, defaultFill)
  const lineColor = colorOf(draft.border, defaultBorder)
  const lineWidth = draft.borderWidth ?? 1

  const tabs: TabDef<TabId>[] = [
    ...(showFill ? [{ id: 'fill' as TabId, label: t('sheet_obj_fill', { defaultValue: 'Remplissage' }) }] : []),
    { id: 'line', label: t('sheet_obj_outline', { defaultValue: 'Contour' }) },
    ...(showText ? [{ id: 'text' as TabId, label: t('sheet_obj_text_group', { defaultValue: 'Texte' }) }] : []),
  ]

  return (
    <FloatingWindow
      // The caller passes the MENU label, whose trailing ellipsis means "opens a
      // dialog" — once the dialog is open it is noise, so it is dropped here rather
      // than duplicating three labels in the 13 language blocks. Both the single
      // character and the three-dot spelling are handled.
      title={(title ?? t('sheet_obj_format_title', { defaultValue: 'Format de l’objet' })).replace(/(…|\.\.\.)\s*$/, '')}
      icon={<Palette size={16} />}
      onClose={onCancel}
      defaultWidth={460}
      backdrop
      className="max-h-[92vh]"
    >
      <div className="flex flex-col min-h-0 p-4 gap-3" data-module="office">
        {tabs.length > 1 && <Tabs tabs={tabs} value={tab} onChange={setTab} size="sm" />}

        <div className="min-h-0 overflow-y-auto">
          {tab === 'fill' && showFill && (
            <Section>
              {/* `Radio` is inline-flex, so the section's vertical rhythm does not apply
                  to it: the choices are stacked by an explicit column here. */}
              <div className="flex flex-col gap-2 items-start">
                <Radio
                  checked={fillMode === 'none'}
                  onChange={() => patch({ fill: 'none' })}
                  label={t('sheet_obj_no_fill', { defaultValue: 'Aucun remplissage' })}
                />
                <Radio
                  checked={fillMode === 'solid'}
                  onChange={() => patch({ fill: fillColor })}
                  label={t('sheet_obj_fill_solid', { defaultValue: 'Couleur unie' })}
                />
              </div>
              {fillMode === 'solid' && (
                <ColorSwatchPicker color={fillColor} t={t} onChange={hex => patch({ fill: hex })} />
              )}
            </Section>
          )}

          {tab === 'line' && (
            <Section>
              <div className="flex flex-col gap-2 items-start">
                <Radio
                  checked={lineMode === 'none'}
                  onChange={() => patch({ border: 'none' })}
                  label={t('sheet_obj_no_outline', { defaultValue: 'Aucun contour' })}
                />
                <Radio
                  checked={lineMode === 'solid'}
                  onChange={() => patch({ border: lineColor, borderWidth: lineWidth })}
                  label={t('sheet_obj_outline_solid', { defaultValue: 'Couleur unie' })}
                />
              </div>
              {lineMode === 'solid' && (
                <>
                  <NumField
                    label={t('sheet_obj_border_width', { defaultValue: 'Épaisseur' })}
                    value={lineWidth}
                    onChange={n => patch({ borderWidth: Math.max(0.25, Math.min(24, n)) })}
                    suffix="px"
                    min={0.25}
                    max={24}
                    step={0.25}
                  />
                  <ColorSwatchPicker color={lineColor} t={t} onChange={hex => patch({ border: hex })} />
                </>
              )}
            </Section>
          )}

          {tab === 'text' && showText && (
            <Section>
              <label className="flex items-center gap-2">
                <span className="text-text-secondary shrink-0" style={{ width: 112 }}>
                  {t('sheet_obj_font', { defaultValue: 'Police' })}
                </span>
                <FontPicker
                  value={draft.font ?? ''}
                  onChange={f => patch({ font: f })}
                  fonts={fonts ?? []}
                  width={220}
                  height={32}
                />
              </label>
              <div className="text-text-secondary">
                {t('sheet_obj_font_help', { defaultValue: 'S’applique à tout le texte de l’objet.' })}
              </div>
            </Section>
          )}
        </div>

        {/* Live preview — plain CSS, so it costs one repaint and no measurement. */}
        <Section title={t('sheet_obj_preview', { defaultValue: 'Aperçu' })}>
          <div
            className="flex items-center justify-center h-20 rounded"
            style={{
              // Chequerboard behind the sample so "aucun remplissage" reads as empty.
              backgroundImage: 'linear-gradient(45deg, rgba(0,0,0,0.06) 25%, transparent 25%, transparent 75%, rgba(0,0,0,0.06) 75%), linear-gradient(45deg, rgba(0,0,0,0.06) 25%, transparent 25%, transparent 75%, rgba(0,0,0,0.06) 75%)',
              backgroundSize: '12px 12px',
              backgroundPosition: '0 0, 6px 6px',
            }}
          >
            <div
              className="flex items-center justify-center w-40 h-14 rounded"
              style={{
                background: fillMode === 'solid' && showFill ? fillColor : 'transparent',
                border: lineMode === 'solid' ? `${lineWidth}px solid ${lineColor}` : 'none',
                fontFamily: draft.font || undefined,
              }}
            >
              <span className="flex items-center gap-1.5">
                <SwatchDot color={fillMode === 'solid' && showFill ? fillColor : 'none'} size={10} />
                {previewText}
              </span>
            </div>
          </div>
        </Section>

        <DialogFooter onOk={() => onSubmit(draft)} onCancel={onCancel} t={t} />
      </div>
    </FloatingWindow>
  )
}
