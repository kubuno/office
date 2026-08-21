import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input } from '@ui'
import { FolderKanban, Cloud, ArrowLeft, Check } from 'lucide-react'
import { projectsApi, type Project } from './api'

interface Props {
  onClose:   () => void
  onCreated: (p: Project) => void
}

// URL-safe identifier preview — mirrors the backend `slugify` so the user sees the
// same value that will be stored. Kept deliberately in sync with projects.rs.
function slugify(name: string): string {
  let out = ''
  let prevDash = false
  // Fold diacritics first (é → e) so accented names give clean identifiers,
  // matching the backend's `fold_accent`.
  const src = name.toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '')
  for (const ch of src) {
    if (/[a-z0-9]/.test(ch)) { out += ch; prevDash = false }
    else if (out.length && !prevDash) { out += '-'; prevDash = true }
  }
  out = out.replace(/^-+|-+$/g, '')
  if (!/^[a-z]/.test(out)) out = `p-${out}`
  return out.slice(0, 30).replace(/-+$/, '')
}

type TypeDef = {
  kind:  'management' | 'cloud'
  Icon:  typeof FolderKanban
  title: string
  desc:  string
}

export default function NewProjectDialog({ onClose, onCreated }: Props) {
  const { t } = useTranslation('office')
  const [step, setStep]   = useState<'pick' | 'cloud'>('pick')
  const [name, setName]   = useState('')
  // `null` = the identifier tracks the name (derived); a string = user overrode it.
  const [slugOverride, setSlugOverride] = useState<string | null>(null)
  const [editingSlug, setEditingSlug]   = useState(false)
  const [busy, setBusy]   = useState(false)

  const TYPES: TypeDef[] = [
    {
      kind: 'management', Icon: FolderKanban,
      title: t('proj_type_mgmt', { defaultValue: 'Projet de gestion' }),
      desc:  t('proj_type_mgmt_desc', { defaultValue: 'Sert à : planning, jalons, chemin critique, ressources et suivi d’avancement.' }),
    },
    {
      kind: 'cloud', Icon: Cloud,
      title: t('proj_type_cloud', { defaultValue: 'Projet cloud' }),
      desc:  t('proj_type_cloud_desc', { defaultValue: 'Sert à : regrouper des ressources et des modules, gérer les accès et la consommation.' }),
    },
  ]

  const effectiveSlug = slugOverride ?? slugify(name || 'projet')

  const pick = async (kind: 'management' | 'cloud') => {
    if (kind === 'cloud') { setStep('cloud'); return }
    // Management projects open straight into the editor; the title is edited in
    // place there (consistent with "the title is the file name"). No form needed.
    setBusy(true)
    try {
      const p = await projectsApi.create({ title: t('proj_new_project'), kind: 'management' })
      onCreated(p)
    } finally { setBusy(false) }
  }

  const createCloud = async () => {
    setBusy(true)
    try {
      const p = await projectsApi.create({
        title: name.trim() || t('proj_new_project'),
        kind:  'cloud',
        slug:  slugOverride ?? undefined,
      })
      onCreated(p)
    } finally { setBusy(false) }
  }

  return (
    <FloatingWindow
      title={step === 'pick'
        ? t('proj_new_title', { defaultValue: 'Nouveau projet' })
        : t('proj_new_cloud_title', { defaultValue: 'Nouveau projet cloud' })}
      onClose={onClose}
      defaultWidth={640}
      backdrop
    >
      {step === 'pick' ? (
        <div className="p-5">
          <p className="text-sm text-text-secondary mb-4">
            {t('proj_new_pick_hint', { defaultValue: 'Choisissez le type de projet à créer.' })}
          </p>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {TYPES.map(({ kind, Icon, title, desc }) => (
              <button
                key={kind}
                disabled={busy}
                onClick={() => pick(kind)}
                className="text-left p-4 rounded-xl border border-border bg-white hover:shadow-md
                           hover:border-border-strong transition-all disabled:opacity-50
                           focus:outline-none focus:ring-2 focus:ring-[var(--color-primary)]"
              >
                <div className="w-10 h-10 rounded-lg flex items-center justify-center mb-3 bg-surface-2">
                  <Icon size={20} style={{ color: 'var(--color-primary)' }} />
                </div>
                <p className="text-sm font-medium text-text-primary mb-1">{title}</p>
                <p className="text-xs text-text-secondary leading-relaxed">{desc}</p>
              </button>
            ))}
          </div>
        </div>
      ) : (
        <div className="p-5 flex flex-col gap-4">
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              {t('proj_field_name', { defaultValue: 'Nom du projet' })}
            </label>
            <Input
              autoFocus
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder={t('proj_new_project')}
            />
            {/* Derived, immutable identifier — the counterpart of a cloud project ID. */}
            <div className="mt-2 text-xs text-text-tertiary flex items-center gap-1.5 flex-wrap">
              <span>{t('proj_field_id', { defaultValue: 'Identifiant' })} :</span>
              {editingSlug ? (
                <input
                  value={effectiveSlug}
                  onChange={e => setSlugOverride(slugify(e.target.value))}
                  onBlur={() => setEditingSlug(false)}
                  autoFocus
                  className="px-1.5 py-0.5 border border-border rounded font-mono text-text-primary
                             focus:outline-none focus:ring-1 focus:ring-[var(--color-primary)]"
                />
              ) : (
                <code className="font-mono text-text-secondary">{effectiveSlug}</code>
              )}
              <span>·</span>
              <span>{t('proj_id_immutable', { defaultValue: 'ne pourra pas être modifié ensuite.' })}</span>
              {!editingSlug && (
                <button
                  onClick={() => setEditingSlug(true)}
                  className="text-[var(--color-primary)] hover:underline font-medium"
                >
                  {t('common_edit', { defaultValue: 'Modifier' })}
                </button>
              )}
            </div>
          </div>

          <div className="flex items-center justify-between gap-2 pt-2">
            <Button variant="ghost" icon={<ArrowLeft size={14} />} onClick={() => setStep('pick')} disabled={busy}>
              {t('common_back', { defaultValue: 'Retour' })}
            </Button>
            <Button icon={<Check size={14} />} onClick={createCloud} loading={busy}>
              {t('proj_create', { defaultValue: 'Créer le projet' })}
            </Button>
          </div>
        </div>
      )}
    </FloatingWindow>
  )
}
