import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { Input, Textarea, Dropdown } from '@ui'
import { CalendarRange, Flag } from 'lucide-react'
import type { Project } from '../api'

const STATUS = ['active', 'on_hold', 'completed', 'cancelled'] as const
// A small palette for the project accent colour.
const COLORS = ['#1a73e8', '#188038', '#e37400', '#d93025', '#9334e6', '#00897b', '#c2185b', '#5f6368']

// The project's own properties — description, accent colour, status and the planned
// start/finish dates — edited where they belong (the File ▸ Properties backstage),
// instead of being set only at creation.
export default function ProjectPropertiesPanel({ project, onSave }: {
  project: Project
  onSave: (patch: Partial<Pick<Project, 'description' | 'color' | 'status' | 'start_date' | 'end_date'>>) => void
}) {
  const { t } = useTranslation('office')
  const [desc, setDesc] = useState(project.description ?? '')
  useEffect(() => { setDesc(project.description ?? '') }, [project.description])

  return (
    <div className="max-w-2xl mx-auto p-6 space-y-6">
      {/* Planning dates — the start anchors the whole schedule */}
      <section>
        <h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary mb-1">
          <CalendarRange size={16} className="text-text-secondary" />
          {t('proj_props_planning', { defaultValue: 'Planification' })}
        </h3>
        <p className="text-xs text-text-tertiary mb-3">
          {t('proj_props_planning_hint', { defaultValue: 'La date de début ancre le planning (jour 0 du Gantt) ; la date de fin est un objectif comparé à la fin calculée.' })}
        </p>
        <div className="flex flex-wrap gap-4">
          <label className="text-xs text-text-secondary">
            <span className="block mb-1">{t('proj_props_start', { defaultValue: 'Date de début prévue' })}</span>
            <Input type="date" value={project.start_date ?? ''}
              onChange={e => onSave({ start_date: e.target.value || null })} />
          </label>
          <label className="text-xs text-text-secondary">
            <span className="block mb-1">{t('proj_props_end', { defaultValue: 'Date de fin cible' })}</span>
            <Input type="date" value={project.end_date ?? ''}
              onChange={e => onSave({ end_date: e.target.value || null })} />
          </label>
        </div>
      </section>

      {/* Status */}
      <section>
        <h3 className="flex items-center gap-2 text-sm font-semibold text-text-primary mb-2">
          <Flag size={16} className="text-text-secondary" />
          {t('proj_props_status', { defaultValue: 'Statut du projet' })}
        </h3>
        <Dropdown value={project.status} onChange={v => onSave({ status: v as Project['status'] })}
          options={STATUS.map(s => ({ value: s, label: t('proj_pstatus_' + s, { defaultValue: statusLabel(s) }) }))} />
      </section>

      {/* Accent colour */}
      <section>
        <h3 className="text-sm font-semibold text-text-primary mb-2">{t('proj_props_color', { defaultValue: 'Couleur' })}</h3>
        <div className="flex flex-wrap gap-2">
          {COLORS.map(c => (
            <button key={c} onClick={() => onSave({ color: c })} aria-label={c}
              className={`w-7 h-7 rounded-full transition ${project.color === c ? 'ring-2 ring-offset-2 ring-offset-surface-0 ring-text-primary' : 'hover:scale-110'}`}
              style={{ background: c }} />
          ))}
        </div>
      </section>

      {/* Description */}
      <section>
        <h3 className="text-sm font-semibold text-text-primary mb-2">{t('proj_props_desc', { defaultValue: 'Description' })}</h3>
        <Textarea rows={4} value={desc} onChange={e => setDesc(e.target.value)}
          onBlur={() => { if (desc !== (project.description ?? '')) onSave({ description: desc }) }}
          placeholder={t('proj_props_desc_ph', { defaultValue: 'Objet, périmètre, contexte du projet…' })} />
      </section>
    </div>
  )
}

function statusLabel(s: string): string {
  return s === 'active' ? 'Actif' : s === 'on_hold' ? 'En pause' : s === 'completed' ? 'Terminé' : 'Annulé'
}
