import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input, ColorField } from '@ui'
import { Palette, Trash2, Plus } from 'lucide-react'
import type { CondBlock, CondRule, CondStyle, ColorScale } from './formula-engine'

interface Props {
  blocks:       CondBlock[]
  selectionRef: string                       // A1 range the new rule defaults to
  onApply:      (blocks: CondBlock[]) => void
  onClose:      () => void
}

type Kind = 'cell' | 'text' | 'empty' | 'formula' | 'scale'
type CellOp = 'gt' | 'lt' | 'ge' | 'le' | 'eq' | 'ne' | 'between'
type TextOp = 'contains' | 'ncontains' | 'starts' | 'ends'

// Excel-style fill/text presets for "format when true" rules.
const PRESETS: { label: string; dxf: CondStyle }[] = [
  { label: 'Rouge clair / texte rouge',  dxf: { bg: '#ffc7ce', color: '#9c0006' } },
  { label: 'Jaune clair / texte jaune',  dxf: { bg: '#ffeb9c', color: '#9c6500' } },
  { label: 'Vert clair / texte vert',    dxf: { bg: '#c6efce', color: '#006100' } },
  { label: 'Remplissage rouge clair',    dxf: { bg: '#ffc7ce' } },
  { label: 'Texte rouge gras',           dxf: { color: '#9c0006', bold: true } },
]
const SCALES: { label: string; cs: ColorScale }[] = [
  { label: 'Vert → Jaune → Rouge', cs: { lo: '#57bb8a', mid: '#ffd666', hi: '#e67c73' } },
  { label: 'Rouge → Jaune → Vert', cs: { lo: '#e67c73', mid: '#ffd666', hi: '#57bb8a' } },
  { label: 'Blanc → Bleu',         cs: { lo: '#ffffff', hi: '#5a9bdc' } },
  { label: 'Blanc → Vert',         cs: { lo: '#ffffff', hi: '#57bb8a' } },
]

// Top-left anchor cell of an A1 range ("A1:C10" → "A1", "B2" → "B2").
const anchorOf = (ref: string) => (ref.split(':')[0] || ref).replace(/\$/g, '').toUpperCase()
// Quote a value as a formula literal: numeric stays bare, text gets quoted+escaped.
const lit = (v: string) => (v.trim() !== '' && !isNaN(+v) ? v.trim() : `"${v.replace(/"/g, '""')}"`)

