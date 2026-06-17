import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { MenuDropdown, useMenuDropdown, type MenuItem } from '@ui'
import { Zap, Play, Plus, Pencil, Trash2, X } from 'lucide-react'
import { macrosApi, scriptsApi, docMacrosApi, type DocMacro } from '../script-api'
import { runMacro, type MacroResult } from './runtime'
import { MacroEditorWindow } from './MacroEditorWindow'
import { FormHost, DialogHost, makeFormsApi } from './FormRuntime'

// Bouton « Macros » réutilisable, à déposer dans le ruban/barre d'outils de chaque
// éditeur Office. Exécute les macros CÔTÉ CLIENT contre l'API vivante (`buildApi`).
//
// STOCKAGE : pour les types « container-bound » (INDOC_TYPES), les macros sont rangées
// DANS la donnée du document (docMacrosApi) → elles voyagent avec le fichier ; édition
// via l'éditeur Script en doc-mode. Les autres modules utilisent encore l'ancienne
// attache macrosApi (table externe), le temps que leur stockage in-document arrive.
const INDOC_TYPES = new Set(['spreadsheet'])

// Modèles de départ par type de document (API exposée = global `Kubuno`).
const TEMPLATES: Record<string, string> = {
  spreadsheet: `// Macro tableur — l'API « Kubuno.Sheet » agit sur la feuille active.
const sel = Kubuno.Sheet.getSelection()
console.log('Sélection :', sel ? sel.from + ':' + sel.to : '(aucune)')
// Exemple : écrire une valeur et la mettre en gras.
Kubuno.Sheet.setValue('A1', 'Bonjour depuis une macro !')
Kubuno.Sheet.setBold('A1', true)
`,
  document: `// Macro document — « Kubuno.Doc » agit sur le document ouvert.
console.log('Mots :', Kubuno.Doc.getWordCount())
Kubuno.Doc.insertText(' [inséré par une macro]')
`,
  presentation: `// Macro présentation (lecture) — « Kubuno.Slides ».
console.log('Diapositives :', Kubuno.Slides.count())
`,
  diagram: `// Macro diagramme (lecture) — « Kubuno.Diagram ».
console.log('Formes :', Kubuno.Diagram.getShapeCount(), '· Connecteurs :', Kubuno.Diagram.getConnectorCount())
`,
  math: `// Macro maths — « Kubuno.Math ».
console.log('LaTeX actuel :', Kubuno.Math.getLatex())
`,
  whiteboard: `// Macro tableau blanc (lecture) — « Kubuno.Board ».
console.log('Objets :', Kubuno.Board.getObjectCount())
`,
  project: `// Macro projet (lecture) — « Kubuno.Project ».
console.log('Tâches :', Kubuno.Project.getTaskCount())
`,
  data: `// Macro data (lecture) — « Kubuno.Data ».
console.log('Rapport :', Kubuno.Data.getReportName())
`,
  default: `// Première macro — « Kubuno.App » offre les utilitaires de base.
Kubuno.App.toast('Macro exécutée !')
console.log('Document :', Kubuno.App.getType(), Kubuno.App.getId())
`,
}

interface MacrosMenuProps {
  docType: string
  docId: string
  /** Construit l'objet `Kubuno` (API du document vivant) au moment de l'exécution. */
  buildApi: () => unknown
  /** Libellé par défaut des nouvelles macros (ex. nom du document). */
  defaultLabel?: string
}

// Forme normalisée pour le rendu (que la macro vienne du document ou de l'ancienne table).
interface UiMacro { key: string; label: string; getSource: () => Promise<string>; edit: () => void; remove: () => void }

