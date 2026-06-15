import '../../modules/code/monacoSetup'
import Editor from '@monaco-editor/react'
import { useState, useEffect, useRef, useCallback } from 'react'
import { useParams } from 'react-router-dom'
import { useDebouncedAutosave } from '@kubuno/sdk'
import { Plus, Play, Save, Code2, Zap, Clock, Trash2, ChevronRight, X, Check, ExternalLink, Copy } from 'lucide-react'
import clsx from 'clsx'
import { useTranslation } from 'react-i18next'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { useConfirm } from '@kubuno/sdk'
import { ConfirmDialog } from '@ui'
import { Button, Dropdown } from '@ui'
import type { StartPageRecentItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import { scriptsApi, triggersApi, runsApi, getApiTypes } from './script-api'
import type { Script, ScriptRun, ScriptTrigger, ConsoleEntry } from './script-api'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_SCRIPT } from './ribbon/officeThemes'
import { fileGroup } from './ribbon/common'

// ── Helper ────────────────────────────────────────────────────────────────────

function formatDuration(ms: number | null) {
  if (ms === null) return '–'
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function formatDate(iso: string | null) {
  if (!iso) return '–'
  return format(new Date(iso), 'P p', { locale: getDateLocale() })
}

function statusColor(status: string) {
  if (status === 'success') return 'text-green-400'
  if (status === 'error')   return 'text-red-400'
  if (status === 'timeout') return 'text-yellow-400'
  if (status === 'running') return 'text-blue-400'
  return 'text-gray-400'
}

// ── Console Panel ─────────────────────────────────────────────────────────────

interface ConsolePanelProps {
  entries: ConsoleEntry[]
  status: string | null
  errorMessage: string | null
  returnValue: unknown
  isStreaming: boolean
}

function ConsolePanel({ entries, status, errorMessage, returnValue, isStreaming }: ConsolePanelProps) {
  const { t } = useTranslation('office')
  const bottomRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [entries])

  function renderArg(arg: unknown) {
    if (typeof arg === 'string') return arg
    return JSON.stringify(arg)
  }

  function entryColor(level: string) {
    if (level === 'warn')  return 'text-yellow-300'
    if (level === 'error') return 'text-red-400'
    return 'text-gray-300'
  }

  return (
    <div className="flex flex-col h-full bg-[#1e1e1e] border-l border-[#3c3c3c]">
      <div className="flex items-center px-3 py-2 border-b border-[#3c3c3c] text-[#cccccc] text-xs font-semibold uppercase tracking-wider">
        <span>{t('script_console')}</span>
        {isStreaming && (
          <span className="ml-2 w-2 h-2 rounded-full bg-blue-400 animate-pulse" />
        )}
        {status && (
          <span className={clsx('ml-auto font-normal', statusColor(status))}>
            {status}
          </span>
        )}
      </div>

      <div className="flex-1 overflow-y-auto font-mono text-xs p-3 space-y-1">
        {entries.length === 0 && !status && (
          <p className="text-[#858585] italic">{t('script_console_empty')}</p>
        )}

        {entries.map((e, i) => (
          <div key={i} className={clsx('flex gap-2', entryColor(e.level))}>
            <span className="opacity-50 select-none">[{e.level}]</span>
            <span>{e.args.map(renderArg).join(' ')}</span>
          </div>
        ))}

        {errorMessage && (
          <div className="mt-2 p-2 border border-red-700 rounded text-red-400 bg-red-950/30">
            <span className="font-semibold">{t('script_error_label')}</span> {errorMessage}
          </div>
        )}

        {returnValue !== null && returnValue !== undefined && status === 'success' && (
          <div className="mt-2 p-2 border border-green-700 rounded text-green-400 bg-green-950/30">
            <span className="font-semibold">{t('script_return_label')}</span>{' '}
            <span>{JSON.stringify(returnValue)}</span>
          </div>
        )}

        <div ref={bottomRef} />
      </div>
    </div>
  )
}

// ── Trigger Form ──────────────────────────────────────────────────────────────

interface TriggerFormProps {
  scriptId: string
  onCreated: () => void
  onClose: () => void
}

