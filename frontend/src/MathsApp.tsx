import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { Plus, Sigma, Trash2, ExternalLink, Copy } from 'lucide-react'
import katex from 'katex'
import 'katex/dist/katex.min.css'
import { Button, Dropdown } from '@ui'
import type { StartPageRecentItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { getDateLocale } from '@kubuno/sdk'
import { useDebouncedAutosave } from '@kubuno/sdk'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_MATHS } from './ribbon/officeThemes'
import { fileGroup } from './ribbon/common'
import { formulasApi, type MathFormula } from './maths-api'
import { MATH_CATEGORIES, CARET, type MathTemplate } from './mathSymbols'
import LatexEditor from './LatexEditor'
import { renderLatex } from './latexRender'

function renderTex(tex: string, displayMode: boolean): string {
  try {
    return katex.renderToString(tex, { displayMode, throwOnError: false, output: 'html', strict: false })
  } catch {
    return ''
  }
}

// ── Bouton de modèle dans la palette (aperçu KaTeX) ─────────────────────────────

function TemplateButton({ tpl, onInsert }: { tpl: MathTemplate; onInsert: (t: MathTemplate) => void }) {
  const html = useMemo(() => renderTex(tpl.tex, false), [tpl.tex])
  return (
    <button
      title={tpl.title ?? tpl.ins.replace(CARET, '')}
      onClick={() => onInsert(tpl)}
      className="flex items-center justify-center h-10 rounded border border-border bg-white
                 hover:bg-primary/5 hover:border-primary/40 transition-colors overflow-hidden px-1"
    >
      <span className="text-[#202124] scale-90" dangerouslySetInnerHTML={{ __html: html }} />
    </button>
  )
}

// ── Vue d'édition d'une formule ─────────────────────────────────────────────────

function FormulaEditorView({ formula, onUpdate }: { formula: MathFormula; onUpdate: (f: MathFormula) => void }) {
  const { t } = useTranslation('office')
  const [latex, setLatex] = useState(formula.latex ?? '')
  const [category, setCategory] = useState(MATH_CATEGORIES[0].id)
  const taRef = useRef<HTMLTextAreaElement>(null)

  useDebouncedAutosave(latex, true, (v) => {
    formulasApi.update(formula.id, { latex: v }).then(d => onUpdate(d.formula)).catch(() => {})
  })

  const cat = MATH_CATEGORIES.find(c => c.id === category) ?? MATH_CATEGORIES[0]

  const insert = useCallback((tpl: MathTemplate) => {
    const el = taRef.current
    const text = tpl.ins.replace(CARET, '')
    const caretRel = tpl.ins.indexOf(CARET)
    if (!el) { setLatex(l => l + text); return }
    const start = el.selectionStart, end = el.selectionEnd
    const cur = el.value
    const next = cur.slice(0, start) + text + cur.slice(end)
    setLatex(next)
    const caret = caretRel >= 0 ? start + caretRel : start + text.length
    requestAnimationFrame(() => { el.focus(); el.setSelectionRange(caret, caret) })
  }, [])

  const preview = renderLatex(latex)

  return (
    <div className="flex flex-1 min-w-0 min-h-0 bg-surface-1 overflow-hidden">
      {/* Palette de symboles */}
      <div className="flex-shrink-0 w-56 bg-white border-r border-border flex flex-col overflow-hidden">
        <div className="p-2 border-b border-border flex-shrink-0">
          <Dropdown
            className="w-full"
            value={category}
            onChange={setCategory}
            options={MATH_CATEGORIES.map(c => ({ value: c.id, label: t('math_cat_' + c.id, { defaultValue: c.label }) }))}
          />
        </div>
        <div className="flex-1 overflow-y-auto p-2">
          <div className="grid grid-cols-3 gap-1.5">
            {cat.items.map((tpl, i) => <TemplateButton key={i} tpl={tpl} onInsert={insert} />)}
          </div>
        </div>
      </div>

      {/* Aperçu + code */}
      <div className="flex-1 flex flex-col min-w-0 min-h-0">
        {/* Aperçu rendu : formule centrée (math pur) ou document mixte (texte + maths). */}
        <div className={`flex-1 min-h-0 overflow-auto bg-white p-8 ${preview.doc ? '' : 'flex items-center justify-center'}`}>
          {preview.html
            ? (preview.doc
                ? <div className="text-[#202124] text-[15px] leading-relaxed mx-auto max-w-[760px]" dangerouslySetInnerHTML={{ __html: preview.html }} />
                : <div className="text-[#202124] text-2xl" dangerouslySetInnerHTML={{ __html: preview.html }} />)
            : <div className="text-text-tertiary text-sm">{t('math_empty_preview', { defaultValue: 'Saisissez une formule LaTeX ou utilisez la palette' })}</div>}
        </div>

        {/* Barre + éditeur de code LaTeX (coloré) */}
        <div className="flex flex-col flex-shrink-0 h-[240px] border-t border-border">
          <div className="flex items-center gap-2 px-3 h-7 bg-surface-2 border-b border-border flex-shrink-0">
            <span className="text-[11px] font-medium text-text-secondary uppercase tracking-wide">{t('math_latex_code', { defaultValue: 'Code LaTeX' })}</span>
          </div>
          <LatexEditor value={latex} onChange={setLatex} taRef={taRef} />
        </div>
      </div>
    </div>
  )
}