export function MacrosMenu({ docType, docId, buildApi, defaultLabel }: MacrosMenuProps) {
  const { t } = useTranslation('office')
  const navigate = useNavigate()
  const qc = useQueryClient()
  const menu = useMenuDropdown()
  const [result, setResult] = useState<{ name: string; res: MacroResult } | null>(null)
  const [busy, setBusy] = useState(false)
  // Macro en cours d'édition → fenêtre volante sombre (in-document uniquement).
  const [editing, setEditing] = useState<string | null>(null)
  const inDoc = INDOC_TYPES.has(docType)
  const key = ['doc-macros', docType, docId, inDoc] as const
  const invalidate = () => qc.invalidateQueries({ queryKey: ['doc-macros', docType, docId] })

  const macrosQuery = useQuery<unknown>({
    queryKey: key,
    queryFn: () => inDoc
      ? docMacrosApi.list(docType, docId)
      : macrosApi.listForDocument(docType, docId).then(r => r.macros),
    enabled: !!docId,
  })

  // Normalise les deux sources de stockage vers une liste rendable.
  const allInDoc = (macrosQuery.data as DocMacro[] | undefined) ?? []
  const macros: UiMacro[] = inDoc
    ? allInDoc.filter(m => m.kind !== 'form').map(m => ({   // les formulaires ne s'exécutent pas seuls
        key: m.id,
        label: m.name,
        getSource: async () => m.source,
        edit: () => setEditing(m.id),   // fenêtre volante au-dessus du document vivant
        remove: () => { void docMacrosApi.save(docType, docId, ((macrosQuery.data as DocMacro[]).filter(x => x.id !== m.id))).then(invalidate) },
      }))
    : ((macrosQuery.data as { id: string; script_id: string; button_label: string }[] | undefined) ?? []).map(m => ({
        key: m.id,
        label: m.button_label,
        getSource: async () => (await scriptsApi.get(m.script_id)).script.source_code,
        edit: () => navigate(`/office/script/${m.script_id}`),
        remove: () => { void macrosApi.delete(m.id).then(invalidate) },
      }))

  const run = async (m: UiMacro) => {
    setBusy(true)
    try {
      const source = await m.getSource()
      const api = buildApi() as Record<string, unknown>
      if (inDoc) api.Forms = makeFormsApi(() => allInDoc, () => api)   // formulaires accessibles depuis la macro
      const res = await runMacro(source, api, Date.now())
      setResult({ name: m.label, res })
    } catch (e) {
      setResult({ name: m.label, res: { ok: false, logs: [], error: e instanceof Error ? e.message : String(e), durationMs: 0 } })
    } finally { setBusy(false) }
  }

  const createMut = useMutation({
    mutationFn: async () => {
      const tmpl = TEMPLATES[docType] ?? TEMPLATES.default
      const label = t('macro_new_label', { defaultValue: 'Ma macro' })
      if (inDoc) {
        const cur = (macrosQuery.data as DocMacro[] | undefined) ?? []
        const id: string = crypto.randomUUID()
        await docMacrosApi.save(docType, docId, [...cur, { id, name: label, source: tmpl }])
        return { kind: 'edit', id } as const
      }
      const { script } = await scriptsApi.create({ name: `Macro ${defaultLabel ?? docType}`, source_code: tmpl })
      await macrosApi.create({ script_id: script.id, document_type: docType, document_id: docId, button_label: label })
      return { kind: 'nav', route: `/office/script/${script.id}` } as const
    },
    onSuccess: (r) => { invalidate(); if (r.kind === 'edit') setEditing(r.id); else navigate(r.route) },
  })

  const items: MenuItem[] = []
  if (macros.length) {
    items.push({ type: 'label', text: t('macros_run', { defaultValue: 'Exécuter' }) })
    for (const m of macros) items.push({ type: 'action', label: m.label, icon: <Play size={14} />, onClick: () => { menu.close(); run(m) } })
    items.push({ type: 'separator' })
  }
  items.push({ type: 'action', label: t('macro_new', { defaultValue: 'Nouvelle macro…' }), icon: <Plus size={14} />, onClick: () => { menu.close(); createMut.mutate() } })
  if (macros.length) {
    items.push({ type: 'submenu', label: t('macro_edit', { defaultValue: 'Modifier…' }), icon: <Pencil size={14} />,
      items: macros.map(m => ({ type: 'action' as const, label: m.label, onClick: m.edit })) })
    items.push({ type: 'submenu', label: t('macro_delete', { defaultValue: 'Supprimer…' }), icon: <Trash2 size={14} />,
      items: macros.map(m => ({ type: 'action' as const, label: m.label, danger: true, onClick: m.remove })) })
  }

  return (
    <>
      <button type="button" onClick={menu.open} disabled={busy}
        title={t('macros', { defaultValue: 'Macros' })}
        className="flex items-center gap-1 h-7 px-2 rounded text-text-secondary hover:bg-black/5 text-sm whitespace-nowrap disabled:opacity-50">
        <Zap size={15} /> <span className="hidden sm:inline">{t('macros', { defaultValue: 'Macros' })}</span>
      </button>
      {menu.isOpen && menu.pos && <MenuDropdown items={items} pos={{ ...menu.pos, minWidth: 220 }} onClose={menu.close} />}
      {result && <MacroResultCard name={result.name} res={result.res} onClose={() => setResult(null)} t={t} />}
      {editing && (
        <MacroEditorWindow docType={docType} docId={docId} macroId={editing} buildApi={buildApi}
          onClose={() => { setEditing(null); invalidate() }} />
      )}
      {/* Hôtes runtime : formulaires (Kubuno.Forms.show) + dialogues (Kubuno.App.alert/confirm/prompt). */}
      {inDoc && <FormHost />}
      <DialogHost />
    </>
  )
}

function MacroResultCard({ name, res, onClose, t }: { name: string; res: MacroResult; onClose: () => void; t: (k: string, o?: Record<string, unknown>) => string }) {
  return (
    <div className="fixed bottom-4 right-4 z-[9998] w-80 max-h-80 flex flex-col rounded-lg bg-white shadow-xl border border-[#dadce0] text-sm overflow-hidden">
      <div className={`flex items-center gap-2 px-3 py-2 text-white ${res.ok ? 'bg-[#1e8e3e]' : 'bg-[#d93025]'}`}>
        <Zap size={14} />
        <span className="flex-1 font-medium truncate">{name}</span>
        <span className="text-xs opacity-90">{res.durationMs} ms</span>
        <button onClick={onClose} className="p-0.5 rounded hover:bg-white/20"><X size={14} /></button>
      </div>
      <div className="flex-1 overflow-auto p-2 font-mono text-xs leading-relaxed">
        {res.logs.map((l, i) => (
          <div key={i} className={l.level === 'error' ? 'text-[#d93025]' : l.level === 'warn' ? 'text-[#e8710a]' : 'text-text-primary'}>{l.text}</div>
        ))}
        {res.error && <pre className="text-[#d93025] whitespace-pre-wrap mt-1">{res.error}</pre>}
        {res.ok && res.returnValue !== undefined && (
          <div className="text-text-secondary mt-1">⟼ {typeof res.returnValue === 'string' ? res.returnValue : JSON.stringify(res.returnValue)}</div>
        )}
        {res.ok && res.logs.length === 0 && res.returnValue === undefined && (
          <div className="text-text-tertiary">{t('macro_done', { defaultValue: 'Terminé (aucune sortie).' })}</div>
        )}
      </div>
    </div>
  )
}
