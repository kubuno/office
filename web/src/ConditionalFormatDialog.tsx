import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { DLG_BTN } from './lib'
import { FloatingWindow, Button, Input, ColorField, Dropdown } from '@ui'
import { Palette, Trash2, Plus } from 'lucide-react'
import type { CondBlock, CondRule, CondStyle, ColorScale } from './formula-engine'

interface Props {
  blocks:       CondBlock[]
  selectionRef: string                       // A1 range the new rule defaults to
  onApply:      (blocks: CondBlock[]) => void
  onClose:      () => void
}

type Kind = 'cell' | 'text' | 'empty' | 'formula' | 'scale' | 'bar' | 'icons' | 'top' | 'avg' | 'dupes'
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
const BAR_COLORS = ['#638ec6', '#63c384', '#ff555a', '#ffb628', '#8957e5']

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
  const [barColor, setBarColor]   = useState(BAR_COLORS[0])
  const [iconSet, setIconSet]     = useState('3Arrows')
  const [topRank, setTopRank]     = useState('10')
  const [topSide, setTopSide]     = useState<'top' | 'bottom'>('top')
  const [topUnit, setTopUnit]     = useState<'values' | 'percent'>('values')
  const [avgMode, setAvgMode]     = useState<'above' | 'below'>('above')
  const [dupMode, setDupMode]     = useState<'dup' | 'uniq'>('dup')

  // Build the engine rule (expression formula anchored at the range's top-left).
  const buildRule = (): CondRule | null => {
    const base = { op: '', formulas: [] as string[], dxf: {} as CondStyle, stop: false }
    if (kind === 'scale') return { ...base, type: 'colorScale', op: scale.mid ? '3' : '2', cs: scale }
    if (kind === 'bar')   return { ...base, type: 'dataBar', bar: { color: barColor } }
    if (kind === 'icons') return { ...base, type: 'iconSet', icons: { set: iconSet } }
    if (kind === 'top') {
      const r = parseInt(topRank, 10)
      if (!r || r < 1) return null
      const rule: CondRule = { ...base, type: 'top10', dxf, stop: true, rank: r }
      if (topUnit === 'percent') rule.percent = true
      if (topSide === 'bottom') rule.bottom = true
      return rule
    }
    if (kind === 'avg') {
      const rule: CondRule = { ...base, type: 'aboveAverage', dxf, stop: true }
      if (avgMode === 'below') rule.above = false
      return rule
    }
    if (kind === 'dupes') return { ...base, type: dupMode === 'dup' ? 'duplicateValues' : 'uniqueValues', dxf, stop: true }
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
    // New rules take priority (computeCondFormats: unprioritised rules run first).
    onApply([{ ranges: [range.trim().toUpperCase()], rules: [rule] }, ...blocks])
  }
  const removeBlock = (i: number) => onApply(blocks.filter((_, idx) => idx !== i))

  const ruleSummary = (b: CondBlock): string => {
    const r = b.rules[0]
    if (!r) return '—'
    switch (r.type) {
      case 'colorScale': return t('cf_sum_scale', { defaultValue: 'Échelle de couleurs' })
      case 'dataBar': return t('cf_sum_bar', { defaultValue: 'Barre de données' })
      case 'iconSet': return t('cf_sum_icons', { defaultValue: 'Jeu d’icônes' })
      case 'top10': return `${r.bottom ? t('cf_bottom', { defaultValue: 'Derniers' }) : t('cf_top', { defaultValue: 'Premiers' })} ${r.rank ?? 10}${r.percent ? ' %' : ''}`
      case 'aboveAverage': return r.above === false ? t('cf_below_avg', { defaultValue: 'Sous la moyenne' }) : t('cf_above_avg', { defaultValue: 'Au-dessus de la moyenne' })
      case 'duplicateValues': return t('cf_dup', { defaultValue: 'Valeurs en double' })
      case 'uniqueValues': return t('cf_uniq', { defaultValue: 'Valeurs uniques' })
      case 'timePeriod': return r.period ?? 'timePeriod'
      case 'containsText': case 'notContainsText': case 'beginsWith': case 'endsWith':
        return r.text ? `${r.type} « ${r.text} »` : (r.formulas[0] || r.type)
      default: return r.formulas[0] || r.type || '—'
    }
  }
  const swatch = (b: CondBlock) => {
    const r = b.rules[0]
    if (r?.cs) return `linear-gradient(90deg, ${r.cs.lo}, ${r.cs.mid ?? r.cs.hi}, ${r.cs.hi})`
    if (r?.type === 'dataBar') return `linear-gradient(90deg, ${r.bar?.color ?? '#638ec6'} 60%, transparent 60%)`
    if (r?.type === 'iconSet') return 'linear-gradient(90deg, #d93025 33%, #f9ab00 33%, #f9ab00 66%, #188038 66%)'
    return r?.dxf?.bg ?? 'transparent'
  }

  const sel = "h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary"
  const showDxf = !['scale', 'bar', 'icons'].includes(kind)

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
              <Dropdown className="w-full" value={kind} onChange={v => setKind(v as Kind)}
                options={[{ value: 'cell', label: t('cf_k_cell', { defaultValue: 'La valeur de la cellule' }) },
                          { value: 'text', label: t('cf_k_text', { defaultValue: 'Le texte' }) },
                          { value: 'empty', label: t('cf_k_empty', { defaultValue: 'Cellule vide / non vide' }) },
                          { value: 'formula', label: t('cf_k_formula', { defaultValue: 'Formule personnalisée' }) },
                          { value: 'top', label: t('cf_k_top', { defaultValue: 'Premières / dernières valeurs' }) },
                          { value: 'avg', label: t('cf_k_avg', { defaultValue: 'Au-dessus / sous la moyenne' }) },
                          { value: 'dupes', label: t('cf_k_dupes', { defaultValue: 'Valeurs en double / uniques' }) },
                          { value: 'scale', label: t('cf_k_scale', { defaultValue: 'Échelle de couleurs' }) },
                          { value: 'bar', label: t('cf_k_bar', { defaultValue: 'Barre de données' }) },
                          { value: 'icons', label: t('cf_k_icons', { defaultValue: 'Jeu d’icônes' }) }]} />
            </div>
          </div>

          {/* Criterion */}
          {kind === 'cell' && (
            <div className="flex items-center gap-2">
              <Dropdown width={200} value={cellOp} onChange={v => setCellOp(v as CellOp)}
                options={[{ value: 'gt', label: t('cf_op_gt', { defaultValue: 'supérieure à' }) },
                          { value: 'lt', label: t('cf_op_lt', { defaultValue: 'inférieure à' }) },
                          { value: 'ge', label: t('cf_op_ge', { defaultValue: 'supérieure ou égale à' }) },
                          { value: 'le', label: t('cf_op_le', { defaultValue: 'inférieure ou égale à' }) },
                          { value: 'eq', label: t('cf_op_eq', { defaultValue: 'égale à' }) },
                          { value: 'ne', label: t('cf_op_ne', { defaultValue: 'différente de' }) },
                          { value: 'between', label: t('cf_op_between', { defaultValue: 'comprise entre' }) }]} />
              <input className={`${sel} flex-1`} placeholder={t('cf_value', { defaultValue: 'valeur' })} value={v1} onChange={e => setV1(e.target.value)} />
              {cellOp === 'between' && <>
                <span className="text-text-secondary">{t('cf_and', { defaultValue: 'et' })}</span>
                <input className={`${sel} flex-1`} placeholder={t('cf_value', { defaultValue: 'valeur' })} value={v2} onChange={e => setV2(e.target.value)} />
              </>}
            </div>
          )}
          {kind === 'text' && (
            <div className="flex items-center gap-2">
              <Dropdown width={180} value={textOp} onChange={v => setTextOp(v as TextOp)}
                options={[{ value: 'contains', label: t('cf_t_contains', { defaultValue: 'contient' }) },
                          { value: 'ncontains', label: t('cf_t_ncontains', { defaultValue: 'ne contient pas' }) },
                          { value: 'starts', label: t('cf_t_starts', { defaultValue: 'commence par' }) },
                          { value: 'ends', label: t('cf_t_ends', { defaultValue: 'se termine par' }) }]} />
              <input className={`${sel} flex-1`} placeholder={t('cf_text', { defaultValue: 'texte' })} value={v1} onChange={e => setV1(e.target.value)} />
            </div>
          )}
          {kind === 'empty' && (
            <Dropdown className="w-full" value={emptyMode} onChange={v => setEmptyMode(v as 'empty' | 'notempty')}
              options={[{ value: 'empty', label: t('cf_is_empty', { defaultValue: 'est vide' }) },
                        { value: 'notempty', label: t('cf_is_notempty', { defaultValue: 'n’est pas vide' }) }]} />
          )}
          {kind === 'formula' && (
            <input className={`${sel} w-full font-mono`} placeholder="=$A1>MOYENNE($A:$A)" value={formula} onChange={e => setFormula(e.target.value)} />
          )}
          {kind === 'top' && (
            <div className="flex items-center gap-2">
              <Dropdown width={140} value={topSide} onChange={v => setTopSide(v as 'top' | 'bottom')}
                options={[{ value: 'top', label: t('cf_top', { defaultValue: 'Premiers' }) },
                          { value: 'bottom', label: t('cf_bottom', { defaultValue: 'Derniers' }) }]} />
              <input className={`${sel} w-20`} type="number" min={1} value={topRank} onChange={e => setTopRank(e.target.value)} />
              <Dropdown width={120} value={topUnit} onChange={v => setTopUnit(v as 'values' | 'percent')}
                options={[{ value: 'values', label: t('cf_unit_values', { defaultValue: 'valeurs' }) },
                          { value: 'percent', label: '%' }]} />
            </div>
          )}
          {kind === 'avg' && (
            <Dropdown className="w-full" value={avgMode} onChange={v => setAvgMode(v as 'above' | 'below')}
              options={[{ value: 'above', label: t('cf_above_avg', { defaultValue: 'Au-dessus de la moyenne' }) },
                        { value: 'below', label: t('cf_below_avg', { defaultValue: 'Sous la moyenne' }) }]} />
          )}
          {kind === 'dupes' && (
            <Dropdown className="w-full" value={dupMode} onChange={v => setDupMode(v as 'dup' | 'uniq')}
              options={[{ value: 'dup', label: t('cf_dup', { defaultValue: 'Valeurs en double' }) },
                        { value: 'uniq', label: t('cf_uniq', { defaultValue: 'Valeurs uniques' }) }]} />
          )}
          {kind === 'bar' && (
            <div className="flex items-center gap-3 text-xs text-text-secondary">
              <span>{t('cf_bar_color', { defaultValue: 'Couleur de la barre' })}</span>
              <div className="flex gap-1.5">
                {BAR_COLORS.map(c => (
                  <button key={c} onClick={() => setBarColor(c)}
                    className={`h-6 w-9 rounded border ${barColor === c ? 'border-primary ring-1 ring-primary' : 'border-border'}`}
                    style={{ background: `linear-gradient(90deg, ${c} 70%, ${c}55)` }} />
                ))}
              </div>
              <ColorField width={24} height={18} color={barColor} onChange={setBarColor} />
            </div>
          )}
          {kind === 'icons' && (
            <div className="flex items-center gap-3 text-xs text-text-secondary">
              <span>{t('cf_icon_set', { defaultValue: 'Jeu d’icônes' })}</span>
              <Dropdown width={200} value={iconSet} onChange={setIconSet}
                options={[{ value: '3Arrows', label: `3 ${t('cf_set_arrows', { defaultValue: 'flèches' })}` },
                          { value: '3TrafficLights1', label: `3 ${t('cf_set_lights', { defaultValue: 'feux' })}` },
                          { value: '3Symbols2', label: `3 ${t('cf_set_symbols', { defaultValue: 'symboles' })}` },
                          { value: '3Flags', label: `3 ${t('cf_set_flags', { defaultValue: 'drapeaux' })}` },
                          { value: '4Arrows', label: `4 ${t('cf_set_arrows', { defaultValue: 'flèches' })}` },
                          { value: '4Rating', label: `4 ${t('cf_set_rating', { defaultValue: 'niveaux' })}` },
                          { value: '5Arrows', label: `5 ${t('cf_set_arrows', { defaultValue: 'flèches' })}` },
                          { value: '5Rating', label: `5 ${t('cf_set_rating', { defaultValue: 'niveaux' })}` },
                          { value: '5Quarters', label: `5 ${t('cf_set_quarters', { defaultValue: 'quartiers' })}` }]} />
            </div>
          )}

          {/* Format / scale */}
          {kind === 'scale' && (
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
          )}
          {showDxf && (
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
          <Button className={DLG_BTN} variant="primary" onClick={addRule}><Plus size={14} /> {t('cf_add', { defaultValue: 'Ajouter la règle' })}</Button>
          <Button className={DLG_BTN} variant="ghost" onClick={onClose}>{t('cf_close', { defaultValue: 'Fermer' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