function TriggerForm({ scriptId, onCreated, onClose }: TriggerFormProps) {
  const { t } = useTranslation('office')
  const [type, setType]       = useState<'cron' | 'event' | 'webhook'>('cron')
  const [name, setName]       = useState('')
  const [cron, setCron]       = useState('0 * * * *')
  const [eventName, setEvent] = useState('')
  const [saving, setSaving]   = useState(false)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setSaving(true)
    try {
      await triggersApi.create(scriptId, {
        name:            name || undefined,
        trigger_type:    type,
        cron_expression: type === 'cron' ? cron : undefined,
        event_name:      type === 'event' ? eventName : undefined,
      })
      onCreated()
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-[#252526] border border-[#3c3c3c] rounded-lg w-96 p-6 space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-white font-semibold">{t('script_trigger_new')}</h3>
          <button onClick={onClose} className="text-[#858585] hover:text-white"><X size={16} /></button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-3">
          <div>
            <label className="block text-xs text-[#cccccc] mb-1">{t('script_trigger_name')}</label>
            <input
              className="w-full bg-[#3c3c3c] border border-[#555] rounded px-3 py-1.5 text-sm text-white focus:outline-none focus:border-blue-500"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder={t('script_trigger_name_placeholder')}
            />
          </div>

          <div>
            <label className="block text-xs text-[#cccccc] mb-1">{t('script_trigger_type')}</label>
            <Dropdown
              variant="dark"
              width="100%"
              value={type}
              onChange={v => setType(v as 'cron' | 'event' | 'webhook')}
              options={[
                { value: 'cron',    label: t('script_trigger_type_cron') },
                { value: 'event',   label: t('script_trigger_type_event') },
                { value: 'webhook', label: t('script_trigger_type_webhook') },
              ]}
            />
          </div>

          {type === 'cron' && (
            <div>
              <label className="block text-xs text-[#cccccc] mb-1">{t('script_cron_expression')}</label>
              <input
                className="w-full bg-[#3c3c3c] border border-[#555] rounded px-3 py-1.5 text-sm text-white font-mono focus:outline-none focus:border-blue-500"
                value={cron}
                onChange={e => setCron(e.target.value)}
                placeholder="0 * * * *"
              />
              <p className="text-[10px] text-[#858585] mt-1">{t('script_cron_hint')}</p>
            </div>
          )}

          {type === 'event' && (
            <div>
              <label className="block text-xs text-[#cccccc] mb-1">{t('script_event_name')}</label>
              <input
                className="w-full bg-[#3c3c3c] border border-[#555] rounded px-3 py-1.5 text-sm text-white focus:outline-none focus:border-blue-500"
                value={eventName}
                onChange={e => setEvent(e.target.value)}
                placeholder="FileUploaded, UserCreated…"
              />
            </div>
          )}

          {type === 'webhook' && (
            <p className="text-xs text-[#858585]">{t('script_webhook_note')}</p>
          )}

          <div className="flex gap-2 pt-2">
            <button
              type="submit"
              disabled={saving}
              className="flex-1 bg-blue-600 hover:bg-blue-700 text-white rounded px-4 py-2 text-sm font-medium disabled:opacity-50"
            >
              {saving ? t('script_creating') : t('common_create')}
            </button>
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm text-[#cccccc] hover:text-white border border-[#555] rounded"
            >
              {t('common_cancel')}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

type View = 'editor' | 'triggers' | 'history'

interface SidebarProps {
  scripts: Script[]
  selectedId: string | null
  view: View
  onSelect: (id: string) => void
  onNew: () => void
  onViewChange: (v: View) => void
}

function Sidebar({ scripts, selectedId, view, onSelect, onNew, onViewChange }: SidebarProps) {
  const { t } = useTranslation('office')
  return (
    <div className="w-56 flex flex-col bg-[#252526] border-r border-[#3c3c3c] h-full">
      <div className="flex items-center justify-between px-3 py-3 border-b border-[#3c3c3c]">
        <span className="text-xs text-[#cccccc] font-semibold uppercase tracking-wider">{t('script_scripts')}</span>
        <button
          onClick={onNew}
          className="text-[#cccccc] hover:text-white hover:bg-[#3c3c3c] rounded p-1"
          title={t('script_new')}
        >
          <Plus size={14} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {scripts.map(s => (
          <button
            key={s.id}
            onClick={() => onSelect(s.id)}
            className={clsx(
              'w-full flex items-center gap-2 px-3 py-2 text-left text-sm truncate',
              s.id === selectedId
                ? 'bg-[#094771] text-white'
                : 'text-[#cccccc] hover:bg-[#2a2d2e]'
            )}
          >
            <Code2 size={14} className="shrink-0 opacity-70" />
            <span className="truncate">{s.name}</span>
            {s.last_run_status && (
              <span className={clsx('ml-auto text-[10px]', statusColor(s.last_run_status))}>●</span>
            )}
          </button>
        ))}

        {scripts.length === 0 && (
          <div className="px-3 py-4 text-xs text-[#858585] text-center">
            {t('script_empty_title')}<br />{t('script_empty_hint')}
          </div>
        )}
      </div>

      {/* View switcher */}
      <div className="border-t border-[#3c3c3c] flex">
        {([
          ['editor',   <Code2 size={14} />,  t('script_view_editor')],
          ['triggers', <Zap size={14} />,    t('script_view_triggers')],
          ['history',  <Clock size={14} />,  t('script_view_history')],
        ] as const).map(([v, icon, label]) => (
          <button
            key={v}
            onClick={() => onViewChange(v)}
            title={label}
            className={clsx(
              'flex-1 flex items-center justify-center py-2',
              view === v ? 'text-white bg-[#094771]' : 'text-[#858585] hover:text-white hover:bg-[#2a2d2e]'
            )}
          >
            {icon}
          </button>
        ))}
      </div>
    </div>
  )
}

// ── Triggers View ─────────────────────────────────────────────────────────────

function TriggersView({ scriptId }: { scriptId: string }) {
  const { t } = useTranslation('office')
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [triggers, setTriggers]       = useState<ScriptTrigger[]>([])
  const [showForm, setShowForm]       = useState(false)
  const [loading, setLoading]         = useState(true)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const data = await triggersApi.list(scriptId)
      setTriggers(data.triggers)
    } finally {
      setLoading(false)
    }
  }, [scriptId])

  useEffect(() => { load() }, [load])

  async function handleToggle(id: string) {
    await triggersApi.toggle(id)
    load()
  }

  async function handleDelete(id: string) {
    const ok = await confirm({
      title:        t('script_trigger_delete_title'),
      message:      t('script_trigger_delete_message'),
      confirmLabel: t('common_delete'),
      variant:      'danger',
    })
    if (!ok) return
    await triggersApi.delete(id)
    load()
  }

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-3">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-white font-semibold">{t('script_view_triggers')}</h2>
        <button
          onClick={() => setShowForm(true)}
          className="flex items-center gap-1.5 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded px-3 py-1.5"
        >
          <Plus size={14} /> {t('script_add')}
        </button>
      </div>

      {loading && <p className="text-[#858585] text-sm">{t('common_loading')}</p>}

      {!loading && triggers.length === 0 && (
        <div className="text-center py-8 text-[#858585]">
          <Zap size={32} className="mx-auto mb-2 opacity-30" />
          <p className="text-sm">{t('script_no_triggers')}</p>
        </div>
      )}

      {triggers.map(trg => (
        <div key={trg.id} className="bg-[#2d2d30] border border-[#3c3c3c] rounded-lg p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className={clsx('w-2 h-2 rounded-full', trg.is_active ? 'bg-green-400' : 'bg-gray-500')} />
              <span className="text-white text-sm font-medium">{trg.name}</span>
              <span className="text-[10px] bg-[#3c3c3c] text-[#cccccc] px-2 py-0.5 rounded">
                {trg.trigger_type}
              </span>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => handleToggle(trg.id)}
                className="text-xs text-[#858585] hover:text-white px-2 py-1 border border-[#555] rounded"
              >
                {trg.is_active ? t('script_deactivate') : t('script_activate')}
              </button>
              <button
                onClick={() => handleDelete(trg.id)}
                className="text-red-400 hover:text-red-300 p-1"
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>

          {trg.cron_expression && (
            <p className="text-xs text-[#858585] mt-1 font-mono">{trg.cron_expression}</p>
          )}
          {trg.event_name && (
            <p className="text-xs text-[#858585] mt-1">{t('script_event_label')} {trg.event_name}</p>
          )}
          {trg.trigger_type === 'webhook' && trg.webhook_token && (
            <p className="text-xs text-[#858585] mt-1 font-mono">
              {t('script_token_label')} {trg.webhook_token.substring(0, 12)}…
            </p>
          )}

          <div className="mt-2 text-xs text-[#858585]">
            {t('script_fire_count', { count: trg.fire_count })}
            {trg.last_fired_at && ` · ${t('script_last_fired', { date: formatDate(trg.last_fired_at) })}`}
          </div>
        </div>
      ))}

      {showForm && (
        <TriggerForm
          scriptId={scriptId}
          onCreated={() => { setShowForm(false); load() }}
          onClose={() => setShowForm(false)}
        />
      )}

      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}

