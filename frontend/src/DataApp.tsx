import { useState, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  BarChart3, Plus, Trash2, Star, RefreshCw,
  LayoutDashboard, Database, Network, Eye, Share2,
  Table, TrendingUp, AlertCircle, Check, ExternalLink, Copy,
} from 'lucide-react'
import clsx from 'clsx'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Button, Input, Dropdown } from '@ui'
import type { StartPageRecentItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import {
  reportsApi, datasetsApi, datasourcesApi, modelApi,
  type Report, type Dataset, type Relation, type Measure,
} from './data-api'
import { DataReportEditor } from './DataReportEditor'
import type { RibbonTab } from './ribbon/types'
import { OfficeShell } from './shell/OfficeShell'
import { SaveButton } from './ribbon/SaveButton'
import { MacrosMenu } from './macros/MacrosMenu'
import { THEME_DATA } from './ribbon/officeThemes'
import { ModuleHome, useFileTab, backstageLabels, InfoPanel } from './ribbon/ModuleBackstage'

// ── Types ─────────────────────────────────────────────────────────────────────

type DataView = 'report' | 'query' | 'model'

// ── Main Shell (route-aware: /office/data and /office/data/:id) ────────────────

export default function DataApp() {
  const { id } = useParams<{ id?: string }>()
  const navigate = useNavigate()
  const [view, setView] = useState<DataView>('report')

  const openReport = (rid: string) => { setView('report'); navigate(`/office/data/${rid}`) }
  const backToList = () => navigate('/office/data')

  if (!id) return <DataReportsList onOpenReport={openReport} />
  return (
    <DataReportShell
      reportId={id}
      view={view}
      onViewChange={setView}
      onBack={backToList}
      onOpenReport={openReport}
    />
  )
}

// ── Start content (Accueil) — réutilisé par la page d'accueil ET le backstage de
//    l'éditeur ouvert (onglet « Fichier »). Récents + parcourir + Nouveau. ──────────
export function DataStartContent({ onOpenReport }: { onOpenReport: (id: string) => void }) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()

  const { data } = useQuery({
    queryKey: ['data-reports', 'all', ''],
    queryFn: () => reportsApi.list({}),
  })

  const createMut = useMutation({
    mutationFn: () => reportsApi.create({ title: t('data_new_report') }),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['data-reports'] }); onOpenReport(d.report.id) },
  })
  const trashMut = useMutation({
    mutationFn: (id: string) => reportsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['data-reports'] }),
  })
  const dupMut = useMutation({
    mutationFn: (id: string) => reportsApi.duplicate(id),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['data-reports'] }); onOpenReport(d.report.id) },
  })

  const reports = data?.reports ?? []

  // Recents = reports (opened by id). The "Browse" tab lists the .kbdrp/.kbdst
  // files under Office/Data and opens them via the FileTypeRegistry handler.
  const recentItems: StartPageRecentItem[] = reports.slice(0, 12).map(r => ({
    id:       r.id,
    name:     r.title,
    subtitle: format(new Date(r.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
    icon:     <BarChart3 size={18} className="text-text-tertiary" strokeWidth={1.5} />,
    onClick:  () => onOpenReport(r.id),
    actions: [
      { id: 'open',  label: t('common_open', { defaultValue: 'Ouvrir' }), icon: <ExternalLink size={15} />, onClick: () => onOpenReport(r.id) },
      { id: 'dup',   label: t('common_duplicate'),                        icon: <Copy size={15} />,         onClick: () => dupMut.mutate(r.id) },
      { id: 'trash', label: t('data_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => trashMut.mutate(r.id) },
    ],
  }))

  return (
    <ModuleStartPage
      recentTitle={t('data_recent', { defaultValue: 'Récents' })}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <BarChart3 size={32} className="text-text-tertiary opacity-30" strokeWidth={1.5} />
          <p className="text-text-tertiary text-xs">{t('data_create_first_report')}</p>
        </div>
      }
      browse={{
        folderPathPrefix: 'Office/Data',
        title: t('data_title'),
        fileTypeModuleId: 'office-data',
        toolbarContent: (
          <Button icon={<Plus size={15} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('data_new_report')}
          </Button>
        ),
      }}
    />
  )
}

// ── Reports List (page d'accueil sans rapport ouvert) ──────────────────────────
function DataReportsList({ onOpenReport }: { onOpenReport: (id: string) => void }) {
  const { t } = useTranslation('office')
  const navigate = useNavigate()
  return (
    <ModuleHome
      theme={THEME_DATA}
      title={t('data_title')}
      titleIcon={<BarChart3 size={16} className="text-white/90 flex-shrink-0" />}
      fileLabel={t('office_bs_file', { defaultValue: 'Fichier' })}
      homeLabel={t('office_bs_home', { defaultValue: 'Accueil' })}
      onBack={() => navigate('/office')}
      startContent={<DataStartContent onOpenReport={onOpenReport} />}
    />
  )
}

// ── Report Shell (Editor + Query + Model tabs) ────────────────────────────────

function DataReportShell({ reportId, view, onViewChange, onBack, onOpenReport }: {
  reportId: string
  view: DataView
  onViewChange: (v: DataView) => void
  onBack: () => void
  onOpenReport: (id: string) => void
}) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()

  const { data, isLoading } = useQuery({
    queryKey: ['data-report', reportId],
    queryFn: () => reportsApi.get(reportId),
  })

  const report = data?.report

  // ── Editable title (standard WorkspaceShell) — synced from the report ──
  const [titleDraft, setTitleDraft] = useState('')
  useEffect(() => { if (report?.title != null) setTitleDraft(report.title) }, [report?.title])

  const updateMut = useMutation({
    mutationFn: (upd: Partial<Report>) => reportsApi.update(reportId, upd),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['data-report', reportId] }),
  })
  const trashMut = useMutation({
    mutationFn: () => reportsApi.trash(reportId),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['data-reports'] }); onBack() },
  })
  const createMut = useMutation({
    mutationFn: () => reportsApi.create({ title: t('data_new_report') }),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['data-reports'] }); onOpenReport(d.report.id) },
  })
  const dupMut = useMutation({
    mutationFn: () => reportsApi.duplicate(reportId),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['data-reports'] }); onOpenReport(d.report.id) },
  })
  const commitTitle = () => {
    const v = titleDraft.trim()
    if (v && v !== report?.title) updateMut.mutate({ title: v })
    else if (!v && report?.title) setTitleDraft(report.title)
  }

  const pages = data?.pages ?? []
  const widgets = data?.widgets ?? []

  // Onglet « Fichier » (backstage façon Office) — TOUJOURS en 1ʳᵉ position du ruban,
  // partagé par les trois vues (rapport/données/modèle) via `renderShell`.
  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_DATA,
    labels: backstageLabels(t),
    startContent: <DataStartContent onOpenReport={onOpenReport} />,
    doc: {
      info: (
        <InfoPanel
          title={report?.title || t('common_untitled', { defaultValue: 'Sans titre' })}
          subtitle={t('data_title')}
          rows={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('data_title')],
            [t('data_new_visual', { defaultValue: 'Visuels' }), widgets.length],
            [t('data_tab_report', { defaultValue: 'Pages' }), pages.length],
            ...(report?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(report.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: onBack,
    },
  })

  // Read-only API surface exposed to macros (global `Kubuno`) for the Data report.
  const makeApi = () => {
    const Data = {
      getReportName: () => report?.title ?? '',
      getWidgetCount: () => widgets.length,
      getPageCount: () => pages.length,
    }
    const App = {
      getType: () => 'data',
      getId: () => reportId,
      toast: (msg: unknown) => console.log(String(msg)),
      log: (msg: unknown) => console.log(String(msg)),
    }
    return { Data, App }
  }

  if (isLoading || !report) {
    return (
      <div className="flex items-center justify-center h-full text-[#9aa0a6] text-sm">
        {t('common_loading')}
      </div>
    )
  }

  const TABS = [
    { id: 'report' as DataView, icon: LayoutDashboard, label: t('data_tab_report') },
    { id: 'query'  as DataView, icon: Database,        label: t('data_tab_data') },
    { id: 'model'  as DataView, icon: Network,         label: t('data_tab_model') },
  ]

  // Single OfficeShell wrapper reused by all three views. The report editor builds
  // its own (rich) ribbon; query/model use the basic file ribbon.
  const renderShell = (ribbon: RibbonTab[], body: React.ReactNode) => (
    <OfficeShell
      ribbon={[fileTab, ...ribbon]}
      activeTabId={activeTabId}
      onTabChange={onTabChange}
      theme={THEME_DATA}
      chromeless
      topbarHeight={64}
      onBack={onBack}
      titleIcon={<BarChart3 size={16} className="text-white/90 flex-shrink-0" />}
      title={titleDraft}
      onTitleChange={setTitleDraft}
      onTitleCommit={commitTitle}
      titlePlaceholder={t('common_untitled', { defaultValue: 'Sans titre' })}
      saveStatus={updateMut.isPending ? t('data_saving', { defaultValue: 'Enregistrement…' }) : t('doc_saved', { defaultValue: 'Enregistré' })}
      titleActions={(
        <>
          {/* Shared save button (before the star + trash) — persists the report immediately.
              The report body auto-saves each edit via React Query; this forces a touch. */}
          <SaveButton
            onSave={() => updateMut.mutate({ title: titleDraft.trim() || report.title })}
            saving={updateMut.isPending}
            label={t('doc_save', { defaultValue: 'Enregistrer' })}
          />
          <button
            onClick={() => updateMut.mutate({ is_starred: !report.is_starred })}
            title={report.is_starred ? t('data_unstar', { defaultValue: 'Retirer des favoris' }) : t('data_star', { defaultValue: 'Ajouter aux favoris' })}
            className={clsx('p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0', report.is_starred ? 'text-warning' : 'text-white/90')}
          >
            <Star size={15} fill={report.is_starred ? 'currentColor' : 'none'} />
          </button>
        </>
      )}
      onDelete={() => trashMut.mutate()}
      deleteTitle={t('data_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('data_delete_confirm_title', { defaultValue: 'Supprimer ce rapport ?' }),
        message: t('data_delete_confirm_msg', { defaultValue: 'Le rapport sera déplacé dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      }}
      topbarActions={<>
        <div className="flex items-center gap-0.5">
          {TABS.map(tab => (
            <button
              key={tab.id}
              onClick={() => onViewChange(tab.id)}
              className={clsx(
                'flex items-center gap-1.5 px-3 py-1.5 text-sm rounded transition-colors',
                view === tab.id
                  ? 'bg-[#e8f0fe] text-[#1a73e8] font-medium'
                  : 'text-[#5f6368] hover:bg-[#f1f3f4]',
              )}
            >
              <tab.icon size={15} />
              {tab.label}
            </button>
          ))}
        </div>
        <MacrosMenu docType="data" docId={reportId} buildApi={makeApi} defaultLabel={report.title} />
        <Button variant="secondary" size="sm" icon={<Eye size={15} />}>{t('data_preview')}</Button>
        <Button size="sm" icon={<Share2 size={15} />}>{t('data_share')}</Button>
      </>}
    >
      {body}
    </OfficeShell>
  )

  if (view === 'report') {
    return (
      <DataReportEditor
        report={report}
        pages={pages}
        widgets={widgets}
        reportId={reportId}
        renderShell={renderShell}
        onSwitchView={(v) => onViewChange(v)}
      />
    )
  }

  // Nouveau/Dupliquer (jadis dans un groupe « Fichier ») déplacés dans un groupe
  // « Rapport » de l'onglet Accueil ; les opérations sur le fichier vivent désormais
  // dans le backstage (onglet Fichier).
  const basicRibbon: RibbonTab[] = [{
    id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
    groups: [{
      id: 'report', label: t('data_tab_report', { defaultValue: 'Rapport' }),
      items: [
        { id: 'new', kind: 'button', icon: <Plus size={15} />, label: t('doc_new', { defaultValue: 'Nouveau' }), onClick: () => createMut.mutate() },
        { id: 'dup', kind: 'button', icon: <Copy size={15} />, label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), onClick: () => dupMut.mutate() },
      ],
    }],
  }]

  return renderShell(basicRibbon, (
    <div className="flex-1 min-w-0 overflow-hidden h-full">
      {view === 'query' && <DataQueryEditor reportId={reportId} />}
      {view === 'model' && <DataModelView reportId={reportId} />}
    </div>
  ))
}

// ── Data Query Editor ─────────────────────────────────────────────────────────

function DataQueryEditor({ reportId: _ }: { reportId: string }) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const [activeDatasetId, setActiveDatasetId] = useState<string | null>(null)
  const [sqlDraft, setSqlDraft] = useState('')
  const [previewData, setPreviewData] = useState<{ columns: string[]; rows: Record<string, unknown>[] } | null>(null)
  const [validating, setValidating] = useState(false)
  const [validErr, setValidErr] = useState<string | null>(null)

  const { data: datasetsData } = useQuery({ queryKey: ['data-datasets'], queryFn: datasetsApi.list })
  const { data: dsData } = useQuery({
    queryKey: ['data-sources'],
    queryFn: datasourcesApi.list,
  })
  const datasets = datasetsData?.datasets ?? []
  void dsData
  const activeDs = datasets.find(d => d.id === activeDatasetId)

  const createMut = useMutation({
    mutationFn: () => datasetsApi.create({ name: t('data_new_dataset') }),
    onSuccess: (d) => {
      qc.invalidateQueries({ queryKey: ['data-datasets'] })
      setActiveDatasetId(d.dataset.id)
      setSqlDraft('')
    },
  })

  const refreshMut = useMutation({
    mutationFn: (id: string) => datasetsApi.refresh(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['data-datasets'] }),
  })

  const saveSqlMut = useMutation({
    mutationFn: ({ id, sql }: { id: string; sql: string }) =>
      datasetsApi.update(id, { raw_sql: sql }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['data-datasets'] }),
  })

  const handlePreview = async () => {
    if (!activeDatasetId) return
    try {
      const res = await datasetsApi.preview(activeDatasetId, { limit: 50 })
      const cols = Array.isArray(res.columns)
        ? (typeof res.columns[0] === 'string' ? res.columns as string[] : (res.columns as { name: string }[]).map(c => c.name))
        : []
      setPreviewData({ columns: cols, rows: res.rows })
    } catch {
      setPreviewData(null)
    }
  }

  const handleValidate = async () => {
    if (!activeDatasetId || !sqlDraft.trim()) return
    setValidating(true)
    try {
      const res = await datasetsApi.validateSql(activeDatasetId, sqlDraft)
      setValidErr(res.valid ? null : (res.error ?? t('data_error')))
    } finally {
      setValidating(false)
    }
  }

  return (
    <div className="flex h-full overflow-hidden bg-[#f8f9fa]">
      {/* Dataset list */}
      <div className="w-56 shrink-0 bg-white border-r border-[#e8eaed] flex flex-col">
        <div className="flex items-center justify-between px-3 py-2 border-b border-[#e8eaed]">
          <span className="text-xs font-medium text-[#5f6368] uppercase">{t('data_queries')}</span>
          <button onClick={() => createMut.mutate()} className="p-1 hover:bg-[#f1f3f4] rounded">
            <Plus size={15} className="text-[#5f6368]" />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto py-1">
          {datasets.map(ds => (
            <button
              key={ds.id}
              onClick={() => { setActiveDatasetId(ds.id); setSqlDraft(ds.raw_sql ?? '') }}
              className={clsx(
                'w-full flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors',
                ds.id === activeDatasetId
                  ? 'bg-[#e8f0fe] text-[#1a73e8]'
                  : 'text-[#202124] hover:bg-[#f1f3f4]',
              )}
            >
              <Table size={14} className="shrink-0 text-[#9aa0a6]" />
              <span className="truncate">{ds.name}</span>
              {ds.row_count != null && (
                <span className="ml-auto text-[10px] text-[#9aa0a6] shrink-0">
                  {ds.row_count.toLocaleString()}
                </span>
              )}
            </button>
          ))}
          {datasets.length === 0 && (
            <p className="text-xs text-[#9aa0a6] text-center py-6">{t('data_no_dataset')}</p>
          )}
        </div>
        <div className="p-2 border-t border-[#e8eaed]">
          <button
            onClick={() => createMut.mutate()}
            className="w-full flex items-center gap-2 px-2 py-1.5 text-sm text-[#1a73e8] hover:bg-[#e8f0fe] rounded-lg"
          >
            <Plus size={14} />
            {t('data_new_dataset')}
          </button>
        </div>
      </div>

      {/* Editor area */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {activeDs ? (
          <>
            {/* Toolbar */}
            <div className="flex items-center gap-2 px-4 py-2 bg-white border-b border-[#e8eaed] shrink-0">
              <span className="text-sm font-medium text-[#202124]">{activeDs.name}</span>
              <StatusBadge status={activeDs.status} />
              <div className="flex-1" />
              <Button
                variant="secondary"
                size="sm"
                icon={<Check size={13} />}
                onClick={handleValidate}
                disabled={validating}
                className="text-xs"
              >
                {t('data_validate_sql')}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => saveSqlMut.mutate({ id: activeDs.id, sql: sqlDraft })}
                className="text-xs"
              >
                {t('common_save')}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                icon={<Eye size={13} />}
                onClick={handlePreview}
                className="text-xs"
              >
                {t('data_preview')}
              </Button>
              <Button
                size="sm"
                icon={<RefreshCw size={13} className={refreshMut.isPending ? 'animate-spin' : ''} />}
                onClick={() => refreshMut.mutate(activeDs.id)}
                disabled={refreshMut.isPending}
                className="text-xs"
              >
                {t('data_refresh')}
              </Button>
            </div>

            {/* SQL Editor */}
            <div className="flex-1 flex flex-col overflow-hidden">
              <textarea
                value={sqlDraft}
                onChange={e => setSqlDraft(e.target.value)}
                placeholder={t('data_sql_placeholder')}
                className="flex-1 p-4 font-mono text-sm bg-[#1e1e2e] text-[#cdd6f4] resize-none outline-none"
                spellCheck={false}
              />

              {/* Validation status */}
              {validErr !== null && (
                <div className={clsx(
                  'flex items-center gap-2 px-4 py-2 text-xs shrink-0 border-t',
                  validErr ? 'bg-[#fce8e6] border-[#f28b82] text-[#d93025]' : 'bg-[#e6f4ea] border-[#81c995] text-[#137333]',
                )}>
                  {validErr ? <AlertCircle size={13} /> : <Check size={13} />}
                  {validErr || t('data_sql_valid')}
                </div>
              )}

              {/* Preview table */}
              {previewData && (
                <div className="h-48 border-t border-[#e8eaed] overflow-auto bg-white shrink-0">
                  <table className="w-full text-xs">
                    <thead className="sticky top-0 bg-[#f8f9fa]">
                      <tr>
                        {previewData.columns.map(c => (
                          <th key={c} className="text-left px-3 py-1.5 text-[#5f6368] font-medium border-b border-[#e8eaed] whitespace-nowrap">
                            {c}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {previewData.rows.map((row, i) => (
                        <tr key={i} className={i % 2 === 0 ? 'bg-white' : 'bg-[#f8f9fa]'}>
                          {previewData.columns.map(c => (
                            <td key={c} className="px-3 py-1 border-b border-[#f1f3f4] max-w-[150px] truncate text-[#202124]">
                              {String(row[c] ?? '')}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="flex flex-col items-center justify-center h-full gap-3 text-[#9aa0a6]">
            <Database size={48} className="opacity-30" />
            <p className="text-sm">{t('data_select_or_create_dataset')}</p>
            <Button onClick={() => createMut.mutate()}>
              {t('data_new_dataset')}
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}

// ── Data Model View ───────────────────────────────────────────────────────────

function DataModelView({ reportId: _ }: { reportId: string }) {
  const { t } = useTranslation('office')
  const { data, isLoading } = useQuery({
    queryKey: ['data-model'],
    queryFn: () => modelApi.get(),
  })

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-[#9aa0a6] text-sm">
        {t('data_loading_model')}
      </div>
    )
  }

  const datasets: Dataset[] = data?.datasets ?? []
  const relations: Relation[] = data?.relations ?? []
  const measures: Measure[] = data?.measures ?? []

  return (
    <div className="flex h-full overflow-hidden">
      {/* Datasets list */}
      <div className="flex-1 overflow-auto p-6">
        <h2 className="text-sm font-medium text-[#5f6368] uppercase tracking-wide mb-4">
          {t('data_model_tables', { count: datasets.length })}
        </h2>
        {datasets.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-60 gap-3 text-[#9aa0a6]">
            <Network size={48} className="opacity-30" />
            <p className="text-sm">{t('data_create_datasets_hint')}</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {datasets.map(ds => (
              <div key={ds.id} className="bg-white border border-[#e8eaed] rounded-xl p-4">
                <div className="flex items-center gap-2 mb-3">
                  <Table size={16} className="text-[#1a73e8]" />
                  <span className="text-sm font-medium text-[#202124]">{ds.name}</span>
                  <StatusBadge status={ds.status} />
                </div>
                {ds.schema_cache.length > 0 ? (
                  <div className="space-y-1 max-h-40 overflow-y-auto">
                    {ds.schema_cache.map(col => (
                      <div key={col.name} className="flex items-center justify-between text-xs">
                        <span className="text-[#202124]">{col.name}</span>
                        <span className="text-[#9aa0a6]">{col.type}</span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-[#9aa0a6]">{t('data_refresh_to_see_schema')}</p>
                )}
                {/* Measures of this dataset */}
                {measures.filter(m => m.dataset_id === ds.id).map(m => (
                  <div key={m.id} className="flex items-center gap-1 mt-2 text-xs">
                    <TrendingUp size={12} className="text-[#34a853]" />
                    <span className="text-[#137333]">{m.name}</span>
                    {!m.is_valid && <AlertCircle size={12} className="text-[#d93025]" />}
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}

        {relations.length > 0 && (
          <div className="mt-8">
            <h2 className="text-sm font-medium text-[#5f6368] uppercase tracking-wide mb-4">
              {t('data_relations', { count: relations.length })}
            </h2>
            <div className="space-y-2">
              {relations.map(rel => {
                const from = datasets.find(d => d.id === rel.from_dataset_id)
                const to   = datasets.find(d => d.id === rel.to_dataset_id)
                return (
                  <div key={rel.id} className="flex items-center gap-3 bg-white border border-[#e8eaed] rounded-lg px-4 py-2 text-sm">
                    <span className="text-[#202124] font-medium">{from?.name ?? '?'}</span>
                    <span className="text-[#9aa0a6]">.{rel.from_column}</span>
                    <span className="text-[#5f6368]">→</span>
                    <span className="text-[#202124] font-medium">{to?.name ?? '?'}</span>
                    <span className="text-[#9aa0a6]">.{rel.to_column}</span>
                    <span className="ml-auto text-xs text-[#9aa0a6]">{rel.cardinality}</span>
                  </div>
                )
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

// ── Status Badge ──────────────────────────────────────────────────────────────

function StatusBadge({ status }: { status: string }) {
  const { t } = useTranslation('office')
  const styles: Record<string, string> = {
    ready:      'bg-[#e6f4ea] text-[#137333]',
    refreshing: 'bg-[#fef7e0] text-[#7a4f01]',
    error:      'bg-[#fce8e6] text-[#c5221f]',
    empty:      'bg-[#f1f3f4] text-[#5f6368]',
  }
  const labels: Record<string, string> = {
    ready: t('data_status_ready'),
    refreshing: t('data_status_refreshing'),
    error: t('data_status_error'),
    empty: t('data_status_empty'),
  }
  return (
    <span className={clsx('text-[10px] px-1.5 py-0.5 rounded-full font-medium', styles[status] ?? styles.empty)}>
      {labels[status] ?? status}
    </span>
  )
}