export default function ConditionalFormatDialog({ blocks, selectionRef, onApply, onClose }: Props) {
  const { t } = useTranslation('office')

  const [range, setRange]   = useState(selectionRef || 'A1')
  const [kind, setKind]     = useState<Kind>('cell')
  const [cellOp, setCellOp] = useState<CellOp>('gt')
  const [textOp, setTextOp] = useState<TextOp>('contains')
  const [v1, setV1]         = useState('')
  const [v2, setV2]         = useState('')
  const [emptyMode, setEmptyMode] = useState<'empty' | 'notempty'>('empty')
  const [formula, setFormula]     = useState('')
  const [dxf, setDxf]       = useState<CondStyle>({ bg: '#ffc7ce', color: '#9c0006' })
  const [scale, setScale]   = useState<ColorScale>(SCALES[0].cs)

  // Build the engine rule (expression formula anchored at the range's top-left).
  const buildRule = (): CondRule | null => {
    if (kind === 'scale') return { type: 'colorScale', op: scale.mid ? '3' : '2', formulas: [], dxf: {}, stop: false, cs: scale }
    const a = anchorOf(range)
    let f = ''
    if (kind === 'cell') {
      if (cellOp === 'between') { if (!v1 || !v2) return null; f = `=AND(${a}>=${lit(v1)},${a}<=${lit(v2)})` }
      else {
        if (v1.trim() === '' && cellOp !== 'eq') return null
        const ops: Record<Exclude<CellOp, 'between'>, string> = { gt: '>', lt: '<', ge: '>=', le: '<=', eq: '=', ne: '<>' }
        f = `=${a}${ops[cellOp]}${lit(v1)}`
      }
    } else if (kind === 'text') {
      if (!v1) return null
      if (textOp === 'contains')   f = `=ISNUMBER(SEARCH(${lit(v1)},${a}))`
      else if (textOp === 'ncontains') f = `=NOT(ISNUMBER(SEARCH(${lit(v1)},${a})))`
      else if (textOp === 'starts') f = `=LEFT(${a},LEN(${lit(v1)}))=${lit(v1)}`
      else f = `=RIGHT(${a},LEN(${lit(v1)}))=${lit(v1)}`
    } else if (kind === 'empty') {
      f = emptyMode === 'empty' ? `=${a}=""` : `=${a}<>""`
    } else if (kind === 'formula') {
      if (!formula.trim()) return null
      f = formula.trim().startsWith('=') ? formula.trim() : `=${formula.trim()}`
    }
    return { type: 'expression', op: '', formulas: [f], dxf, stop: true }
  }

  const addRule = () => {
    const rule = buildRule(); if (!rule || !range.trim()) return
    // New rules take priority (computeCondFormats: first matching block wins).
    onApply([{ ranges: [range.trim().toUpperCase()], rules: [rule] }, ...blocks])
  }
  const removeBlock = (i: number) => onApply(blocks.filter((_, idx) => idx !== i))

  const ruleSummary = (b: CondBlock): string => {
    const r = b.rules[0]
    if (!r) return '—'
    if (r.type === 'colorScale') return t('cf_sum_scale', { defaultValue: 'Échelle de couleurs' })
    return r.formulas[0] || '—'
  }
  const swatch = (b: CondBlock) => {
    const r = b.rules[0]
    if (r?.cs) return `linear-gradient(90deg, ${r.cs.lo}, ${r.cs.mid ?? r.cs.hi}, ${r.cs.hi})`
    return r?.dxf?.bg ?? 'transparent'
  }

  const sel = "h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary"

  return (
    <FloatingWindow
      title={t('cf_title', { defaultValue: 'Mise en forme conditionnelle' })}
      icon={<Palette size={16} />}
      onClose={onClose}
      backdrop resizable
      defaultWidth={520} defaultHeight={560} minWidth={440} minHeight={420}
    >
      <div className="flex flex-col h-full text-sm" data-module="office">
        {/* Existing rules */}
        <div className="border border-border rounded overflow-auto" style={{ maxHeight: 150 }}>
          {blocks.length === 0 ? (
            <div className="p-4 text-center text-text-tertiary text-xs">
              {t('cf_empty', { defaultValue: 'Aucune règle. Créez-en une ci-dessous.' })}
            </div>
          ) : blocks.map((b, i) => (
            <div key={i} className="flex items-center gap-2 px-2 py-1.5 border-b border-border last:border-0">
              <span className="w-6 h-5 rounded border border-border flex-shrink-0" style={{ background: swatch(b) }} />
              <span className="text-xs text-text-secondary flex-shrink-0 w-20 truncate" title={b.ranges.join(', ')}>{b.ranges.join(', ')}</span>
              <span className="text-xs font-mono flex-1 truncate" title={ruleSummary(b)}>{ruleSummary(b)}</span>
              <button className="p-1 rounded hover:bg-danger-light text-danger" onClick={() => removeBlock(i)} title={t('cf_delete', { defaultValue: 'Supprimer' })}>
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </div>

        {/* New rule */}
        <div className="mt-3 pt-3 border-t border-border space-y-2 overflow-auto flex-1">
          <div className="flex items-end gap-2">
            <div className="w-36">
              <Input label={t('cf_range', { defaultValue: 'Appliquer à la plage' })} value={range} onChange={e => setRange(e.target.value)} className="font-mono" />
            </div>
            <div className="flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t('cf_kind', { defaultValue: 'Type de règle' })}</label>
              <select className={`${sel} w-full`} value={kind} onChange={e => setKind(e.target.value as Kind)}>
                <option value="cell">{t('cf_k_cell', { defaultValue: 'La valeur de la cellule' })}</option>
                <option value="text">{t('cf_k_text', { defaultValue: 'Le texte' })}</option>
                <option value="empty">{t('cf_k_empty', { defaultValue: 'Cellule vide / non vide' })}</option>
                <option value="formula">{t('cf_k_formula', { defaultValue: 'Formule personnalisée' })}</option>
                <option value="scale">{t('cf_k_scale', { defaultValue: 'Échelle de couleurs' })}</option>
              </select>
            </div>
          </div>

          {/* Criterion */}
          {kind === 'cell' && (
            <div className="flex items-center gap-2">
              <select className={sel} value={cellOp} onChange={e => setCellOp(e.target.value as CellOp)}>
                <option value="gt">{t('cf_op_gt', { defaultValue: 'supérieure à' })}</option>
                <option value="lt">{t('cf_op_lt', { defaultValue: 'inférieure à' })}</option>
                <option value="ge">{t('cf_op_ge', { defaultValue: 'supérieure ou égale à' })}</option>
                <option value="le">{t('cf_op_le', { defaultValue: 'inférieure ou égale à' })}</option>
                <option value="eq">{t('cf_op_eq', { defaultValue: 'égale à' })}</option>
                <option value="ne">{t('cf_op_ne', { defaultValue: 'différente de' })}</option>
                <option value="between">{t('cf_op_between', { defaultValue: 'comprise entre' })}</option>
              </select>
              <input className={`${sel} flex-1`} placeholder={t('cf_value', { defaultValue: 'valeur' })} value={v1} onChange={e => setV1(e.target.value)} />
              {cellOp === 'between' && <>
                <span className="text-text-secondary">{t('cf_and', { defaultValue: 'et' })}</span>
                <input className={`${sel} flex-1`} placeholder={t('cf_value', { defaultValue: 'valeur' })} value={v2} onChange={e => setV2(e.target.value)} />
              </>}
            </div>
          )}
          {kind === 'text' && (
            <div className="flex items-center gap-2">
              <select className={sel} value={textOp} onChange={e => setTextOp(e.target.value as TextOp)}>
                <option value="contains">{t('cf_t_contains', { defaultValue: 'contient' })}</option>
                <option value="ncontains">{t('cf_t_ncontains', { defaultValue: 'ne contient pas' })}</option>
                <option value="starts">{t('cf_t_starts', { defaultValue: 'commence par' })}</option>
                <option value="ends">{t('cf_t_ends', { defaultValue: 'se termine par' })}</option>
              </select>
              <input className={`${sel} flex-1`} placeholder={t('cf_text', { defaultValue: 'texte' })} value={v1} onChange={e => setV1(e.target.value)} />
            </div>
          )}
          {kind === 'empty' && (
            <select className={`${sel} w-full`} value={emptyMode} onChange={e => setEmptyMode(e.target.value as 'empty' | 'notempty')}>
              <option value="empty">{t('cf_is_empty', { defaultValue: 'est vide' })}</option>
              <option value="notempty">{t('cf_is_notempty', { defaultValue: 'n’est pas vide' })}</option>
            </select>
          )}
          {kind === 'formula' && (
            <input className={`${sel} w-full font-mono`} placeholder="=$A1>MOYENNE($A:$A)" value={formula} onChange={e => setFormula(e.target.value)} />
          )}

          {/* Format / scale */}
          {kind === 'scale' ? (
            <div className="space-y-2">
              <div className="flex flex-wrap gap-1.5">
                {SCALES.map(s => (
                  <button key={s.label} onClick={() => setScale(s.cs)}
                    className={`h-7 w-20 rounded border ${scale === s.cs ? 'border-primary ring-1 ring-primary' : 'border-border'}`}
                    style={{ background: `linear-gradient(90deg, ${s.cs.lo}, ${s.cs.mid ?? s.cs.hi}, ${s.cs.hi})` }} title={s.label} />
                ))}
              </div>
              <div className="flex items-center gap-3 text-xs text-text-secondary">
                <span className="flex items-center gap-1">{t('cf_min', { defaultValue: 'Min' })} <ColorField width={24} height={18} color={scale.lo} onChange={hex => setScale(s => ({ ...s, lo: hex }))} /></span>
                {scale.mid != null && <span className="flex items-center gap-1">{t('cf_mid', { defaultValue: 'Milieu' })} <ColorField width={24} height={18} color={scale.mid} onChange={hex => setScale(s => ({ ...s, mid: hex }))} /></span>}
                <span className="flex items-center gap-1">{t('cf_max', { defaultValue: 'Max' })} <ColorField width={24} height={18} color={scale.hi} onChange={hex => setScale(s => ({ ...s, hi: hex }))} /></span>
                <button className="ml-auto text-primary hover:underline"
                  onClick={() => setScale(s => s.mid != null ? { lo: s.lo, hi: s.hi } : { lo: s.lo, mid: '#ffffff', hi: s.hi })}>
                  {scale.mid != null ? t('cf_2color', { defaultValue: '2 couleurs' }) : t('cf_3color', { defaultValue: '3 couleurs' })}
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-2">
              <label className="block text-xs text-text-secondary">{t('cf_format', { defaultValue: 'Mise en forme si la condition est vraie' })}</label>
              <div className="flex flex-wrap gap-1.5">
                {PRESETS.map(p => (
                  <button key={p.label} onClick={() => setDxf(p.dxf)} title={p.label}
                    className={`px-2 h-7 rounded border text-xs font-medium ${dxf === p.dxf ? 'border-primary ring-1 ring-primary' : 'border-border'}`}
                    style={{ background: p.dxf.bg ?? '#fff', color: p.dxf.color ?? '#000', fontWeight: p.dxf.bold ? 700 : 400 }}>
                    123
                  </button>
                ))}
              </div>
              <div className="flex items-center gap-3 text-xs text-text-secondary">
                <span className="flex items-center gap-1">{t('cf_fill', { defaultValue: 'Remplissage' })} <ColorField width={24} height={18} color={dxf.bg ?? '#ffffff'} onChange={hex => setDxf(d => ({ ...d, bg: hex }))} /></span>
                <span className="flex items-center gap-1">{t('cf_textcolor', { defaultValue: 'Texte' })} <ColorField width={24} height={18} color={dxf.color ?? '#000000'} onChange={hex => setDxf(d => ({ ...d, color: hex }))} /></span>
                <button className={`px-2 h-6 rounded border ${dxf.bold ? 'border-primary bg-primary-light' : 'border-border'}`} onClick={() => setDxf(d => ({ ...d, bold: !d.bold }))}><b>B</b></button>
                <button className={`px-2 h-6 rounded border ${dxf.italic ? 'border-primary bg-primary-light' : 'border-border'}`} onClick={() => setDxf(d => ({ ...d, italic: !d.italic }))}><i>I</i></button>
              </div>
            </div>
          )}
        </div>

        <div className="pt-2 mt-2 border-t border-border flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('cf_close', { defaultValue: 'Fermer' })}</Button>
          <Button variant="primary" onClick={addRule}><Plus size={14} /> {t('cf_add', { defaultValue: 'Ajouter la règle' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