// ── History View ──────────────────────────────────────────────────────────────

function HistoryView({ scriptId }: { scriptId: string }) {
  const { t } = useTranslation('office')
  const [runs, setRuns]   = useState<ScriptRun[]>([])
  const [loading, setLoading] = useState(true)
  const [selected, setSelected] = useState<ScriptRun | null>(null)

  useEffect(() => {
    setLoading(true)
    runsApi.listForScript(scriptId).then(d => {
      setRuns(d.runs)
      setLoading(false)
    })
  }, [scriptId])

  return (
    <div className="flex-1 flex overflow-hidden">
      {/* List */}
      <div className="w-64 border-r border-[#3c3c3c] overflow-y-auto">
        {loading && <p className="text-[#858585] text-sm p-4">{t('common_loading')}</p>}
        {!loading && runs.length === 0 && (
          <p className="text-[#858585] text-sm p-4 text-center">{t('script_no_runs')}</p>
        )}
        {runs.map(run => (
          <button
            key={run.id}
            onClick={() => setSelected(run)}
            className={clsx(
              'w-full flex items-start gap-2 px-3 py-2 text-left border-b border-[#3c3c3c]',
              selected?.id === run.id ? 'bg-[#094771]' : 'hover:bg-[#2a2d2e]'
            )}
          >
            <span className={clsx('mt-1 w-2 h-2 rounded-full shrink-0', {
              'bg-green-400':  run.status === 'success',
              'bg-red-400':    run.status === 'error',
              'bg-yellow-400': run.status === 'timeout',
              'bg-blue-400':   run.status === 'running',
            })} />
            <div className="min-w-0">
              <p className="text-xs text-white">{formatDate(run.started_at)}</p>
              <p className="text-[10px] text-[#858585]">
                {run.run_source} · {formatDuration(run.duration_ms)}
              </p>
            </div>
          </button>
        ))}
      </div>

      {/* Detail */}
      <div className="flex-1 overflow-y-auto p-4">
        {!selected && (
          <p className="text-[#858585] text-sm text-center mt-8">
            {t('script_select_run')}
          </p>
        )}
        {selected && (
          <div className="space-y-4">
            <div className="flex items-center gap-3">
              <span className={clsx('text-sm font-medium', statusColor(selected.status))}>
                {selected.status.toUpperCase()}
              </span>
              <span className="text-xs text-[#858585]">{formatDuration(selected.duration_ms)}</span>
              <span className="text-xs text-[#858585]">{formatDate(selected.started_at)}</span>
            </div>

            {selected.error_message && (
              <div className="p-3 bg-red-950/30 border border-red-700 rounded text-sm text-red-300">
                {selected.error_message}
              </div>
            )}

            {selected.return_value !== null && (
              <div>
                <p className="text-xs text-[#858585] mb-1">{t('script_return_value')}</p>
                <pre className="bg-[#1e1e1e] rounded p-3 text-xs text-green-300 overflow-x-auto">
                  {JSON.stringify(selected.return_value, null, 2)}
                </pre>
              </div>
            )}

            <div>
              <p className="text-xs text-[#858585] mb-1">{t('script_console_entries', { count: selected.console_output.length })}</p>
              <div className="bg-[#1e1e1e] rounded p-3 space-y-1 max-h-64 overflow-y-auto font-mono text-xs">
                {selected.console_output.map((e, i) => (
                  <div key={i} className={clsx(
                    'flex gap-2',
                    e.level === 'error' ? 'text-red-400' :
                    e.level === 'warn'  ? 'text-yellow-300' : 'text-gray-300'
                  )}>
                    <span className="opacity-50">[{e.level}]</span>
                    <span>{e.args.map(a => typeof a === 'string' ? a : JSON.stringify(a)).join(' ')}</span>
                  </div>
                ))}
                {selected.console_output.length === 0 && (
                  <p className="text-[#858585]">{t('script_no_console_output')}</p>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

// ── Editor View ───────────────────────────────────────────────────────────────

interface EditorViewProps {
  script: Script
  onUpdate: (s: Script) => void
}

function EditorView({ script, onUpdate }: EditorViewProps) {
  const { t } = useTranslation('office')
  const [code, setCode]           = useState(script.source_code)
  const [saving, setSaving]       = useState(false)
  const [running, setRunning]     = useState(false)
  const [saved, setSaved]         = useState(false)
  const [apiTypes, setApiTypes]   = useState<string>('')

  // SSE console state
  const [consoleEntries, setConsoleEntries] = useState<ConsoleEntry[]>([])
  const [runStatus, setRunStatus]   = useState<string | null>(null)
  const [errorMessage, setErrorMsg] = useState<string | null>(null)
  const [returnValue, setReturn]    = useState<unknown>(null)
  const [isStreaming, setStreaming]  = useState(false)

  const esRef = useRef<EventSource | null>(null)

  // Update code when script changes
  useEffect(() => {
    setCode(script.source_code)
  }, [script.id, script.source_code])

  // Autosave fiable (debounce + flush au démontage/fermeture) — la source
  // (.kbscr) n'était sauvée que manuellement / avant exécution.
  useDebouncedAutosave(code, !!script.id, () => { void handleSave() })

  // Load API types for Monaco IntelliSense
  useEffect(() => {
    getApiTypes().then(setApiTypes).catch(() => {})
  }, [])

  async function handleSave() {
    setSaving(true)
    try {
      const result = await scriptsApi.update(script.id, { source_code: code })
      onUpdate(result.script)
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
    } finally {
      setSaving(false)
    }
  }

  async function handleRun() {
    // First save
    if (code !== script.source_code) {
      await handleSave()
    }

    setRunning(true)
    setConsoleEntries([])
    setRunStatus('running')
    setErrorMsg(null)
    setReturn(null)
    setStreaming(true)

    try {
      const { run_id } = await scriptsApi.run(script.id)

      // Close any previous SSE
      esRef.current?.close()

      const url = runsApi.streamUrl(run_id)
      const es = new EventSource(url)
      esRef.current = es

      es.addEventListener('console_log', (e) => {
        const data = JSON.parse(e.data)
        setConsoleEntries(prev => [...prev, data.entry as ConsoleEntry])
      })

      es.addEventListener('finished', (e) => {
        const data = JSON.parse(e.data)
        setRunStatus(data.status)
        setErrorMsg(data.error_message ?? null)
        setReturn(data.return_value ?? null)
        setStreaming(false)
        setRunning(false)
        es.close()
        esRef.current = null
      })

      es.onerror = () => {
        setStreaming(false)
        setRunning(false)
        es.close()
        esRef.current = null
      }
    } catch (err) {
      setRunStatus('error')
      setErrorMsg(String(err))
      setStreaming(false)
      setRunning(false)
    }
  }

  async function handleCompile() {
    const result = await scriptsApi.compile(script.id)
    if (result.compiled) {
      onUpdate({ ...script, compiled_code: result.compiled_code })
    }
  }

  function handleEditorMount(_editor: unknown, monaco: { languages: { typescript: { typescriptDefaults: { addExtraLib: (types: string, name: string) => void } } } }) {
    if (apiTypes) {
      monaco.languages.typescript.typescriptDefaults.addExtraLib(
        apiTypes,
        'file:///kubuno-script-api.d.ts'
      )
    }
  }

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-4 py-2 bg-[#2d2d30] border-b border-[#3c3c3c]">
        <h2 className="text-white text-sm font-medium truncate max-w-48">{script.name}</h2>
        <div className="flex items-center gap-1 ml-auto">
          <button
            onClick={handleCompile}
            className="flex items-center gap-1.5 text-xs text-[#cccccc] hover:text-white px-2 py-1.5 hover:bg-[#3c3c3c] rounded"
            title={t('script_compile_tooltip')}
          >
            <ChevronRight size={14} /> {t('script_compile')}
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-1.5 text-xs text-[#cccccc] hover:text-white px-2 py-1.5 hover:bg-[#3c3c3c] rounded disabled:opacity-50"
          >
            {saved ? <Check size={14} className="text-green-400" /> : <Save size={14} />}
            {saving ? t('script_saving') : t('common_save')}
          </button>
          <button
            onClick={handleRun}
            disabled={running}
            className="flex items-center gap-1.5 text-xs bg-green-700 hover:bg-green-600 text-white px-3 py-1.5 rounded disabled:opacity-50"
          >
            <Play size={14} />
            {running ? t('script_running') : t('script_run')}
          </button>
        </div>
      </div>

      {/* Editor + Console split */}
      <div className="flex-1 flex overflow-hidden">
        {/* Monaco Editor */}
        <div className="flex-1 overflow-hidden">
          <Editor
            height="100%"
            defaultLanguage="typescript"
            value={code}
            onChange={v => setCode(v ?? '')}
            onMount={handleEditorMount}
            theme="vs-dark"
            options={{
              fontSize: 14,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              wordWrap: 'on',
              tabSize: 2,
              insertSpaces: true,
              automaticLayout: true,
              lineNumbers: 'on',
              glyphMargin: false,
              folding: true,
              lineDecorationsWidth: 10,
              padding: { top: 8 },
            }}
          />
        </div>

        {/* Console Panel */}
        <div className="w-80 flex-shrink-0">
          <ConsolePanel
            entries={consoleEntries}
            status={runStatus}
            errorMessage={errorMessage}
            returnValue={returnValue}
            isStreaming={isStreaming}
          />
        </div>
      </div>

      {script.compile_error && (
        <div className="px-4 py-2 bg-red-900/40 border-t border-red-700 text-xs text-red-300">
          {t('script_compile_error')} {script.compile_error}
        </div>
      )}
    </div>
  )
}

// ── Main ScriptApp ────────────────────────────────────────────────────────────

export default function ScriptApp() {
  const { t, i18n } = useTranslation('office')
  const { id: routeId } = useParams<{ id: string }>()
  const [scripts, setScripts]     = useState<Script[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [view, setView]           = useState<View>('editor')
  const [loading, setLoading]     = useState(true)

  const load = useCallback(async () => {
    const data = await scriptsApi.list()
    setScripts(data.scripts)
    setLoading(false)
  }, [])

  useEffect(() => { load() }, [load])

  // Ouverture par URL (/office/script/:id) — ex. double-clic d'un .kbscr dans files.
  useEffect(() => {
    if (!routeId) return
    scriptsApi.get(routeId).then(({ script }) => {
      setScripts(prev => prev.some(s => s.id === script.id) ? prev.map(s => s.id === script.id ? script : s) : [script, ...prev])
      setSelectedId(script.id); setView('editor')
    }).catch(() => {})
  }, [routeId])

  async function handleNew() {
    const data = await scriptsApi.create({ name: t('script_new') })
    setScripts(prev => [data.script, ...prev])
    setSelectedId(data.script.id)
    setView('editor')
  }

  function handleUpdate(updated: Script) {
    setScripts(prev => prev.map(s => s.id === updated.id ? updated : s))
  }

  const selected = scripts.find(s => s.id === selectedId) ?? null

  // ── Titre éditable (standard WorkspaceShell) — nom du script sélectionné ──────
  const [titleDraft, setTitleDraft] = useState('')
  useEffect(() => { setTitleDraft(selected?.name ?? '') }, [selected?.name])
  const commitTitle = async () => {
    if (!selected) return
    const v = titleDraft.trim()
    if (v && v !== selected.name) {
      const data = await scriptsApi.update(selected.id, { name: v })
      handleUpdate(data.script)
    } else if (!v) {
      setTitleDraft(selected.name)
    }
  }
  async function handleTrash() {
    if (!selected) return
    await scriptsApi.trash(selected.id)
    setScripts(prev => prev.filter(s => s.id !== selected.id))
    setSelectedId(null)
  }
  async function handleDuplicate() {
    if (!selected) return
    const data = await scriptsApi.duplicate(selected.id)
    setScripts(prev => [data.script, ...prev])
    setSelectedId(data.script.id)
    setView('editor')
  }

  // Ouverture d'un fichier .kbscr depuis le navigateur → éditeur.
  const handleOpenFile = (file: FileItem): boolean => {
    scriptsApi.openByFile(file.id)
      .then(({ script }) => {
        setScripts(prev => prev.some(s => s.id === script.id) ? prev : [script, ...prev])
        setSelectedId(script.id); setView('editor')
      })
      .catch(() => {})
    return true
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center bg-[#1e1e1e] text-[#858585]">
        {t('common_loading')}
      </div>
    )
  }

  // Accueil (aucun script ouvert) : StartPage (récents + navigation Office/Scripts).
  if (!selected) {
    const recentItems: StartPageRecentItem[] = scripts.slice(0, 12).map(s => ({
      id:       s.id,
      name:     s.name,
      subtitle: format(new Date(s.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
      icon:     <Code2 size={18} className="text-text-tertiary" strokeWidth={1.5} />,
      onClick:  () => { setSelectedId(s.id); setView('editor') },
      actions: [
        { id: 'open',  label: t('common_open', { defaultValue: 'Ouvrir' }), icon: <ExternalLink size={15} />, onClick: () => { setSelectedId(s.id); setView('editor') } },
        { id: 'dup',   label: t('common_duplicate'), icon: <Copy size={15} />, onClick: () => { scriptsApi.duplicate(s.id).then(d => { setScripts(prev => [d.script, ...prev]); setSelectedId(d.script.id); setView('editor') }) } },
        { id: 'trash', label: t('script_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => { scriptsApi.trash(s.id).then(() => setScripts(prev => prev.filter(x => x.id !== s.id))) } },
      ],
    }))
    return (
      <ModuleStartPage
        recentTitle={t('script_recent', { defaultValue: 'Récents' })}
        recentItems={recentItems}
        recentEmpty={
          <div className="flex flex-col items-center gap-2">
            <Code2 size={32} className="text-text-tertiary opacity-30" strokeWidth={1.5} />
            <p className="text-text-tertiary text-xs">{t('script_select_or_create')}</p>
          </div>
        }
        browse={{
          folderPathPrefix: 'Office/Scripts',
          title: t('script_title', { defaultValue: 'Script' }),
          fileTypeModuleId: 'office-script',
          onOpenFile: handleOpenFile,
          toolbarContent: (
            <Button icon={<Plus size={15} />} onClick={handleNew}>
              {t('script_new')}
            </Button>
          ),
        }}
      />
    )
  }

  return (
    <OfficeShell
      ribbon={[{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
        groups: [fileGroup(t, { onNew: handleNew, onDuplicate: selected ? handleDuplicate : undefined })] }]}
      theme={THEME_SCRIPT}
      chromeless
      topbarHeight={64}
      titleIcon={<Code2 size={16} style={{ color: THEME_SCRIPT.accent }} className="flex-shrink-0" />}
      title={selected ? titleDraft : t('script_title', { defaultValue: 'Script' })}
      onBack={() => setSelectedId(null)}
      onTitleChange={selected ? setTitleDraft : undefined}
      onTitleCommit={selected ? commitTitle : undefined}
      titlePlaceholder={t('common_untitled', { defaultValue: 'Sans titre' })}
      onDelete={selected ? handleTrash : undefined}
      deleteTitle={t('script_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('script_delete_confirm_title', { defaultValue: 'Supprimer ce script ?' }),
        message: t('script_delete_confirm_msg', { defaultValue: 'Le script sera déplacé dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      }}
    >
    <div className="flex flex-1 min-w-0 min-h-0 bg-[#1e1e1e] text-[#cccccc] overflow-hidden">
      <Sidebar
        scripts={scripts}
        selectedId={selectedId}
        view={view}
        onSelect={id => { setSelectedId(id); setView('editor') }}
        onNew={handleNew}
        onViewChange={setView}
      />

      {!selected && (
        <div className="flex-1 flex flex-col items-center justify-center text-[#858585]">
          <Code2 size={48} className="opacity-30 mb-4" />
          <p className="text-sm">{t('script_select_or_create')}</p>
          <button
            onClick={handleNew}
            className="mt-4 flex items-center gap-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded px-4 py-2"
          >
            <Plus size={16} /> {t('script_new')}
          </button>
        </div>
      )}

      {selected && view === 'editor' && (
        <EditorView script={selected} onUpdate={handleUpdate} />
      )}

      {selected && view === 'triggers' && (
        <TriggersView scriptId={selected.id} />
      )}

      {selected && view === 'history' && (
        <HistoryView scriptId={selected.id} />
      )}
    </div>
    </OfficeShell>
  )
}
