import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { X } from 'lucide-react'
import type { DocMacro, FormControl } from '../script-api'

// Runtime des FORMULAIRES (UserForm VBA) : `Kubuno.Forms.show(name)` rend le
// formulaire nommé en modale interactive et renvoie une promesse résolue à sa
// fermeture (valeurs des champs, ou ce que `Form.close(x)` passe). Les clics de
// bouton exécutent le gestionnaire `<NomBouton>_Click()` défini dans le code du
// formulaire ; celui-ci dispose de `Form` (getValue/setValue/close) et `Kubuno`.

interface ShowReq {
  form: DocMacro
  kubuno: unknown
  resolve: (value: unknown) => void
}

// Pont module-level entre l'API appelée DANS la macro et le composant React FormHost.
let hostShow: ((req: ShowReq) => void) | null = null

export function makeFormsApi(getForms: () => DocMacro[], getKubuno: () => unknown) {
  return {
    /** Affiche le formulaire nommé ; résout à sa fermeture. */
    show: (name: string) => new Promise<unknown>((resolve) => {
      const form = getForms().find(f => (f.kind === 'form') && f.name === name)
      if (!form || !hostShow) { resolve(null); return }
      hostShow({ form, kubuno: getKubuno(), resolve })
    }),
    /** Noms des formulaires disponibles. */
    list: () => getForms().filter(f => f.kind === 'form').map(f => f.name),
  }
}

// ── Boîtes de dialogue App (alert / confirm / prompt) ─────────────────────────
interface DialogReq { kind: 'alert' | 'confirm' | 'prompt'; message: string; def?: string; resolve: (v: unknown) => void }
let dialogShow: ((r: DialogReq) => void) | null = null
export function appAlert(message: unknown): Promise<void> {
  return new Promise(res => { if (!dialogShow) { res(); return } dialogShow({ kind: 'alert', message: String(message), resolve: () => res() }) })
}
export function appConfirm(message: unknown): Promise<boolean> {
  return new Promise(res => { if (!dialogShow) { res(false); return } dialogShow({ kind: 'confirm', message: String(message), resolve: v => res(!!v) }) })
}
export function appPrompt(message: unknown, def?: unknown): Promise<string | null> {
  return new Promise(res => { if (!dialogShow) { res(null); return } dialogShow({ kind: 'prompt', message: String(message), def: def == null ? '' : String(def), resolve: v => res(v as string | null) }) })
}

