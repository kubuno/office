import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Dropdown, Checkbox } from '@ui'
import { projectsApi, type ProjectArtifactKey, type ProjectMethodology } from '../api'
import ProjectCalendarSection from './ProjectCalendarSection'

const METHODOLOGIES: ProjectMethodology[] = ['predictive', 'agile', 'hybrid']

const ARTIFACTS: ProjectArtifactKey[] = ['charter', 'wbs', 'deliverables', 'requirements', 'risks', 'issues', 'costs', 'stakeholders', 'quality', 'communications', 'decisions', 'changes', 'closure', 'procurement', 'schedule', 'board', 'calendar', 'workload', 'network', 'roadmap', 'baselines', 'timelog']

// Artifacts that render a *view* of the project. Switching every one of them off
// would leave the editor with nothing to open, so the last one standing is locked.
const VIEW_ARTIFACTS: ProjectArtifactKey[] = ['schedule', 'board', 'calendar', 'workload', 'network', 'roadmap']

export default function ProjectSettingsPanel({ projectId }: { projectId: string }) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()

  const { data: settings, isLoading } = useQuery({
    queryKey: ['project-settings', projectId],
    queryFn: () => projectsApi.getSettings(projectId),
  })

  const updateMut = useMutation({
    mutationFn: (data: Parameters<typeof projectsApi.updateSettings>[1]) => projectsApi.updateSettings(projectId, data),
    onSuccess: () => {
      // The methodology drives the opening view, which the editor reads from the project.
      qc.invalidateQueries({ queryKey: ['project-settings', projectId] })
      qc.invalidateQueries({ queryKey: ['project', projectId] })
    },
  })

  const methodLabel = (m: ProjectMethodology) => ({
    predictive: t('proj_method_predictive', { defaultValue: 'Prédictive (planning)' }),
    agile:      t('proj_method_agile',      { defaultValue: 'Agile (tableau)' }),
    hybrid:     t('proj_method_hybrid',     { defaultValue: 'Hybride' }),
  }[m])

  const artifactLabel = (k: ProjectArtifactKey) => ({
    charter:      t('proj_artifact_charter',      { defaultValue: 'Charte' }),
    wbs:          t('proj_artifact_wbs',          { defaultValue: 'Découpage (WBS)' }),
    deliverables: t('proj_artifact_deliverables', { defaultValue: 'Livrables' }),
    requirements: t('proj_artifact_requirements', { defaultValue: 'Exigences' }),
    risks:        t('proj_artifact_risks',        { defaultValue: 'Risques' }),
    issues:       t('proj_artifact_issues',       { defaultValue: 'Incidents' }),
    costs:        t('proj_artifact_costs',        { defaultValue: 'Coûts' }),
    stakeholders: t('proj_artifact_stakeholders', { defaultValue: 'Parties prenantes' }),
    quality:      t('proj_artifact_quality',      { defaultValue: 'Qualité' }),
    communications: t('proj_artifact_comms',     { defaultValue: 'Communications' }),
    decisions:      t('proj_artifact_decisions', { defaultValue: 'Décisions' }),
    changes:        t('proj_artifact_changes',   { defaultValue: 'Changements' }),
    closure:        t('proj_artifact_closure',   { defaultValue: 'Clôture' }),
    procurement:    t('proj_artifact_procurement', { defaultValue: 'Contrats' }),
    schedule:  t('proj_artifact_schedule',  { defaultValue: 'Planning' }),
    board:     t('proj_artifact_board',     { defaultValue: 'Tableau' }),
    calendar:  t('proj_artifact_calendar',  { defaultValue: 'Calendrier' }),
    workload:  t('proj_artifact_workload',  { defaultValue: 'Charge' }),
    network:   t('proj_artifact_network',   { defaultValue: 'Réseau' }),
    roadmap:   t('proj_artifact_roadmap',   { defaultValue: 'Roadmap' }),
    baselines: t('proj_artifact_baselines', { defaultValue: 'Plans de référence' }),
    timelog:   t('proj_artifact_timelog',   { defaultValue: 'Journal de temps' }),
  }[k])

  const artifactHint = (k: ProjectArtifactKey) => ({
    charter:      t('proj_artifact_charter_hint',      { defaultValue: 'Objet, sponsor, critères de succès' }),
    wbs:          t('proj_artifact_wbs_hint',          { defaultValue: 'Arborescence numérotée et dictionnaire' }),
    deliverables: t('proj_artifact_deliverables_hint', { defaultValue: 'Suivis jusqu’à leur acceptation' }),
    requirements: t('proj_artifact_requirements_hint', { defaultValue: 'Et leur matrice de traçabilité' }),
    risks:        t('proj_artifact_risks_hint',        { defaultValue: 'Registre et matrice probabilité/impact' }),
    issues:       t('proj_artifact_issues_hint',       { defaultValue: 'Ce qui se produit en ce moment' }),
    costs:        t('proj_artifact_costs_hint',        { defaultValue: 'Budget, dépenses et valeur acquise' }),
    stakeholders: t('proj_artifact_stakeholders_hint', { defaultValue: 'Registre, grille pouvoir/intérêt et RACI' }),
    quality:      t('proj_artifact_quality_hint',      { defaultValue: 'Indicateurs, contrôles et coût de la qualité' }),
    communications: t('proj_artifact_comms_hint',     { defaultValue: 'Qui reçoit quoi, et qui ne reçoit rien' }),
    decisions:      t('proj_artifact_decisions_hint', { defaultValue: 'Ce qui a été tranché, et pourquoi' }),
    changes:        t('proj_artifact_changes_hint',   { defaultValue: 'Demandes, évaluation d’impact et décision' }),
    closure:        t('proj_artifact_closure_hint',   { defaultValue: 'Ce qui reste à régler, et ce que le projet a appris' }),
    procurement:    t('proj_artifact_procurement_hint', { defaultValue: 'Ce qu’on achète, et qui porte le risque' }),
    schedule:  t('proj_artifact_schedule_hint',  { defaultValue: 'Diagramme de Gantt et chemin critique' }),
    board:     t('proj_artifact_board_hint',     { defaultValue: 'Colonnes par statut' }),
    calendar:  t('proj_artifact_calendar_hint',  { defaultValue: 'Les tâches par date' }),
    workload:  t('proj_artifact_workload_hint',  { defaultValue: 'Occupation des ressources' }),
    network:   t('proj_artifact_network_hint',   { defaultValue: 'Diagramme des dépendances (PERT)' }),
    roadmap:   t('proj_artifact_roadmap_hint',   { defaultValue: 'Versions de livraison' }),
    baselines: t('proj_artifact_baselines_hint', { defaultValue: 'Comparer au plan initial' }),
    timelog:   t('proj_artifact_timelog_hint',   { defaultValue: 'Heures passées par tâche' }),
  }[k])

  if (isLoading || !settings) {
    return (
      <div className="flex-1 overflow-y-auto bg-surface-1">
        <div className="max-w-2xl mx-auto p-6">
          <div className="bg-surface-0 border border-border rounded-xl p-5 text-sm text-text-tertiary">
            {t('common_loading', { defaultValue: 'Chargement…' })}
          </div>
        </div>
      </div>
    )
  }

  const enabledViews = VIEW_ARTIFACTS.filter(k => settings.artifacts[k]?.enabled)

  const toggle = (key: ProjectArtifactKey, enabled: boolean) => {
    // Send the artifact's current config back, otherwise the server resets it to {}.
    updateMut.mutate({ artifacts: { [key]: { enabled, config: settings.artifacts[key]?.config ?? {} } } })
  }

  return (
    <div className="flex-1 overflow-y-auto bg-surface-1">
      <div className="max-w-2xl mx-auto p-6 space-y-4">
        <h1 className="text-xl font-semibold text-text-primary">{t('proj_settings', { defaultValue: 'Réglages du projet' })}</h1>

        {/* Methodology — decides the view the project opens on. */}
        <div className="bg-surface-0 border border-border rounded-xl p-5">
          <h2 className="text-sm font-semibold text-text-primary">{t('proj_settings_method', { defaultValue: 'Méthode' })}</h2>
          <p className="text-xs text-text-tertiary mt-0.5 mb-3">
            {t('proj_settings_method_hint', { defaultValue: 'La méthode détermine la vue sur laquelle le projet s’ouvre : le planning pour un projet prédictif, le tableau pour un projet agile.' })}
          </p>
          <Dropdown
            className="w-full" height={36} fontSize={14} focusable
            value={settings.methodology}
            onChange={v => updateMut.mutate({ methodology: v as ProjectMethodology })}
            options={METHODOLOGIES.map(m => ({ value: m, label: methodLabel(m) }))}
          />
        </div>

        {/* Artifacts — what this project actually uses. */}
        <div className="bg-surface-0 border border-border rounded-xl p-5">
          <h2 className="text-sm font-semibold text-text-primary">{t('proj_settings_artifacts', { defaultValue: 'Ce que ce projet utilise' })}</h2>
          <p className="text-xs text-text-tertiary mt-0.5 mb-3">
            {t('proj_settings_artifacts_hint', { defaultValue: 'Décochez ce dont vous n’avez pas besoin : l’interface n’en propose que le reste.' })}
          </p>
          <ul className="space-y-3">
            {ARTIFACTS.map(k => {
              const enabled = settings.artifacts[k]?.enabled ?? false
              // The last remaining view cannot be switched off: the project would have nothing to show.
              const isLastView = enabled && VIEW_ARTIFACTS.includes(k) && enabledViews.length === 1
              // Label and description belong to the Checkbox itself: laid out beside
              // it they sit outside its <label>, so clicking the text — the natural
              // gesture — did nothing at all.
              return (
                <li key={k} title={isLastView ? t('proj_settings_last_view', { defaultValue: 'Un projet doit garder au moins une vue. Activez-en une autre pour pouvoir désactiver celle-ci.' }) : undefined}>
                  <Checkbox
                    checked={enabled}
                    disabled={isLastView || updateMut.isPending}
                    onChange={(checked: boolean) => toggle(k, checked)}
                    label={artifactLabel(k)}
                    description={artifactHint(k)}
                  />
                </li>
              )
            })}
          </ul>
        </div>

        {/* Working calendar — only where it changes anything: a project that does
            not use the schedule has no use for worked days. */}
        {settings.artifacts.schedule?.enabled && <ProjectCalendarSection projectId={projectId} />}
      </div>
    </div>
  )
}
