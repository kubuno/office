import './../monacoSetup'
import { useState, useEffect, useRef, useCallback } from 'react'
import { createPortal } from 'react-dom'
import Editor from '@monaco-editor/react'
import { useTranslation } from 'react-i18next'
import { MenuDropdown, type MenuItem } from '@ui'
import { Zap, Play, Save, Check, Minus, X, Maximize2, Settings, FileCode, AppWindow, Plus, ChevronDown, Trash2, Terminal, Copy } from 'lucide-react'
import { docMacrosApi, getApiTypes, type DocMacro, type FormControl } from '../script-api'
import { runMacro, type MacroResult } from './runtime'
import { FormDesigner } from './FormDesigner'
import { makeFormsApi } from './FormRuntime'
import { ensureJetBrainsMono, JETBRAINS_MONO } from './jetbrainsMono'

// Mini-IDE de macros façon VBE (Visual Basic Editor) en FENÊTRE VOLANTE SOMBRE au
// dessus du document vivant : ARBORESCENCE de projet à droite (Modules + Formulaires),
// éditeur de code Monaco OU concepteur de formulaires (UserForm) au centre. À
// l'exécution, peut se réduire en mini-bande façon mini-lecteur (option persistée).

const MIN_ON_RUN_KEY = 'kubuno-macro-minimize-on-run'
type MonacoMount = { languages: { typescript: { typescriptDefaults: { addExtraLib: (types: string, name: string) => void } } } }

const MODULE_TEMPLATE = `// Module — l'API « Kubuno » agit sur le document ouvert.
// Astuce : affichez un formulaire avec Kubuno.Forms.show('Form1').
console.log('Hello')
`

interface Props {
  docType: string
  docId: string
  macroId: string
  buildApi: () => unknown
  onClose: () => void
}