export function DialogHost() {
  const { t } = useTranslation('office')
  const [req, setReq] = useState<DialogReq | null>(null)
  const [val, setVal] = useState('')
  useEffect(() => { dialogShow = r => { setVal(r.def ?? ''); setReq(r) }; return () => { dialogShow = null } }, [])
  if (!req) return null
  const done = (v: unknown) => { const r = req; setReq(null); r.resolve(v) }
  return createPortal(
    <div className="fixed inset-0 z-[2147483600] flex items-center justify-center bg-black/30" onMouseDown={() => req.kind === 'alert' ? done(undefined) : done(req.kind === 'confirm' ? false : null)}>
      <div className="w-80 rounded-lg bg-white shadow-2xl border border-[#dadce0] overflow-hidden" onMouseDown={e => e.stopPropagation()}>
        <div className="px-4 py-3 text-sm text-text-primary whitespace-pre-wrap">{req.message}</div>
        {req.kind === 'prompt' && (
          <div className="px-4 pb-2"><input autoFocus value={val} onChange={e => setVal(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') done(val) }}
            className="w-full h-9 px-2 rounded border border-border focus:border-primary outline-none text-sm" /></div>
        )}
        <div className="flex justify-end gap-2 px-3 py-2 bg-surface-1 border-t border-border">
          {req.kind !== 'alert' && <button onClick={() => done(req.kind === 'confirm' ? false : null)} className="px-3 h-8 rounded text-sm text-text-secondary hover:bg-black/5">{t('common_cancel', { defaultValue: 'Annuler' })}</button>}
          <button onClick={() => done(req.kind === 'alert' ? undefined : req.kind === 'confirm' ? true : val)} className="px-3 h-8 rounded text-sm bg-primary text-white hover:bg-primary-hover">{t('common_ok', { defaultValue: 'OK' })}</button>
        </div>
      </div>
    </div>, document.body)
}

export function FormHost() {
  const [active, setActive] = useState<ShowReq | null>(null)
  useEffect(() => { hostShow = (req) => setActive(req); return () => { hostShow = null } }, [])
  if (!active) return null
  return <FormModal req={active} onDone={() => setActive(null)} />
}

function initialValues(controls: FormControl[]): Record<string, unknown> {
  const v: Record<string, unknown> = {}
  for (const c of controls) {
    if (c.type === 'textbox') v[c.name] = c.text ?? ''
    else if (c.type === 'checkbox') v[c.name] = !!c.value
  }
  return v
}

function FormModal({ req, onDone }: { req: ShowReq; onDone: () => void }) {
  const { form, kubuno, resolve } = req
  const controls = form.controls ?? []
  const [values, setValues] = useState<Record<string, unknown>>(() => initialValues(controls))
  const valuesRef = useRef(values); valuesRef.current = values

  const close = (ret?: unknown) => { resolve(ret !== undefined ? ret : valuesRef.current); onDone() }

  // Événements gérés par type de contrôle (façon VBA) → noms des fonctions attendues.
  const eventsFor = (c: FormControl): string[] =>
    c.type === 'button' || c.type === 'label' ? ['Click']
    : c.type === 'checkbox' ? ['Click', 'Change']
    : ['Change'] // textbox

  // Construit l'API `Form` + compile le code du formulaire une fois → gestionnaires
  // d'événements de TOUS les contrôles (`<Nom>_Click`, `<Nom>_Change`).
  const handlersRef = useRef<Record<string, ((...a: unknown[]) => void) | null>>({})
  const closedRef = useRef(false)
  useEffect(() => {
    const formApi = {
      getValue: (name: string) => valuesRef.current[name],
      setValue: (name: string, val: unknown) => { valuesRef.current = { ...valuesRef.current, [name]: val }; setValues(valuesRef.current) },
      close: (ret?: unknown) => { if (!closedRef.current) { closedRef.current = true; close(ret) } },
    }
    const sandboxConsole = { log: () => {}, warn: () => {}, error: () => {}, info: () => {} }
    const wanted = controls.flatMap(c => eventsFor(c).map(ev => `${c.name}_${ev}`))
    const probe = [...new Set(wanted)].map(fn => `'${fn}': (typeof ${fn} === 'function' ? ${fn} : null)`).join(',')
    try {
      // eslint-disable-next-line no-new-func
      const fn = new Function('Form', 'Kubuno', 'console', `"use strict";\n${form.source || ''}\n;return {${probe}};`)
      handlersRef.current = fn(formApi, kubuno, sandboxConsole) as Record<string, ((...a: unknown[]) => void) | null>
    } catch { handlersRef.current = {} }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // Déclenche le gestionnaire `<Nom>_<Event>` s'il est défini.
  const fire = (name: string, event: string, ...args: unknown[]) => {
    const h = handlersRef.current[`${name}_${event}`]
    if (h) { try { h(...args) } catch (e) { console.error('Form handler error:', e) } }
  }
  // Met à jour une valeur (synchro pour que getValue voie la nouvelle) puis déclenche.
  const setAndFire = (name: string, val: unknown, events: string[]) => {
    valuesRef.current = { ...valuesRef.current, [name]: val }
    setValues(valuesRef.current)
    for (const ev of events) fire(name, ev)
  }

  const w = form.formW ?? 360, h = form.formH ?? 240
  return createPortal(
    <div className="fixed inset-0 z-[2147483600] flex items-center justify-center bg-black/30" onMouseDown={() => close()}>
      <div className="rounded-lg shadow-2xl bg-[#f0f0f0] overflow-hidden" style={{ width: w + 2 }} onMouseDown={e => e.stopPropagation()}>
        <div className="flex items-center gap-2 px-3 h-8 bg-gradient-to-b from-[#5a8fd6] to-[#3f74bd] text-white text-sm font-medium select-none">
          <span className="flex-1 truncate">{form.name}</span>
          <button onClick={() => close()} className="p-0.5 rounded hover:bg-white/20"><X size={14} /></button>
        </div>
        <div className="relative" style={{ width: w, height: h }}>
          {controls.map(c => <RuntimeControl key={c.id} c={c} values={values}
            onClickEvent={() => fire(c.name, 'Click')}
            onChangeEvent={(val, events) => setAndFire(c.name, val, events)} />)}
        </div>
      </div>
    </div>,
    document.body,
  )
}

function RuntimeControl({ c, values, onClickEvent, onChangeEvent }: {
  c: FormControl; values: Record<string, unknown>
  onClickEvent: () => void; onChangeEvent: (val: unknown, events: string[]) => void
}) {
  const style: React.CSSProperties = { position: 'absolute', left: c.x, top: c.y, width: c.w, height: c.h }
  if (c.type === 'label') return <div style={style} onClick={onClickEvent} className="flex items-center text-[13px] text-[#202124] px-0.5 overflow-hidden">{c.text}</div>
  if (c.type === 'button') return <button style={style} onClick={onClickEvent} className="text-[13px] bg-[#e0e0e0] hover:bg-[#d5d5d5] border border-[#adadad] rounded-sm text-[#202124]">{c.text}</button>
  if (c.type === 'checkbox') return (
    <label style={style} className="flex items-center gap-1.5 text-[13px] text-[#202124] overflow-hidden cursor-pointer">
      <input type="checkbox" checked={!!values[c.name]} onChange={e => onChangeEvent(e.target.checked, ['Click', 'Change'])} /> {c.text}
    </label>
  )
  return <input style={style} value={String(values[c.name] ?? '')} onChange={e => onChangeEvent(e.target.value, ['Change'])}
    className="border border-[#adadad] rounded-sm text-[13px] text-[#202124] px-1 bg-white outline-none focus:border-[#5a9bdc]" />
}
