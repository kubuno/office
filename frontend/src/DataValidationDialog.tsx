import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input } from '@ui'
import { ListChecks, Trash2, Plus } from 'lucide-react'
import type { DVBlock, DVCriterion, NumOp } from './data-validation'
import { criterionLabel } from './data-validation'

interface Props {
  blocks:       DVBlock[]
  selectionRef: string
  onApply:      (blocks: DVBlock[]) => void
  onClose:      () => void
}
type Kind = 'list' | 'listRange' | 'number' | 'textLen' | 'checkbox'

export default function DataValidationDialog({ blocks, selectionRef, onApply, onClose }: Props) {
  const { t } = useTranslation('office')
  const [range, setRange]   = useState(selectionRef || 'A1')
  const [kind, setKind]     = useState<Kind>('list')
  const [listText, setListText] = useState('')        // valeurs séparées par des virgules
  const [source, setSource] = useState('')            // plage source pour listRange
  const [op, setOp]         = useState<NumOp>('between')
  const [v1, setV1]         = useState('')
  const [v2, setV2]         = useState('')
  const [strict, setStrict] = useState(true)          // n'autoriser que les valeurs de la liste
  const [dropdown, setDropdown] = useState(true)      // afficher le menu déroulant dans la cellule
  const [reject, setReject] = useState(true)          // refuser (vs avertir)
  const [help, setHelp]     = useState('')

  const buildCrit = (): DVCriterion | null => {
    if (kind === 'checkbox') return { kind: 'checkbox' }
    if (kind === 'list') {
      const values = listText.split(',').map(s => s.trim()).filter(Boolean)
      if (!values.length) return null
      return { kind: 'list', values, strict, dropdown }
    }
    if (kind === 'listRange') {
      if (!source.trim()) return null
      return { kind: 'listRange', source: source.trim().toUpperCase(), strict, dropdown }
    }
    const a = Number(v1)
    if (v1.trim() === '' || isNaN(a)) return null
    const b = v2.trim() !== '' ? Number(v2) : undefined
    if ((op === 'between' || op === 'notBetween') && (b == null || isNaN(b))) return null
    return kind === 'number' ? { kind: 'number', op, v1: a, v2: b } : { kind: 'textLen', op, v1: a, v2: b }
  }

  const add = () => {
    const crit = buildCrit(); if (!crit || !range.trim()) return
    onApply([{ ranges: [range.trim().toUpperCase()], rule: { crit, reject: kind === 'checkbox' ? false : reject, help: help.trim() || undefined } }, ...blocks])
  }
  const removeBlock = (i: number) => onApply(blocks.filter((_, idx) => idx !== i))

  const sel = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary'
  const needsRange = kind === 'number' || kind === 'textLen'

  return (
    <FloatingWindow
      title={t('dv_title', { defaultValue: 'Validation des données' })}
      icon={<ListChecks size={16} />}
      onClose={onClose} backdrop resizable
      defaultWidth={520} defaultHeight={540} minWidth={440} minHeight={400}
    >
      <div className="flex flex-col h-full text-sm" data-module="office">
        <div className="border border-border rounded overflow-auto" style={{ maxHeight: 130 }}>
          {blocks.length === 0 ? (
            <div className="p-4 text-center text-text-tertiary text-xs">{t('dv_empty', { defaultValue: 'Aucune règle de validation.' })}</div>
          ) : blocks.map((b, i) => (
            <div key={i} className="flex items-center gap-2 px-2 py-1.5 border-b border-border last:border-0">
              <span className="text-xs text-text-secondary w-20 truncate" title={b.ranges.join(', ')}>{b.ranges.join(', ')}</span>
              <span className="text-xs flex-1 truncate" title={criterionLabel(b.rule.crit)}>{criterionLabel(b.rule.crit)}</span>
              <button className="p-1 rounded hover:bg-danger-light text-danger" onClick={() => removeBlock(i)}><Trash2 size={13} /></button>
            </div>
          ))}
        </div>

        <div className="mt-3 pt-3 border-t border-border space-y-2 overflow-auto flex-1">
          <div className="flex items-end gap-2">
            <div className="w-36"><Input label={t('dv_range', { defaultValue: 'Plage' })} value={range} onChange={e => setRange(e.target.value)} className="font-mono" /></div>
            <div className="flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t('dv_criteria', { defaultValue: 'Critères' })}</label>
              <select className={`${sel} w-full`} value={kind} onChange={e => setKind(e.target.value as Kind)}>
                <option value="list">{t('dv_k_list', { defaultValue: 'Liste d’éléments' })}</option>
                <option value="listRange">{t('dv_k_listrange', { defaultValue: 'Liste depuis une plage' })}</option>
                <option value="checkbox">{t('dv_k_checkbox', { defaultValue: 'Case à cocher' })}</option>
                <option value="number">{t('dv_k_number', { defaultValue: 'Nombre' })}</option>
                <option value="textLen">{t('dv_k_textlen', { defaultValue: 'Longueur du texte' })}</option>
              </select>
            </div>
          </div>

          {kind === 'list' && (
            <input className={`${sel} w-full`} placeholder={t('dv_list_ph', { defaultValue: 'Oui, Non, Peut-être' })} value={listText} onChange={e => setListText(e.target.value)} />
          )}
          {kind === 'listRange' && (
            <input className={`${sel} w-full font-mono`} placeholder="A1:A10" value={source} onChange={e => setSource(e.target.value)} />
          )}
          {needsRange && (
            <div className="flex items-center gap-2">
              <select className={sel} value={op} onChange={e => setOp(e.target.value as NumOp)}>
                <option value="between">{t('dv_op_between', { defaultValue: 'entre' })}</option>
                <option value="notBetween">{t('dv_op_nbetween', { defaultValue: 'hors de' })}</option>
                <option value="gt">{t('dv_op_gt', { defaultValue: 'supérieur à' })}</option>
                <option value="lt">{t('dv_op_lt', { defaultValue: 'inférieur à' })}</option>
                <option value="ge">{t('dv_op_ge', { defaultValue: '≥' })}</option>
                <option value="le">{t('dv_op_le', { defaultValue: '≤' })}</option>
                <option value="eq">{t('dv_op_eq', { defaultValue: '=' })}</option>
                <option value="ne">{t('dv_op_ne', { defaultValue: '≠' })}</option>
              </select>
              <input className={`${sel} flex-1`} placeholder={t('dv_value', { defaultValue: 'valeur' })} value={v1} onChange={e => setV1(e.target.value)} />
              {(op === 'between' || op === 'notBetween') && <>
                <span className="text-text-secondary">{t('dv_and', { defaultValue: 'et' })}</span>
                <input className={`${sel} flex-1`} value={v2} onChange={e => setV2(e.target.value)} />
              </>}
            </div>
          )}

          {(kind === 'list' || kind === 'listRange') && (
            <div className="flex flex-col gap-1.5 text-xs">
              <label className="flex items-center gap-2"><input type="checkbox" checked={dropdown} onChange={e => setDropdown(e.target.checked)} /> {t('dv_dropdown', { defaultValue: 'Afficher le menu déroulant dans la cellule' })}</label>
              <label className="flex items-center gap-2"><input type="checkbox" checked={strict} onChange={e => setStrict(e.target.checked)} /> {t('dv_strict', { defaultValue: 'Refuser les valeurs hors de la liste' })}</label>
            </div>
          )}
          {kind !== 'checkbox' && !(kind === 'list' || kind === 'listRange') && (
            <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={reject} onChange={e => setReject(e.target.checked)} /> {t('dv_reject', { defaultValue: 'Refuser la saisie invalide (sinon : avertir)' })}</label>
          )}
          {kind !== 'checkbox' && (
            <input className={`${sel} w-full`} placeholder={t('dv_help', { defaultValue: 'Texte d’aide (facultatif)' })} value={help} onChange={e => setHelp(e.target.value)} />
          )}
        </div>

        <div className="pt-2 mt-2 border-t border-border flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('dv_close', { defaultValue: 'Fermer' })}</Button>
          <Button variant="primary" onClick={add}><Plus size={14} /> {t('dv_add', { defaultValue: 'Ajouter' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