export function MacroEditorWindow({ docType, docId, macroId, buildApi, onClose }: Props) {
  const { t } = useTranslation('office')
  const [items, setItems] = useState<DocMacro[] | null>(null)
  const [selectedId, setSelectedId] = useState<string>(macroId)
  const [formTab, setFormTab] = useState<'design' | 'code'>('design')
  const [apiTypes, setApiTypes] = useState('')
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)
  const [running, setRunning] = useState(false)
  const [result, setResult] = useState<MacroResult | null>(null)
  const [minimized, setMinimized] = useState(false)
  const [showOpts, setShowOpts] = useState(false)
  const [minOnRun, setMinOnRun] = useState(() => { try { return localStorage.getItem(MIN_ON_RUN_KEY) === '1' } catch { return false } })
  const [pos, setPos] = useState(() => ({ x: Math.max(20, (window.innerWidth - 860) / 2), y: 70 }))
  const [size, setSize] = useState({ w: 860, h: 540 })
  const drag = useRef<{ dx: number; dy: number } | null>(null)
  const resize = useRef<{ sx: number; sy: number; w: number; h: number } | null>(null)
  useEffect(() => { ensureJetBrainsMono() }, [])
  // Menu contextuel de l'arborescence + fenêtre Exécution/Immédiat (débogage façon VBA).
  const [treeCtx, setTreeCtx] = useState<{ x: number; y: number; kind: 'item' | 'modules' | 'forms'; id?: string } | null>(null)
  const [debugOpen, setDebugOpen] = useState(false)
  const [immediate, setImmediate] = useState('')
  const [consoleLines, setConsoleLines] = useState<{ level: string; text: string }[]>([])
  const lastApiRef = useRef<Record<string, unknown> | null>(null)
  const consoleScrollRef = useRef<HTMLDivElement>(null)
  const runRef = useRef<() => void>(() => {})
  useEffect(() => { const el = consoleScrollRef.current; if (el) el.scrollTop = el.scrollHeight }, [consoleLines])

  useEffect(() => { getApiTypes().then(setApiTypes).catch(() => {}) }, [])
  useEffect(() => {
    docMacrosApi.list(docType, docId).then(ms => { setItems(ms); if (!ms.some(m => m.id === selectedId) && ms[0]) setSelectedId(ms[0].id) }).catch(() => setItems([]))
  }, [docType, docId]) // eslint-disable-line react-hooks/exhaustive-deps

  const itemsRef = useRef<DocMacro[] | null>(items); itemsRef.current = items
  const selected = items?.find(i => i.id === selectedId) ?? null
  const isForm = selected?.kind === 'form'

  const update = (id: string, patch: Partial<DocMacro>) => setItems(prev => prev?.map(i => i.id === id ? { ...i, ...patch } : i) ?? null)

  const save = useCallback(async () => {
    if (!itemsRef.current) return
    setSaving(true)
    try { setItems(await docMacrosApi.save(docType, docId, itemsRef.current)); setSaved(true); setTimeout(() => setSaved(false), 1500) }
    finally { setSaving(false) }
  }, [docType, docId])

  const addModule = () => {
    const n = (items?.filter(i => i.kind !== 'form').length ?? 0) + 1
    const it: DocMacro = { id: crypto.randomUUID(), name: `Module${n}`, kind: 'module', source: MODULE_TEMPLATE }
    setItems(prev => [...(prev ?? []), it]); setSelectedId(it.id)
  }
  const addForm = () => {
    const n = (items?.filter(i => i.kind === 'form').length ?? 0) + 1
    const it: DocMacro = { id: crypto.randomUUID(), name: `Form${n}`, kind: 'form', source: `// Code du formulaire ${`Form${n}`}.\n// function Button1_Click() { Form.close() }\n`, controls: [], formW: 360, formH: 240 }
    setItems(prev => [...(prev ?? []), it]); setSelectedId(it.id); setFormTab('design')
  }
  const removeItem = (id: string) => setItems(prev => {
    const next = (prev ?? []).filter(i => i.id !== id)
    if (id === selectedId) setSelectedId(next[0]?.id ?? '')
    return next
  })
  const duplicateItem = (id: string) => {
    const it = items?.find(i => i.id === id); if (!it) return
    const base = it.name.replace(/\d+$/, ''); const n = (items?.filter(i => i.name.startsWith(base)).length ?? 0) + 1
    const copy: DocMacro = { ...it, id: crypto.randomUUID(), name: `${base}${n}`, controls: it.controls?.map(c => ({ ...c, id: crypto.randomUUID() })) }
    setItems(prev => [...(prev ?? []), copy]); setSelectedId(copy.id)
  }

  // Attribue/édite une fonction d'événement à un contrôle (façon VBA) : crée le stub
  // `function <Nom>_<Event>() {}` s'il manque, puis bascule sur l'onglet Code.
  const editControlHandler = (c: FormControl, event: string) => {
    const cur = itemsRef.current?.find(i => i.id === selectedId)
    if (!cur) return
    const fnName = `${c.name}_${event}`
    let src = cur.source || ''
    if (!new RegExp(`function\\s+${fnName}\\b`).test(src)) {
      src = `${src.trimEnd()}\n\nfunction ${fnName}() {\n  \n}\n`.replace(/^\n+/, '')
      update(cur.id, { source: src })
    }
    setFormTab('code')
  }

  const buildFullApi = () => {
    const api = buildApi() as Record<string, unknown>
    api.Forms = makeFormsApi(() => itemsRef.current ?? [], () => api)   // injecte le runtime de formulaires
    lastApiRef.current = api
    return api
  }
  const runItem = async (it: DocMacro) => {
    if (it.kind === 'form') return
    await save()
    setRunning(true)
    try {
      const res = await runMacro(it.source, buildFullApi(), Date.now())
      setResult(res)
      const lines: { level: string; text: string }[] = res.logs.map(l => ({ level: l.level as string, text: l.text }))
      if (res.error) lines.push({ level: 'error', text: res.error })
      else if (res.returnValue !== undefined) lines.push({ level: 'return', text: '⟼ ' + (typeof res.returnValue === 'string' ? res.returnValue : JSON.stringify(res.returnValue)) })
      setConsoleLines(prev => [...prev.slice(-200), { level: 'sys', text: `▶ ${it.name} · ${res.durationMs} ms` }, ...lines])
      if (minOnRun) setMinimized(true)
      else if (lines.length) setDebugOpen(true)
    } finally { setRunning(false) }
  }
  const run = () => { if (selected) void runItem(selected) }

  // Items du menu contextuel de l'arborescence (clic droit).
  const treeCtxItems = (): MenuItem[] => {
    if (!treeCtx) return []
    if (treeCtx.kind !== 'item') {
      const isMod = treeCtx.kind === 'modules'
      return [{ type: 'action', label: isMod ? t('macro_add_module', { defaultValue: 'Ajouter un module' }) : t('macro_add_form', { defaultValue: 'Ajouter un formulaire' }), icon: <Plus size={14} />, onClick: isMod ? addModule : addForm }]
    }
    const it = items?.find(i => i.id === treeCtx.id); if (!it) return []
    const out: MenuItem[] = []
    if (it.kind !== 'form') out.push({ type: 'action', label: t('script_run', { defaultValue: 'Exécuter' }), icon: <Play size={14} />, onClick: () => { setSelectedId(it.id); void runItem(it) } })
    out.push({ type: 'action', label: t('common_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: () => duplicateItem(it.id) })
    out.push({ type: 'action', label: t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={14} />, danger: true, onClick: () => removeItem(it.id) })
    return out
  }

  // Fenêtre Exécution / immédiat (façon VBA) : évalue une expression avec `Kubuno`.
  const evalImmediate = async () => {
    const expr = immediate.trim(); if (!expr) return
    setImmediate('')
    setConsoleLines(prev => [...prev, { level: 'input', text: '> ' + expr }])
    try {
      const api = lastApiRef.current ?? buildFullApi()
      const logs: { level: string; text: string }[] = []
      const sandboxConsole = { log: (...a: unknown[]) => logs.push({ level: 'log', text: a.map(x => typeof x === 'string' ? x : JSON.stringify(x)).join(' ') }), warn: () => {}, error: () => {}, info: () => {} }
      // eslint-disable-next-line no-new-func
      const fn = new Function('Kubuno', 'console', `"use strict"; return (async () => (${expr}))()`)
      const v = await fn(api, sandboxConsole)
      setConsoleLines(prev => [...prev, ...logs, { level: 'return', text: '⟼ ' + (v === undefined ? 'undefined' : typeof v === 'string' ? v : JSON.stringify(v)) }])
    } catch (e) { setConsoleLines(prev => [...prev, { level: 'error', text: e instanceof Error ? e.message : String(e) }]) }
  }

  const toggleMinOnRun = () => setMinOnRun(v => { const nv = !v; try { localStorage.setItem(MIN_ON_RUN_KEY, nv ? '1' : '0') } catch { /* noop */ } return nv })
  const onHeaderDown = (e: React.PointerEvent) => { if ((e.target as HTMLElement).closest('input,button')) return; drag.current = { dx: e.clientX - pos.x, dy: e.clientY - pos.y }; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId) }
  const onHeaderMove = (e: React.PointerEvent) => { if (!drag.current) return; setPos({ x: Math.min(window.innerWidth - 100, Math.max(0, e.clientX - drag.current.dx)), y: Math.min(window.innerHeight - 40, Math.max(0, e.clientY - drag.current.dy)) }) }
  // Redimensionnement par la poignée d'angle bas-droit.
  const onResizeDown = (e: React.PointerEvent) => { e.stopPropagation(); resize.current = { sx: e.clientX, sy: e.clientY, w: size.w, h: size.h }; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId) }
  const onResizeMove = (e: React.PointerEvent) => { if (!resize.current) return; const r = resize.current; setSize({ w: Math.max(520, Math.round(r.w + e.clientX - r.sx)), h: Math.max(320, Math.round(r.h + e.clientY - r.sy)) }) }
  runRef.current = run
  const onMount = (editor: { addCommand?: (k: number, fn: () => void) => void }, monaco: MonacoMount & { KeyMod?: { CtrlCmd: number }; KeyCode?: { Enter: number } }) => {
    if (apiTypes) monaco.languages.typescript.typescriptDefaults.addExtraLib(apiTypes, 'file:///kubuno-script-api.d.ts')
    // Ctrl/Cmd+Entrée = exécuter (façon IDE).
    if (editor.addCommand && monaco.KeyMod && monaco.KeyCode) editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => runRef.current())
  }

  const statusDot = running ? 'bg-amber-400 animate-pulse' : result ? (result.ok ? 'bg-green-400' : 'bg-red-400') : 'bg-[#5a5a5a]'

  if (minimized) {
    return createPortal(
      <div data-kubuno-floating className="fixed bottom-4 right-4 z-[2147483000] flex items-center gap-2 h-11 pl-3 pr-1.5 rounded-lg bg-[#2d2d30] text-[#e8e8e8] shadow-2xl border border-[#3c3c3c] select-none" style={{ width: 300 }}>
        <span className={`w-2 h-2 rounded-full flex-shrink-0 ${statusDot}`} />
        <Zap size={14} className="flex-shrink-0 text-amber-300" />
        <span className="flex-1 text-sm truncate">{selected?.name || t('macros', { defaultValue: 'Macros' })}</span>
        <button onClick={run} disabled={running || isForm} title={t('script_run', { defaultValue: 'Exécuter' })} className="p-1.5 rounded hover:bg-[#3c3c3c] disabled:opacity-40"><Play size={15} className="text-green-400" /></button>
        <button onClick={() => setMinimized(false)} title={t('macro_restore', { defaultValue: 'Agrandir' })} className="p-1.5 rounded hover:bg-[#3c3c3c]"><Maximize2 size={14} /></button>
        <button onClick={() => { void save(); onClose() }} title={t('common_close', { defaultValue: 'Fermer' })} className="p-1.5 rounded hover:bg-[#3c3c3c]"><X size={14} /></button>
      </div>, document.body)
  }

  const modules = items?.filter(i => i.kind !== 'form') ?? []
  const forms = items?.filter(i => i.kind === 'form') ?? []

  return createPortal(
    <div data-kubuno-floating className="fixed z-[2147483000] flex flex-col rounded-lg overflow-hidden shadow-2xl border border-[#3c3c3c] bg-[#1e1e1e]" style={{ left: pos.x, top: pos.y, width: size.w, height: size.h }}>
      {/* En-tête déplaçable */}
      <div onPointerDown={onHeaderDown} onPointerMove={onHeaderMove} onPointerUp={() => { drag.current = null }} className="flex items-center gap-2 px-3 h-9 bg-[#2d2d30] border-b border-[#3c3c3c] cursor-move select-none">
        <Zap size={14} className="text-amber-300 flex-shrink-0" />
        <span className="text-[#e8e8e8] text-sm font-medium">{t('macros_ide', { defaultValue: 'Éditeur de macros' })}</span>
        <span className="text-[#8e8e8e] text-xs">· {docType}</span>
        <div className="ml-auto relative">
          <button onClick={() => setShowOpts(v => !v)} title={t('common_options', { defaultValue: 'Options' })} className="p-1 rounded text-[#cccccc] hover:bg-[#3c3c3c]"><Settings size={14} /></button>
          {showOpts && (
            <div className="absolute right-0 top-8 z-10 w-60 p-2 rounded bg-[#2d2d30] border border-[#3c3c3c] shadow-xl">
              <label className="flex items-center gap-2 px-1 py-1.5 text-xs text-[#e8e8e8] cursor-pointer"><input type="checkbox" checked={minOnRun} onChange={toggleMinOnRun} /> {t('macro_minimize_on_run', { defaultValue: 'Réduire à l’exécution' })}</label>
            </div>
          )}
        </div>
        <button onClick={() => setMinimized(true)} title={t('macro_minimize', { defaultValue: 'Réduire' })} className="p-1 rounded text-[#cccccc] hover:bg-[#3c3c3c]"><Minus size={14} /></button>
        <button onClick={() => { void save(); onClose() }} title={t('common_close', { defaultValue: 'Fermer' })} className="p-1 rounded text-[#cccccc] hover:bg-[#3c3c3c]"><X size={14} /></button>
      </div>

      {/* Corps : centre (éditeur/concepteur) + arborescence à droite */}
      <div className="flex-1 flex overflow-hidden">
        <div className="flex-1 flex flex-col overflow-hidden border-r border-[#3c3c3c]">
          {items === null ? <div className="text-[#8e8e8e] text-sm p-4">{t('common_loading', { defaultValue: 'Chargement…' })}</div>
            : !selected ? <div className="flex-1 flex items-center justify-center text-[#7e7e7e] text-sm">{t('macros_empty', { defaultValue: 'Ajoutez un module ou un formulaire depuis l’arbre →' })}</div>
            : isForm ? (
              <>
                <div className="flex items-center gap-1 px-2 h-8 bg-[#252526] border-b border-[#3c3c3c]">
                  <input value={selected.name} onChange={e => update(selected.id, { name: e.target.value.replace(/\s/g, '') })} className="bg-transparent text-[#e8e8e8] text-xs font-medium outline-none w-32" />
                  <div className="ml-auto flex">
                    <TabBtn active={formTab === 'design'} onClick={() => setFormTab('design')}>{t('form_design', { defaultValue: 'Conception' })}</TabBtn>
                    <TabBtn active={formTab === 'code'} onClick={() => setFormTab('code')}>{t('form_code', { defaultValue: 'Code' })}</TabBtn>
                  </div>
                </div>
                <div className="flex-1 overflow-hidden">
                  {formTab === 'design'
                    ? <FormDesigner controls={selected.controls ?? []} width={selected.formW ?? 360} height={selected.formH ?? 240}
                        onChange={(controls, w, h) => update(selected.id, { controls, formW: w, formH: h })}
                        onEditHandler={editControlHandler} />
                    : <Editor height="100%" defaultLanguage="typescript" value={selected.source} onChange={v => update(selected.id, { source: v ?? '' })} onMount={onMount} theme="vs-dark"
                        options={{ fontSize: 13, fontFamily: JETBRAINS_MONO, fontLigatures: true, minimap: { enabled: false }, scrollBeyondLastLine: false, wordWrap: 'on', tabSize: 2, automaticLayout: true, padding: { top: 6 } }} />}
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center gap-1 px-2 h-8 bg-[#252526] border-b border-[#3c3c3c]">
                  <input value={selected.name} onChange={e => update(selected.id, { name: e.target.value.replace(/\s/g, '') })} className="bg-transparent text-[#e8e8e8] text-xs font-medium outline-none w-40" />
                </div>
                <div className="flex-1 overflow-hidden">
                  <Editor height="100%" defaultLanguage="typescript" value={selected.source} onChange={v => update(selected.id, { source: v ?? '' })} onMount={onMount} theme="vs-dark"
                    options={{ fontSize: 13, fontFamily: JETBRAINS_MONO, fontLigatures: true, minimap: { enabled: false }, scrollBeyondLastLine: false, wordWrap: 'on', tabSize: 2, automaticLayout: true, padding: { top: 6 } }} />
                </div>
              </>
            )}
        </div>

        {/* Arborescence du projet (à droite) */}
        <div className="w-56 flex-shrink-0 bg-[#252526] flex flex-col text-[#cccccc] text-xs overflow-auto"
          onContextMenu={e => { e.preventDefault(); setTreeCtx({ x: e.clientX, y: e.clientY, kind: 'modules' }) }}>
          <TreeGroup icon={<FileCode size={13} />} label={t('macros_modules', { defaultValue: 'Modules' })} onAdd={addModule}
            onContextMenu={e => { e.preventDefault(); e.stopPropagation(); setTreeCtx({ x: e.clientX, y: e.clientY, kind: 'modules' }) }}>
            {modules.map(m => <TreeItem key={m.id} label={m.name} active={m.id === selectedId} onClick={() => setSelectedId(m.id)} onDelete={() => removeItem(m.id)}
              onContextMenu={e => { e.preventDefault(); e.stopPropagation(); setSelectedId(m.id); setTreeCtx({ x: e.clientX, y: e.clientY, kind: 'item', id: m.id }) }} />)}
          </TreeGroup>
          <TreeGroup icon={<AppWindow size={13} />} label={t('macros_forms', { defaultValue: 'Formulaires' })} onAdd={addForm}
            onContextMenu={e => { e.preventDefault(); e.stopPropagation(); setTreeCtx({ x: e.clientX, y: e.clientY, kind: 'forms' }) }}>
            {forms.map(f => <TreeItem key={f.id} label={f.name} active={f.id === selectedId} onClick={() => { setSelectedId(f.id); setFormTab('design') }} onDelete={() => removeItem(f.id)}
              onContextMenu={e => { e.preventDefault(); e.stopPropagation(); setSelectedId(f.id); setTreeCtx({ x: e.clientX, y: e.clientY, kind: 'item', id: f.id }) }} />)}
          </TreeGroup>
        </div>
      </div>
      {treeCtx && <MenuDropdown items={treeCtxItems()} pos={{ top: treeCtx.y, left: treeCtx.x }} onClose={() => setTreeCtx(null)} theme="dark" />}

      {/* Poignée de redimensionnement (angle bas-droit) */}
      <div onPointerDown={onResizeDown} onPointerMove={onResizeMove} onPointerUp={() => { resize.current = null }}
        className="absolute bottom-0 right-0 w-4 h-4 cursor-nwse-resize z-10"
        style={{ background: 'linear-gradient(135deg, transparent 50%, #5a5a5a 50%, #5a5a5a 60%, transparent 60%, transparent 72%, #5a5a5a 72%, #5a5a5a 82%, transparent 82%)' }} />

      {/* Fenêtre Exécution / Immédiat (débogage façon VBA) */}
      {debugOpen && (
        <div className="h-40 flex flex-col border-t border-[#3c3c3c] bg-[#181818] flex-shrink-0">
          <div className="flex items-center gap-2 px-3 h-6 bg-[#252526] text-[#9e9e9e] text-[10px] uppercase tracking-wide">
            <Terminal size={12} /> {t('macro_console', { defaultValue: 'Exécution / Immédiat' })}
            <button onClick={() => setConsoleLines([])} className="ml-auto hover:text-white normal-case">{t('common_clear', { defaultValue: 'Effacer' })}</button>
            <button onClick={() => setDebugOpen(false)} className="hover:text-white"><X size={12} /></button>
          </div>
          <div ref={consoleScrollRef} style={{ fontFamily: JETBRAINS_MONO }} className="flex-1 overflow-auto px-3 py-1 text-[11px] leading-relaxed">
            {consoleLines.length === 0 && <div className="text-[#5e5e5e]">{t('macro_console_hint', { defaultValue: 'Sortie des exécutions. Tapez une expression ci-dessous (ex. Kubuno.Sheet.getValue("A1")) puis Entrée.' })}</div>}
            {consoleLines.map((l, i) => <div key={i} className={lineClass(l.level)}>{l.text}</div>)}
          </div>
          <div className="flex items-center gap-1.5 px-3 h-7 border-t border-[#3c3c3c]">
            <span className="text-[#5a9bdc] text-xs font-mono">&gt;</span>
            <input value={immediate} onChange={e => setImmediate(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); void evalImmediate() } }}
              placeholder={t('macro_immediate_ph', { defaultValue: 'Évaluer une expression…' })}
              style={{ fontFamily: JETBRAINS_MONO }} className="flex-1 bg-transparent text-[#e8e8e8] text-[11px] outline-none" />
          </div>
        </div>
      )}

      {/* Barre d'actions */}
      <div className="flex items-center gap-2 px-3 h-10 bg-[#2d2d30] border-t border-[#3c3c3c]">
        <button onClick={run} disabled={running || isForm || !selected} title={isForm ? t('macro_run_form_hint', { defaultValue: 'Affichez un formulaire depuis un module : Kubuno.Forms.show(\'…\')' }) : t('macro_run_shortcut', { defaultValue: 'Exécuter (Ctrl+Entrée)' })}
          className="flex items-center gap-1.5 text-xs bg-green-700 hover:bg-green-600 text-white px-3 py-1.5 rounded disabled:opacity-40">
          <Play size={14} /> {running ? t('script_running', { defaultValue: 'Exécution…' }) : t('script_run', { defaultValue: 'Exécuter' })}
        </button>
        <button onClick={() => void save()} disabled={saving} className="flex items-center gap-1.5 text-xs text-[#cccccc] hover:text-white px-2 py-1.5 hover:bg-[#3c3c3c] rounded disabled:opacity-50">
          {saved ? <Check size={14} className="text-green-400" /> : <Save size={14} />} {saving ? t('script_saving', { defaultValue: 'Enregistrement…' }) : t('common_save', { defaultValue: 'Enregistrer' })}
        </button>
        <button onClick={() => setDebugOpen(v => !v)} title={t('macro_console', { defaultValue: 'Exécution / Immédiat' })}
          className={`flex items-center gap-1.5 text-xs px-2 py-1.5 rounded hover:bg-[#3c3c3c] ${debugOpen ? 'text-[#5a9bdc]' : 'text-[#cccccc]'}`}><Terminal size={14} /></button>
        {result && (
          <div className="ml-auto flex items-center gap-1.5 text-xs truncate max-w-80">
            <span className={`w-2 h-2 rounded-full flex-shrink-0 ${result.ok ? 'bg-green-400' : 'bg-red-400'}`} />
            <span className="text-[#b8b8b8] truncate">
              {result.ok ? (result.returnValue !== undefined ? `⟼ ${typeof result.returnValue === 'string' ? result.returnValue : JSON.stringify(result.returnValue)}` : (result.logs.at(-1)?.text ?? t('macro_done', { defaultValue: 'Terminé' }))) : (result.error?.split('\n')[0] ?? t('macro_error', { defaultValue: 'Erreur' }))}
            </span>
            <span className="text-[#6e6e6e] flex-shrink-0">{result.durationMs} ms</span>
          </div>
        )}
      </div>
    </div>, document.body)
}