// ── Application Maths ───────────────────────────────────────────────────────────

export default function MathsApp() {
  const { t, i18n } = useTranslation('office')
  const { id: routeId } = useParams<{ id: string }>()
  const [formulas, setFormulas]   = useState<MathFormula[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [loading, setLoading]     = useState(true)

  const load = useCallback(async () => {
    const data = await formulasApi.list()
    setFormulas(data.formulas)
    setLoading(false)
  }, [])
  useEffect(() => { load() }, [load])

  // `formulasApi.list()` ne renvoie PAS le LaTeX (le contenu vit dans le .kbmath, lu
  // seulement par get/open-by-file). On suit donc quelles formules ont leur contenu
  // chargé pour n'ouvrir l'éditeur qu'avec le vrai LaTeX (sinon on l'écraserait à vide).
  const [loadedIds, setLoadedIds] = useState<Set<string>>(() => new Set())
  const upsert      = (f: MathFormula) => setFormulas(prev => prev.some(x => x.id === f.id) ? prev.map(x => x.id === f.id ? f : x) : [f, ...prev])
  const markLoaded  = (id: string) => setLoadedIds(s => s.has(id) ? s : new Set(s).add(id))

  const selected = formulas.find(f => f.id === selectedId) ?? null

  // Charge le contenu complet de la formule sélectionnée (si pas déjà chargé).
  useEffect(() => {
    if (!selectedId || loadedIds.has(selectedId)) return
    let cancel = false
    formulasApi.get(selectedId).then(({ formula }) => {
      if (cancel) return
      upsert(formula); markLoaded(formula.id)
    }).catch(() => {})
    return () => { cancel = true }
  }, [selectedId, loadedIds]) // eslint-disable-line react-hooks/exhaustive-deps

  // Ouverture par URL (/office/maths/:id) — ex. double-clic d'un .kbmath dans files.
  useEffect(() => {
    if (!routeId) return
    formulasApi.get(routeId).then(({ formula }) => {
      upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
    }).catch(() => {})
  }, [routeId]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleUpdate = (u: MathFormula) => setFormulas(prev => prev.map(f => f.id === u.id ? { ...f, ...u } : f))

  async function handleNew() {
    const { formula } = await formulasApi.create({ name: t('math_new', { defaultValue: 'Nouvelle formule' }) })
    upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
  }
  async function handleTrash() {
    if (!selected) return
    await formulasApi.trash(selected.id)
    setFormulas(prev => prev.filter(f => f.id !== selected.id))
    setSelectedId(null)
  }
  async function handleDuplicate() {
    if (!selected) return
    const { formula } = await formulasApi.duplicate(selected.id)
    upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
  }

  const handleOpenFile = (file: FileItem): boolean => {
    formulasApi.openByFile(file.id).then(({ formula }) => {
      upsert(formula); markLoaded(formula.id); setSelectedId(formula.id)
    }).catch(() => {})
    return true
  }

  // Titre éditable (WorkspaceShell) — nom de la formule sélectionnée.
  const [titleDraft, setTitleDraft] = useState('')
  useEffect(() => { setTitleDraft(selected?.name ?? '') }, [selected?.name])
  const commitTitle = async () => {
    if (!selected) return
    const v = titleDraft.trim()
    if (v && v !== selected.name) {
      const data = await formulasApi.update(selected.id, { name: v })
      handleUpdate(data.formula)
    } else if (!v) {
      setTitleDraft(selected.name)
    }
  }

  if (loading) {
    return <div className="flex h-full items-center justify-center text-text-tertiary">{t('common_loading', { defaultValue: 'Chargement…' })}</div>
  }

  // Accueil (aucune formule ouverte).
  if (!selected) {
    const recentItems: StartPageRecentItem[] = formulas.slice(0, 12).map(f => ({
      id:       f.id,
      name:     f.name,
      subtitle: format(new Date(f.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
      icon:     <Sigma size={18} className="text-text-tertiary" strokeWidth={1.5} />,
      onClick:  () => setSelectedId(f.id),
      actions: [
        { id: 'open',  label: t('common_open', { defaultValue: 'Ouvrir' }), icon: <ExternalLink size={15} />, onClick: () => setSelectedId(f.id) },
        { id: 'dup',   label: t('common_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={15} />, onClick: () => { formulasApi.duplicate(f.id).then(d => { upsert(d.formula); markLoaded(d.formula.id); setSelectedId(d.formula.id) }) } },
        { id: 'trash', label: t('math_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => { formulasApi.trash(f.id).then(() => setFormulas(prev => prev.filter(x => x.id !== f.id))) } },
      ],
    }))
    return (
      <ModuleStartPage
        recentTitle={t('math_recent', { defaultValue: 'Récents' })}
        recentItems={recentItems}
        recentEmpty={
          <div className="flex flex-col items-center gap-2">
            <Sigma size={32} className="text-text-tertiary opacity-30" strokeWidth={1.5} />
            <p className="text-text-tertiary text-xs">{t('math_select_or_create', { defaultValue: 'Créez une formule ou ouvrez-en une existante' })}</p>
          </div>
        }
        browse={{
          folderPathPrefix: 'Office/Maths',
          title: t('math_title', { defaultValue: 'Maths' }),
          fileTypeModuleId: 'office-maths',
          onOpenFile: handleOpenFile,
          toolbarContent: (
            <Button icon={<Plus size={15} />} onClick={handleNew}>
              {t('math_new', { defaultValue: 'Nouvelle formule' })}
            </Button>
          ),
        }}
      />
    )
  }

  return (
    <OfficeShell
      ribbon={[{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
        groups: [fileGroup(t, { onNew: handleNew, onDuplicate: handleDuplicate })] }]}
      theme={THEME_MATHS}
      chromeless
      topbarHeight={64}
      titleIcon={<Sigma size={16} className="text-white/90 flex-shrink-0" />}
      title={titleDraft}
      onBack={() => setSelectedId(null)}
      onTitleChange={setTitleDraft}
      onTitleCommit={commitTitle}
      titlePlaceholder={t('common_untitled', { defaultValue: 'Sans titre' })}
      onDelete={handleTrash}
      deleteTitle={t('math_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('math_delete_confirm_title', { defaultValue: 'Supprimer cette formule ?' }),
        message: t('math_delete_confirm_msg', { defaultValue: 'La formule sera déplacée dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      }}
    >
      {loadedIds.has(selected.id)
        ? <FormulaEditorView key={selected.id} formula={selected} onUpdate={handleUpdate} />
        : <div className="flex flex-1 items-center justify-center text-text-tertiary text-sm">{t('common_loading', { defaultValue: 'Chargement…' })}</div>}
    </OfficeShell>
  )
}
