// « Insérer une fonction » — modelled on LibreOffice's Assistant Fonction rather
// than on a plain picker, because the hard part is never finding the NAME: it is
// knowing what the arguments are and whether the formula does what you meant.
//
// Hence, next to the search: a category TREE (find by topic, not by spelling), one
// FIELD PER ARGUMENT with its live value, the assembled formula in an editable box
// and its RESULT computed against the real sheet — so the dialog is validated
// before it is closed, not after. The Structure tab breaks the formula down the
// way Calc's second tab does.

import { useState, useMemo, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { DLG_BTN } from './lib'
import { FloatingWindow, Button, Dropdown, Checkbox } from '@ui'
import { FunctionSquare, Search, Star } from 'lucide-react'
import { CAT_LABEL, CAT_COLOR, type FnCat } from './formula-catalog'
import {
  buildWizardCatalog, groupByCategory, matches, rankByRelevance, composeFormula,
  type WizardFn,
} from './fn-wizard/catalog'
import { FunctionTree } from './fn-wizard/FunctionTree'
import { ArgumentFields } from './fn-wizard/ArgumentFields'

interface Props {
  /** Receives the finished formula body, e.g. `SUM(A1:A10)` or just `SUM(`. */
  onInsert: (text: string) => void
  onClose: () => void
  /** Descriptions of the documented functions (name → i18n key). */
  descriptions?: Map<string, string>
  /** Evaluates a formula body against the active sheet, for the live result. */
  evaluate?: (body: string) => string
  /** Favourites, persisted by the host (user preferences). */
  favourites?: string[]
  onToggleFavourite?: (name: string) => void
}

export default function FunctionBrowserDialog({
  onInsert, onClose, descriptions, evaluate, favourites, onToggleFavourite,
}: Props) {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<'functions' | 'structure'>('functions')
  const [q, setQ] = useState('')
  const [similar, setSimilar] = useState(false)
  const [cat, setCat] = useState<FnCat | 'all'>('all')
  const [sel, setSel] = useState<string | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [argv, setArgv] = useState<string[]>([])
  const [manual, setManual] = useState<string | null>(null) // formula edited by hand

  const catalog = useMemo(() => buildWizardCatalog(descriptions ?? new Map()), [descriptions])
  const cats = useMemo(() => [...new Set(catalog.map(f => f.cat))], [catalog])

  const filtered = useMemo(() => {
    const hits = catalog.filter(f => (cat === 'all' || f.cat === cat) && matches(f, q, similar))
    return rankByRelevance(hits, q)
  }, [catalog, cat, q, similar])
  const groups = useMemo(() => groupByCategory(filtered), [filtered])

  // Searching opens every group holding a hit: the point of the search is to SEE
  // the matches, not to hunt for them behind collapsed headers.
  useEffect(() => {
    if (q.trim()) setExpanded(new Set(groups.map(g => g.cat)))
  }, [q, groups])

  const current = useMemo(
    () => filtered.find(f => f.name === sel) ?? filtered[0] ?? null,
    [filtered, sel],
  )
  // Switching function starts a fresh argument list.
  useEffect(() => { setArgv([]); setManual(null) }, [current?.name])

  const built = current ? composeFormula(current, argv) : ''
  const formula = manual ?? built
  const result = evaluate && formula ? evaluate(formula) : ''
  const isFav = !!(current && favourites?.includes(current.name))

  const field = 'h-8 px-2 border border-border rounded bg-surface-0 outline-none focus:border-primary'
  const tabCls = (on: boolean) =>
    `px-3 h-7 rounded-t border-b-2 ${on ? 'border-primary text-primary' : 'border-transparent text-text-secondary hover:text-text-primary'}`

  const confirm = (fn?: WizardFn | null) => {
    const f = fn ?? current
    if (!f) return
    // A formula with arguments goes in whole; a bare pick keeps the old behaviour
    // (open parenthesis, caret inside) so the user can keep typing in the cell.
    onInsert(manual ?? (argv.some(v => v.trim()) ? built : `${f.name}(`))
  }

  return (
    <FloatingWindow
      title={t('fnb_title', { defaultValue: 'Insérer une fonction' })}
      icon={<FunctionSquare size={16} />}
      onClose={onClose} backdrop resizable
      defaultWidth={860} defaultHeight={560} minWidth={640} minHeight={420}
    >
      <div className="flex flex-col h-full" data-module="office">
        <div className="flex items-center gap-1 border-b border-border mb-2">
          <button type="button" className={tabCls(tab === 'functions')} onClick={() => setTab('functions')}>
            {t('fnb_tab_functions', { defaultValue: 'Fonctions' })}
          </button>
          <button type="button" className={tabCls(tab === 'structure')} onClick={() => setTab('structure')}>
            {t('fnb_tab_structure', { defaultValue: 'Structure' })}
          </button>
        </div>

        <div className="flex-1 min-h-0 flex gap-3">
          {/* Left: search, category, tree */}
          <div className="w-64 flex-shrink-0 flex flex-col gap-2 min-h-0">
            <div className="relative">
              <Search size={14} className="absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary" />
              <input
                autoFocus value={q} onChange={e => setQ(e.target.value)}
                placeholder={t('fnb_search', { defaultValue: 'Rechercher une fonction…' })}
                className={`${field} w-full`} style={{ paddingLeft: 26 }}
              />
            </div>
            <Checkbox
              checked={similar} onChange={setSimilar}
              label={t('fnb_similar', { defaultValue: 'Similaire' })}
            />
            <Dropdown
              value={cat} onChange={v => setCat(v as FnCat | 'all')}
              options={[{ value: 'all', label: t('fnb_all', { defaultValue: 'Toutes' }) },
                        ...cats.map(c => ({ value: c, label: CAT_LABEL[c] }))]}
            />
            <div className="flex-1 min-h-0 overflow-auto border border-border rounded">
              <FunctionTree
                groups={groups}
                expanded={expanded}
                onToggle={c => setExpanded(prev => {
                  const next = new Set(prev); next.has(c) ? next.delete(c) : next.add(c); return next
                })}
                selected={current?.name ?? null}
                onSelect={f => setSel(f.name)}
                onConfirm={f => { setSel(f.name); confirm(f) }}
                emptyLabel={t('fnb_none', { defaultValue: 'Aucune fonction' })}
              />
            </div>
          </div>

          {/* Right: description + arguments, or the structure breakdown */}
          <div className="flex-1 min-w-0 flex flex-col gap-2 min-h-0">
            {tab === 'functions' ? (
              <>
                {current ? (
                  <>
                    <div className="flex items-center gap-2">
                      {onToggleFavourite && (
                        <button
                          type="button"
                          onClick={() => onToggleFavourite(current.name)}
                          title={t('fnb_favourite', { defaultValue: 'Favori' })}
                          className="text-text-tertiary hover:text-warning"
                        >
                          <Star size={15} fill={isFav ? 'currentColor' : 'none'} className={isFav ? 'text-warning' : ''} />
                        </button>
                      )}
                      <span className="font-mono font-semibold" style={{ color: CAT_COLOR[current.cat] }}>{current.name}</span>
                      <span className="text-text-tertiary">{CAT_LABEL[current.cat]}</span>
                    </div>
                    <div className="font-mono text-text-secondary">{current.syntax}</div>
                    {current.descKey && (
                      <div className="text-text-secondary">{t(current.descKey, { defaultValue: '' })}</div>
                    )}
                    <div className="flex-1 min-h-0 overflow-auto pt-1">
                      <ArgumentFields
                        args={current.args}
                        values={argv}
                        onChange={(i, v) => { setManual(null); setArgv(prev => { const n = [...prev]; n[i] = v; return n }) }}
                        preview={evaluate}
                        optionalLabel={t('fnb_optional', { defaultValue: 'facultatif' })}
                        emptyLabel={t('fnb_no_args', { defaultValue: 'Cette fonction ne prend pas d’argument.' })}
                      />
                    </div>
                  </>
                ) : (
                  <div className="text-text-tertiary">{t('fnb_none', { defaultValue: 'Aucune fonction' })}</div>
                )}
              </>
            ) : (
              <div className="flex-1 min-h-0 overflow-auto">
                <div className="text-text-secondary mb-1">{t('fnb_structure_title', { defaultValue: 'Structure de la formule' })}</div>
                {current ? (
                  <ul className="font-mono">
                    <li style={{ color: CAT_COLOR[current.cat] }}>{current.name}</li>
                    {current.args.map((a, i) => (
                      <li key={i} className="pl-4 text-text-secondary">
                        {a.name}
                        {argv[i]?.trim() ? <span className="text-text-tertiary"> = {argv[i].trim()}</span> : null}
                      </li>
                    ))}
                  </ul>
                ) : <div className="text-text-tertiary">—</div>}
              </div>
            )}

            {/* Formula + live result — Calc's bottom strip */}
            <label className="flex items-center gap-2">
              <span className="w-16 flex-shrink-0 text-text-secondary">{t('fnb_formula', { defaultValue: 'Formule' })}</span>
              <input
                value={formula ? `=${formula}` : ''}
                onChange={e => setManual(e.target.value.replace(/^=/, ''))}
                className={`${field} flex-1 min-w-0 font-mono`}
              />
            </label>
            <div className="flex items-center gap-2">
              <span className="w-16 flex-shrink-0 text-text-secondary">{t('fnb_result', { defaultValue: 'Résultat' })}</span>
              <span className="flex-1 min-w-0 h-8 px-2 flex items-center border border-border rounded bg-surface-1 font-mono truncate" title={result}>
                {result}
              </span>
            </div>
          </div>
        </div>

        <div className="pt-2 mt-2 border-t border-border flex justify-end gap-2">
          <Button className={DLG_BTN} variant="primary" disabled={!current} onClick={() => confirm()}>
            {t('fnb_insert', { defaultValue: 'Insérer' })}
          </Button>
          <Button className={DLG_BTN} variant="ghost" onClick={onClose}>
            {t('fnb_cancel', { defaultValue: 'Annuler' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
