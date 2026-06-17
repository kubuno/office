import { useState, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { MenuDropdown, type MenuItem } from '@ui'
import { Type, TextCursorInput, RectangleHorizontal, SquareCheck, Trash2, Code2, Copy } from 'lucide-react'
import type { FormControl, ControlType } from '../script-api'

// Concepteur de formulaires façon UserForm VBA : barre d'outils (contrôles), canevas
// (placer / déplacer / redimensionner / sélectionner) et panneau de propriétés.

const TOOLS: { type: ControlType; Icon: typeof Type; label: string }[] = [
  { type: 'label',    Icon: Type,                label: 'Étiquette' },
  { type: 'textbox',  Icon: TextCursorInput,     label: 'Champ texte' },
  { type: 'button',   Icon: RectangleHorizontal, label: 'Bouton' },
  { type: 'checkbox', Icon: SquareCheck,         label: 'Case à cocher' },
]
const DEFAULTS: Record<ControlType, { w: number; h: number; prefix: string; text: (n: string) => string }> = {
  label:    { w: 100, h: 22, prefix: 'Label',    text: n => n },
  textbox:  { w: 140, h: 26, prefix: 'TextBox',  text: () => '' },
  button:   { w: 100, h: 28, prefix: 'Button',   text: n => n },
  checkbox: { w: 140, h: 22, prefix: 'CheckBox', text: n => n },
}

// Événement par défaut d'un contrôle (double-clic → édite ce gestionnaire), façon VBA.
export function defaultEvent(type: ControlType): string { return type === 'textbox' ? 'Change' : 'Click' }

interface Props {
  controls: FormControl[]
  width: number
  height: number
  onChange: (controls: FormControl[], width: number, height: number) => void
  /** Ouvre l'éditeur de code sur le gestionnaire d'événement du contrôle (façon VBA). */
  onEditHandler?: (c: FormControl, event: string) => void
}

export function FormDesigner({ controls, width, height, onChange, onEditHandler }: Props) {
  const { t } = useTranslation('office')
  const [selId, setSelId] = useState<string | null>(null)
  const drag = useRef<{ id: string; sx: number; sy: number; ox: number; oy: number; mode: 'move' | 'resize' } | null>(null)

  const sel = controls.find(c => c.id === selId) ?? null

  // Ajoute un contrôle. `at` = position de dépôt (glisser-déposer) ; sinon en cascade.
  const addControl = (type: ControlType, at?: { x: number; y: number }) => {
    const d = DEFAULTS[type]
    const n = controls.filter(c => c.type === type).length + 1
    const name = `${d.prefix}${n}`
    const x = at ? Math.max(0, Math.min(width - d.w, Math.round(at.x - d.w / 2))) : 16
    const y = at ? Math.max(0, Math.min(height - d.h, Math.round(at.y - d.h / 2))) : 16 + (controls.length % 8) * 6
    const ctrl: FormControl = { id: crypto.randomUUID(), type, name, x, y, w: d.w, h: d.h, text: d.text(name), value: false }
    onChange([...controls, ctrl], width, height)
    setSelId(ctrl.id)
  }
  const [dropHint, setDropHint] = useState(false)
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; id: string | null } | null>(null)
  const update = (id: string, patch: Partial<FormControl>) => onChange(controls.map(c => c.id === id ? { ...c, ...patch } : c), width, height)
  const remove = (id: string) => { onChange(controls.filter(c => c.id !== id), width, height); setSelId(null) }
  const duplicate = (id: string) => {
    const c = controls.find(x => x.id === id); if (!c) return
    const n = controls.filter(x => x.type === c.type).length + 1
    const copy: FormControl = { ...c, id: crypto.randomUUID(), name: `${DEFAULTS[c.type].prefix}${n}`, x: c.x + 12, y: c.y + 12 }
    onChange([...controls, copy], width, height); setSelId(copy.id)
  }
  const eventsOf = (type: ControlType): string[] => type === 'button' || type === 'label' ? ['Click'] : type === 'checkbox' ? ['Click', 'Change'] : ['Change']

  // Menu contextuel (clic droit) — via MenuDropdown de @ui (jamais de div maison).
  const ctxItems = (): MenuItem[] => {
    if (!ctxMenu) return []
    const c = ctxMenu.id ? controls.find(x => x.id === ctxMenu.id) : null
    if (!c) { // zone vide → ajouter un contrôle
      return TOOLS.map(({ type, label }) => ({ type: 'action', label: `${t('form_add', { defaultValue: 'Ajouter' })} ${label}`, onClick: () => addControl(type, { x: ctxMenu.x, y: ctxMenu.y }) }))
    }
    const evs = eventsOf(c.type)
    return [
      { type: 'action', label: t('form_edit_code', { defaultValue: 'Modifier le code' }) + ` (${evs[0]})`, icon: <Code2 size={14} />, onClick: () => onEditHandler?.(c, evs[0]) },
      ...(evs.length > 1 ? [{ type: 'submenu' as const, label: t('form_events', { defaultValue: 'Événements' }), items: evs.map(ev => ({ type: 'action' as const, label: `${c.name}_${ev}`, onClick: () => onEditHandler?.(c, ev) })) }] : []),
      { type: 'separator' },
      { type: 'action', label: t('common_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={14} />, onClick: () => duplicate(c.id) },
      { type: 'action', label: t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={14} />, danger: true, onClick: () => remove(c.id) },
    ]
  }

  const onDown = (e: React.MouseEvent, c: FormControl, mode: 'move' | 'resize') => {
    e.stopPropagation()
    setSelId(c.id)
    drag.current = { id: c.id, sx: e.clientX, sy: e.clientY, ox: mode === 'move' ? c.x : c.w, oy: mode === 'move' ? c.y : c.h, mode }
  }
  const onMove = (e: React.MouseEvent) => {
    const d = drag.current; if (!d) return
    const dx = e.clientX - d.sx, dy = e.clientY - d.sy
    if (d.mode === 'move') update(d.id, { x: Math.max(0, Math.round(d.ox + dx)), y: Math.max(0, Math.round(d.oy + dy)) })
    else update(d.id, { w: Math.max(24, Math.round(d.ox + dx)), h: Math.max(16, Math.round(d.oy + dy)) })
  }
  const onUp = () => { drag.current = null }

  return (
    <div className="flex h-full bg-[#2b2b2b]">
      {/* Boîte à outils */}
      <div className="w-9 flex flex-col gap-1 p-1 bg-[#252526] border-r border-[#3c3c3c] flex-shrink-0">
        {TOOLS.map(({ type, Icon, label }) => (
          <button key={type} draggable onClick={() => addControl(type)} title={`${label} — cliquer ou glisser sur le formulaire`}
            onDragStart={e => { e.dataTransfer.setData('application/x-kubuno-control', type); e.dataTransfer.effectAllowed = 'copy' }}
            className="w-7 h-7 flex items-center justify-center rounded text-[#cccccc] hover:bg-[#3c3c3c] cursor-grab active:cursor-grabbing"><Icon size={15} /></button>
        ))}
      </div>

      {/* Canevas */}
      <div className="flex-1 overflow-auto p-5" onMouseMove={onMove} onMouseUp={onUp} onMouseLeave={onUp}>
        <div className={`relative bg-[#ececec] shadow-lg ${dropHint ? 'outline outline-2 outline-dashed outline-[#1a73e8]' : ''}`} style={{ width, height }}
          onMouseDown={() => setSelId(null)}
          onContextMenu={e => { e.preventDefault(); setCtxMenu({ x: e.clientX, y: e.clientY, id: null }) }}
          onDragOver={e => { if (e.dataTransfer.types.includes('application/x-kubuno-control')) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; if (!dropHint) setDropHint(true) } }}
          onDragLeave={() => setDropHint(false)}
          onDrop={e => {
            e.preventDefault(); setDropHint(false)
            const type = e.dataTransfer.getData('application/x-kubuno-control') as ControlType
            if (!type) return
            const r = e.currentTarget.getBoundingClientRect()
            addControl(type, { x: e.clientX - r.left, y: e.clientY - r.top })
          }}>
          {controls.map(c => (
            <div key={c.id}
              onMouseDown={e => onDown(e, c, 'move')}
              onDoubleClick={e => { e.stopPropagation(); onEditHandler?.(c, defaultEvent(c.type)) }}
              onContextMenu={e => { e.preventDefault(); e.stopPropagation(); setSelId(c.id); setCtxMenu({ x: e.clientX, y: e.clientY, id: c.id }) }}
              className={`absolute select-none cursor-move ${c.id === selId ? 'outline outline-2 outline-[#1a73e8]' : ''}`}
              style={{ left: c.x, top: c.y, width: c.w, height: c.h }}>
              <ControlPreview c={c} />
              {c.id === selId && (
                <div onMouseDown={e => onDown(e, c, 'resize')}
                  className="absolute -right-1 -bottom-1 w-3 h-3 bg-[#1a73e8] border border-white cursor-nwse-resize" />
              )}
            </div>
          ))}
        </div>
        <div className="text-[#8e8e8e] text-[11px] mt-2">{t('form_canvas_hint', { defaultValue: 'Glissez pour déplacer · poignée bleue pour redimensionner' })}</div>
      </div>

      {/* Propriétés */}
      <div className="w-52 bg-[#252526] border-l border-[#3c3c3c] p-2 text-xs text-[#cccccc] flex-shrink-0 overflow-auto">
        <div className="font-medium text-[#9e9e9e] uppercase tracking-wide text-[10px] mb-2">{t('form_properties', { defaultValue: 'Propriétés' })}</div>
        {sel ? (
          <div className="space-y-2">
            <Field label="Type"><span className="text-[#9e9e9e]">{sel.type}</span></Field>
            <Field label={t('form_prop_name', { defaultValue: 'Nom' })}>
              <input value={sel.name} onChange={e => update(sel.id, { name: e.target.value.replace(/\s/g, '') })} className={inputCls} />
            </Field>
            {sel.type !== 'textbox' && (
              <Field label={t('form_prop_caption', { defaultValue: 'Légende' })}>
                <input value={sel.text ?? ''} onChange={e => update(sel.id, { text: e.target.value })} className={inputCls} />
              </Field>
            )}
            {sel.type === 'textbox' && (
              <Field label={t('form_prop_value', { defaultValue: 'Valeur initiale' })}>
                <input value={sel.text ?? ''} onChange={e => update(sel.id, { text: e.target.value })} className={inputCls} />
              </Field>
            )}
            {sel.type === 'checkbox' && (
              <label className="flex items-center gap-2 py-0.5"><input type="checkbox" checked={!!sel.value} onChange={e => update(sel.id, { value: e.target.checked })} /> {t('form_prop_checked', { defaultValue: 'Cochée' })}</label>
            )}
            <div className="grid grid-cols-2 gap-1.5">
              <Field label="X"><input type="number" value={sel.x} onChange={e => update(sel.id, { x: +e.target.value })} className={inputCls} /></Field>
              <Field label="Y"><input type="number" value={sel.y} onChange={e => update(sel.id, { y: +e.target.value })} className={inputCls} /></Field>
              <Field label="L"><input type="number" value={sel.w} onChange={e => update(sel.id, { w: +e.target.value })} className={inputCls} /></Field>
              <Field label="H"><input type="number" value={sel.h} onChange={e => update(sel.id, { h: +e.target.value })} className={inputCls} /></Field>
            </div>
            {onEditHandler && (
              <div className="pt-1">
                <span className="text-[#8e8e8e] block mb-1">{t('form_events', { defaultValue: 'Événements' })}</span>
                {eventsOf(sel.type).map(ev => (
                  <button key={ev} onClick={() => onEditHandler(sel, ev)} className="flex items-center gap-1.5 text-[#5a9bdc] hover:bg-[#3c3c3c] px-1.5 py-1 rounded w-full">
                    <Code2 size={13} /> {sel.name}_{ev}
                  </button>
                ))}
              </div>
            )}
            <button onClick={() => remove(sel.id)} className="flex items-center gap-1.5 text-[#e84a4a] hover:bg-[#3c3c3c] px-1.5 py-1 rounded w-full"><Trash2 size={13} /> {t('common_delete', { defaultValue: 'Supprimer' })}</button>
          </div>
        ) : (
          <div className="text-[#7e7e7e]">{t('form_no_selection', { defaultValue: 'Sélectionnez un contrôle, ou ajoutez-en un depuis la boîte à outils.' })}</div>
        )}
        <div className="mt-3 pt-2 border-t border-[#3c3c3c]">
          <div className="font-medium text-[#9e9e9e] uppercase tracking-wide text-[10px] mb-1.5">{t('form_size', { defaultValue: 'Formulaire' })}</div>
          <div className="grid grid-cols-2 gap-1.5">
            <Field label="L"><input type="number" value={width} onChange={e => onChange(controls, Math.max(120, +e.target.value), height)} className={inputCls} /></Field>
            <Field label="H"><input type="number" value={height} onChange={e => onChange(controls, width, Math.max(80, +e.target.value))} className={inputCls} /></Field>
          </div>
        </div>
      </div>

      {ctxMenu && <MenuDropdown items={ctxItems()} pos={{ top: ctxMenu.y, left: ctxMenu.x }} onClose={() => setCtxMenu(null)} theme="dark" />}
    </div>
  )
}

const inputCls = 'w-full bg-[#1e1e1e] border border-[#3c3c3c] rounded px-1.5 py-1 text-[#e8e8e8] outline-none focus:border-[#5a9bdc]'
function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="block"><span className="text-[#8e8e8e] block mb-0.5">{label}</span>{children}</label>
}

// Aperçu (mode conception) d'un contrôle.
function ControlPreview({ c }: { c: FormControl }) {
  if (c.type === 'label') return <div className="w-full h-full flex items-center text-[13px] text-[#202124] px-0.5 overflow-hidden">{c.text}</div>
  if (c.type === 'button') return <div className="w-full h-full flex items-center justify-center text-[13px] bg-[#e0e0e0] border border-[#adadad] rounded-sm text-[#202124] overflow-hidden">{c.text}</div>
  if (c.type === 'checkbox') return <div className="w-full h-full flex items-center gap-1.5 text-[13px] text-[#202124] overflow-hidden"><input type="checkbox" readOnly checked={!!c.value} className="pointer-events-none" /> {c.text}</div>
  return <div className="w-full h-full bg-white border border-[#adadad] rounded-sm text-[13px] text-[#202124] px-1 flex items-center overflow-hidden">{c.text}</div>
}
