import { useState, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button } from '@ui'
import { FunctionSquare, Search } from 'lucide-react'
import { FUNCTION_CATALOG, CAT_LABEL, CAT_COLOR, type FnCat } from './formula-catalog'

interface Props {
  onInsert: (name: string) => void
  onClose:  () => void
}

// Navigateur de fonctions (façon « Insérer une fonction » d'Excel) : recherche +
// filtre par catégorie sur les ~270 fonctions, avec syntaxe.
export default function FunctionBrowserDialog({ onInsert, onClose }: Props) {
  const { t } = useTranslation('office')
  const [q, setQ] = useState('')
  const [cat, setCat] = useState<FnCat | 'all'>('all')
  const [sel, setSel] = useState<string | null>(null)

  const cats = useMemo(() => [...new Set(FUNCTION_CATALOG.map(f => f.cat))], [])
  const list = useMemo(() => {
    const qq = q.trim().toUpperCase()
    return FUNCTION_CATALOG.filter(f => (cat === 'all' || f.cat === cat) && (!qq || f.name.includes(qq)))
  }, [q, cat])
  const current = list.find(f => f.name === sel) ?? list[0]

  const selStyle = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary'

  return (
    <FloatingWindow
      title={t('fnb_title', { defaultValue: 'Insérer une fonction' })}
      icon={<FunctionSquare size={16} />}
      onClose={onClose} backdrop resizable
      defaultWidth={560} defaultHeight={480} minWidth={460} minHeight={360}
    >
      <div className="flex flex-col h-full text-sm" data-module="office">
        <div className="flex items-center gap-2 mb-2">
          <div className="relative flex-1">
            <Search size={14} className="absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary" />
            <input autoFocus value={q} onChange={e => setQ(e.target.value)} placeholder={t('fnb_search', { defaultValue: 'Rechercher une fonction…' })}
              className={`${selStyle} w-full`} style={{ paddingLeft: 26 }} />
          </div>
          <select className={selStyle} value={cat} onChange={e => setCat(e.target.value as FnCat | 'all')}>
            <option value="all">{t('fnb_all', { defaultValue: 'Toutes' })}</option>
            {cats.map(c => <option key={c} value={c}>{CAT_LABEL[c]}</option>)}
          </select>
        </div>

        <div className="flex-1 overflow-auto border border-border rounded">
          {list.length === 0 ? (
            <div className="p-4 text-center text-text-tertiary text-xs">{t('fnb_none', { defaultValue: 'Aucune fonction' })}</div>
          ) : list.map(f => (
            <button key={f.name}
              onClick={() => setSel(f.name)}
              onDoubleClick={() => onInsert(f.name)}
              className={`w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-surface-1 ${current?.name === f.name ? 'bg-primary-light' : ''}`}>
              <span className="w-1.5 h-1.5 rounded-full flex-shrink-0" style={{ background: CAT_COLOR[f.cat] }} />
              <span className="font-mono font-semibold" style={{ color: CAT_COLOR[f.cat] }}>{f.name}</span>
              <span className="text-xs text-text-tertiary truncate ml-auto">{f.syntax}</span>
            </button>
          ))}
        </div>

        {current && (
          <div className="mt-2 p-2 bg-surface-1 rounded text-xs">
            <div className="font-mono text-text-secondary">{current.syntax}</div>
            <div className="text-text-tertiary mt-1">{CAT_LABEL[current.cat]}</div>
          </div>
        )}

        <div className="pt-2 mt-2 border-t border-border flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>{t('fnb_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" disabled={!current} onClick={() => current && onInsert(current.name)}>{t('fnb_insert', { defaultValue: 'Insérer' })}</Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