function TabBtn({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return <button onClick={onClick} className={`px-2.5 h-6 text-xs rounded-t ${active ? 'bg-[#1e1e1e] text-[#e8e8e8]' : 'text-[#9e9e9e] hover:text-[#e8e8e8]'}`}>{children}</button>
}
// Couleur d'une ligne de la console selon son niveau.
function lineClass(level: string): string {
  return level === 'error' ? 'text-[#e84a4a] whitespace-pre-wrap'
    : level === 'return' ? 'text-[#5a9bdc]'
    : level === 'warn' ? 'text-[#e8a23d]'
    : level === 'sys' ? 'text-[#6e9e6e]'
    : level === 'input' ? 'text-[#9e9e9e]'
    : 'text-[#cccccc]'
}
function TreeGroup({ icon, label, onAdd, onContextMenu, children }: { icon: React.ReactNode; label: string; onAdd: () => void; onContextMenu?: (e: React.MouseEvent) => void; children: React.ReactNode }) {
  return (
    <div className="py-1">
      <div onContextMenu={onContextMenu} className="flex items-center gap-1 px-2 py-1 text-[#9e9e9e] uppercase tracking-wide text-[10px] font-medium">
        <ChevronDown size={12} /> {icon} <span className="flex-1">{label}</span>
        <button onClick={onAdd} className="p-0.5 rounded hover:bg-[#3c3c3c] text-[#cccccc]"><Plus size={13} /></button>
      </div>
      <div>{children}</div>
    </div>
  )
}
function TreeItem({ label, active, onClick, onDelete, onContextMenu }: { label: string; active: boolean; onClick: () => void; onDelete: () => void; onContextMenu?: (e: React.MouseEvent) => void }) {
  return (
    <div onClick={onClick} onContextMenu={onContextMenu} className={`group flex items-center gap-1.5 pl-7 pr-1.5 py-1 cursor-pointer ${active ? 'bg-[#37373d] text-white' : 'hover:bg-[#2a2a2a]'}`}>
      <span className="flex-1 truncate">{label}</span>
      <button onClick={e => { e.stopPropagation(); onDelete() }} className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-[#4a2a2a] text-[#e84a4a]"><Trash2 size={12} /></button>
    </div>
  )
}
