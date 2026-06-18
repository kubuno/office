import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input } from '@ui'
import { Tag, Pencil, Trash2, Plus } from 'lucide-react'
import { validateNameFormula, isValidDefinedName } from './formula-engine'

interface NameManagerDialogProps {
  names:          Record<string, string>
  onSet:          (name: string, def: string) => void
  onDelete:       (name: string) => void
  onClose:        () => void
  // Current selection in A1 notation (e.g. "A1:C3"), used to pre-fill "name the
  // selection" when the dialog is opened with no row being edited.
  selectionRef?:  string
}

// Workbook defined-names manager (named ranges / values / LAMBDAs), modelled on
// Excel's Name Manager. Names are case-insensitive and stored uppercase.
export default function NameManagerDialog({ names, onSet, onDelete, onClose, selectionRef }: NameManagerDialogProps) {
  const { t } = useTranslation('office')
  const entries = Object.entries(names).sort(([a], [b]) => a.localeCompare(b))

  // Edit/add form state. `editing` = the original name being edited (null = new).
  const [editing, setEditing] = useState<string | null>(null)
  const [nameField, setNameField] = useState('')
  const [defField, setDefField] = useState(selectionRef ? `=${selectionRef}` : '')
  const [error, setError] = useState<string | null>(null)

  const startEdit = (name: string) => {
    setEditing(name); setNameField(name); setDefField(names[name]); setError(null)
  }
  const startNew = () => {
    setEditing(null); setNameField(''); setDefField(selectionRef ? `=${selectionRef}` : ''); setError(null)
  }

  const submit = () => {
    const name = nameField.trim()
    if (!isValidDefinedName(name)) {
      setError(t('names_err_name', { defaultValue: 'Nom invalide (lettres/chiffres/_, ne doit pas ressembler à une référence A1).' }))
      return
    }
    let def = defField.trim()
    if (!def.startsWith('=')) def = `=${def}`
    const verr = validateNameFormula(def)
    if (verr) { setError(t('names_err_def', { defaultValue: 'Définition invalide : {{e}}', e: verr })); return }
    // Renaming: drop the old key.
    if (editing && editing.toUpperCase() !== name.toUpperCase()) onDelete(editing)
    onSet(name, def)
    startNew()
  }

  return (
    <FloatingWindow
      title={t('names_title', { defaultValue: 'Gestionnaire de noms' })}
      icon={<Tag size={16} />}
      onClose={onClose}
      backdrop
      resizable
      defaultWidth={560}
      defaultHeight={460}
      minWidth={420}
      minHeight={320}
    >
      <div className="flex flex-col h-full text-sm" data-module="office">
        {/* Existing names */}
        <div className="flex-1 overflow-auto border border-border rounded">
          {entries.length === 0 ? (
            <div className="p-6 text-center text-text-tertiary">
              {t('names_empty', { defaultValue: 'Aucun nom défini. Créez-en un ci-dessous.' })}
            </div>
          ) : (
            <table className="w-full border-collapse">
              <thead className="sticky top-0 bg-surface-1 text-text-secondary text-xs">
                <tr>
                  <th className="text-left font-medium px-3 py-2">{t('names_col_name', { defaultValue: 'Nom' })}</th>
                  <th className="text-left font-medium px-3 py-2">{t('names_col_def', { defaultValue: 'Fait référence à' })}</th>
                  <th className="px-2 py-2 w-16"></th>
                </tr>
              </thead>
              <tbody>
                {entries.map(([name, def]) => (
                  <tr key={name} className="border-t border-border hover:bg-surface-1">
                    <td className="px-3 py-1.5 font-medium">{name}</td>
                    <td className="px-3 py-1.5 font-mono text-xs text-text-secondary truncate max-w-[260px]" title={def}>{def}</td>
                    <td className="px-2 py-1.5">
                      <div className="flex gap-1 justify-end">
                        <button className="p-1 rounded hover:bg-surface-2 text-text-secondary" title={t('names_edit', { defaultValue: 'Modifier' })} onClick={() => startEdit(name)}>
                          <Pencil size={14} />
                        </button>
                        <button className="p-1 rounded hover:bg-danger-light text-danger" title={t('names_delete', { defaultValue: 'Supprimer' })} onClick={() => onDelete(name)}>
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {/* Add / edit form */}
        <div className="mt-3 pt-3 border-t border-border space-y-2">
          <div className="text-xs font-medium text-text-secondary">
            {editing ? t('names_editing', { defaultValue: 'Modifier « {{n}} »', n: editing }) : t('names_new', { defaultValue: 'Nouveau nom' })}
          </div>
          <div className="flex gap-2">
            <div className="w-40">
              <Input
                label={t('names_col_name', { defaultValue: 'Nom' })}
                value={nameField}
                onChange={e => { setNameField(e.target.value); setError(null) }}
                placeholder="TauxTVA"
              />
            </div>
            <div className="flex-1">
              <Input
                label={t('names_col_def', { defaultValue: 'Fait référence à' })}
                value={defField}
                onChange={e => { setDefField(e.target.value); setError(null) }}
                placeholder="=Feuille1!A1:A10  ·  =0.2  ·  =LAMBDA(x, x*2)"
                className="font-mono"
                onKeyDown={e => { if (e.key === 'Enter') submit() }}
              />
            </div>
          </div>
          {error && <div className="text-xs text-danger">{error}</div>}
          <div className="flex gap-2 justify-end">
            {editing && (
              <Button variant="ghost" onClick={startNew}>{t('names_cancel', { defaultValue: 'Annuler' })}</Button>
            )}
            <Button variant="primary" onClick={submit}>
              {editing ? <>{t('names_save', { defaultValue: 'Enregistrer' })}</> : <><Plus size={14} /> {t('names_add', { defaultValue: 'Ajouter' })}</>}
            </Button>
          </div>
        </div>
      </div>
    </FloatingWindow>
  )
}
