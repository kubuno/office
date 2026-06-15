import { useState, useRef, useCallback, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  BarChart3, Plus, Trash2, Star, RefreshCw,
  LayoutDashboard, Database, Network, Eye, Share2,
  Table, TrendingUp, PieChart, AlignLeft, X, Check, AlertCircle,
  GripVertical, ExternalLink, Copy,
} from 'lucide-react'
import clsx from 'clsx'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Button, Input, Dropdown } from '@ui'
import type { StartPageRecentItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import * as Dialog from '@radix-ui/react-dialog'
import {
  reportsApi, datasetsApi, datasourcesApi, executeApi, pagesApi, widgetsApi, modelApi,
  type Report, type ReportPage, type Widget, type Dataset, type Relation, type Measure,
  type WidgetType,
} from './data-api'
import { WidgetRenderer } from './DataCharts'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_DATA } from './ribbon/officeThemes'
import { fileGroup } from './ribbon/common'

// ── Types ─────────────────────────────────────────────────────────────────────

type DataView = 'list' | 'report' | 'query' | 'model'

// ── Main Shell ────────────────────────────────────────────────────────────────

export default function DataApp() {
  const [view, setView] = useState<DataView>('list')
  const [activeReportId, setActiveReportId] = useState<string | null>(null)

  const openReport = (id: string) => {
    setActiveReportId(id)
    setView('report')
  }

  const backToList = () => {
    setActiveReportId(null)
    setView('list')
  }

  if (view === 'list') {
    return <DataReportsList onOpenReport={openReport} />
  }

  if (activeReportId) {
    return (
      <DataReportShell
        reportId={activeReportId}
        view={view}
        onViewChange={setView}
        onBack={backToList}
        onOpenReport={openReport}
      />
    )
  }

  return <DataReportsList onOpenReport={openReport} />
}

// ── Reports List ──────────────────────────────────────────────────────────────

function DataReportsList({ onOpenReport }: { onOpenReport: (id: string) => void }) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()

  const { data } = useQuery({
    queryKey: ['data-reports', 'all', ''],
    queryFn: () => reportsApi.list({}),
  })

  const createMut = useMutation({
    mutationFn: () => reportsApi.create({ title: t('data_new_report') }),
    onSuccess: (d) => {
      qc.invalidateQueries({ queryKey: ['data-reports'] })
      onOpenReport(d.report.id)
    },
  })

  const trashMut = useMutation({
    mutationFn: (id: string) => reportsApi.trash(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['data-reports'] }),
  })

  const dupMut = useMutation({
    mutationFn: (id: string) => reportsApi.duplicate(id),
    onSuccess: (d) => {
      qc.invalidateQueries({ queryKey: ['data-reports'] })
      onOpenReport(d.report.id)
    },
  })

  const reports = data?.reports ?? []

  // Récents = rapports (ouverts par id). Le navigateur « Parcourir » liste les
  // fichiers .kbdst/.kbdrp du dossier Office/Data dans `files`.
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

// ── Report Shell (Editor + Query + Model tabs) ────────────────────────────────

function DataReportShell({ reportId, view, onViewChange, onBack, onOpenReport }: {
  reportId: string
  view: DataView
  onViewChange: (v: DataView) => void
  onBack: () => void
  onOpenReport: (id: string) => void
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()

  const { data, isLoading } = useQuery({
    queryKey: ['data-report', reportId],
    queryFn: () => reportsApi.get(reportId),
  })

  const report = data?.report

  // ── Titre éditable (standard WorkspaceShell) — synchronisé depuis le rapport ──
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

  return (
    <OfficeShell
      ribbon={[{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
        groups: [fileGroup(t, { onNew: () => createMut.mutate(), onDuplicate: () => dupMut.mutate() })] }]}
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
        <button
          onClick={() => updateMut.mutate({ is_starred: !report.is_starred })}
          title={report.is_starred ? t('data_unstar', { defaultValue: 'Retirer des favoris' }) : t('data_star', { defaultValue: 'Ajouter aux favoris' })}
          className={clsx('p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0', report.is_starred ? 'text-warning' : 'text-white/90')}
        >
          <Star size={15} fill={report.is_starred ? 'currentColor' : 'none'} />
        </button>
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
        <Button variant="secondary" size="sm" icon={<Eye size={15} />}>
          {t('data_preview')}
        </Button>
        <Button size="sm" icon={<Share2 size={15} />}>
          {t('data_share')}
        </Button>
      </>}
    >
      {/* Content */}
      <div className="flex-1 min-w-0 overflow-hidden">
        {view === 'report' && (
          <ReportEditor report={report} pages={pages} widgets={widgets} />
        )}
        {view === 'query' && <DataQueryEditor reportId={reportId} />}
        {view === 'model' && <DataModelView reportId={reportId} />}
      </div>
    </OfficeShell>
  )
}

// ── Report Editor (canvas) ─────────────────────────────────────────────────────

function ReportEditor({ report, pages, widgets }: {
  report: Report
  pages: ReportPage[]
  widgets: Widget[]
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const [activePageId, setActivePageId] = useState(pages[0]?.id ?? '')
  const [selectedWidgetId, setSelectedWidgetId] = useState<string | null>(null)
  const [showAddWidget, setShowAddWidget] = useState(false)

  const pageWidgets = widgets.filter(w => w.page_id === activePageId)
  const page = pages.find(p => p.id === activePageId)

  const addPageMut = useMutation({
    mutationFn: () => pagesApi.create(report.id),
    onSuccess: (d) => {
      qc.invalidateQueries({ queryKey: ['data-report', report.id] })
      setActivePageId(d.page.id)
    },
  })

  const addWidgetMut = useMutation({
    mutationFn: (type: WidgetType) => widgetsApi.create(activePageId, {
      widget_type: type,
      x: 50, y: 80,
      width: type === 'kpi_card' ? 220 : 400,
      height: type === 'kpi_card' ? 120 : 280,
      config: { title: t(widgetLabelKey(type)) },
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['data-report', report.id] })
      setShowAddWidget(false)
    },
  })

  const deleteWidgetMut = useMutation({
    mutationFn: (id: string) => widgetsApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['data-report', report.id] })
      setSelectedWidgetId(null)
    },
  })

  const selected = widgets.find(w => w.id === selectedWidgetId)

  return (
    <div className="flex h-full overflow-hidden">
      {/* Canvas area */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Insert toolbar */}
        <div className="flex items-center gap-2 px-4 py-2 border-b border-[#e8eaed] bg-white shrink-0">
          <Button
            variant="secondary"
            size="sm"
            icon={<Plus size={15} />}
            onClick={() => setShowAddWidget(true)}
          >
            {t('data_add_chart')}
          </Button>
          {selectedWidgetId && (
            <Button
              variant="danger"
              size="sm"
              icon={<Trash2 size={14} />}
              onClick={() => deleteWidgetMut.mutate(selectedWidgetId)}
            >
              {t('common_delete')}
            </Button>
          )}
        </div>

        {/* Canvas */}
        <div className="flex-1 overflow-auto bg-[#f8f9fa] p-6">
          <div
            className="relative bg-white shadow-md mx-auto"
            style={{ width: page?.width ?? 1200, height: page?.height ?? 800, minHeight: 400 }}
            onClick={() => setSelectedWidgetId(null)}
          >
            {pageWidgets.map(widget => (
              <WidgetCard
                key={widget.id}
                widget={widget}
                isSelected={widget.id === selectedWidgetId}
                onSelect={() => setSelectedWidgetId(widget.id)}
                reportId={report.id}
              />
            ))}
            {pageWidgets.length === 0 && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 text-[#9aa0a6]">
                <BarChart3 size={48} className="opacity-30" />
                <p className="text-sm">{t('data_canvas_empty')}</p>
              </div>
            )}
          </div>
        </div>

        {/* Pages tabs */}
        <div className="flex items-center gap-1 px-4 py-1.5 bg-white border-t border-[#e8eaed] shrink-0 overflow-x-auto">
          {pages.map(p => (
            <button
              key={p.id}
              onClick={() => setActivePageId(p.id)}
              className={clsx(
                'px-3 py-1 text-xs rounded whitespace-nowrap transition-colors',
                p.id === activePageId
                  ? 'bg-[#e8f0fe] text-[#1a73e8] font-medium'
                  : 'text-[#5f6368] hover:bg-[#f1f3f4]',
              )}
            >
              {p.title}
            </button>
          ))}
          <button
            onClick={() => addPageMut.mutate()}
            className="p-1 text-[#9aa0a6] hover:text-[#5f6368] hover:bg-[#f1f3f4] rounded shrink-0"
          >
            <Plus size={14} />
          </button>
        </div>
      </div>

      {/* Properties panel */}
      {selected && (
        <WidgetPropertiesPanel
          widget={selected}
          reportId={report.id}
          onClose={() => setSelectedWidgetId(null)}
        />
      )}

      {/* Add Widget Dialog */}
      <Dialog.Root open={showAddWidget} onOpenChange={setShowAddWidget}>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 bg-black/30 z-40" />
          <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-50 bg-white rounded-xl shadow-xl p-6 w-[500px]">
            <Dialog.Title className="text-base font-medium text-[#202124] mb-4">
              {t('data_choose_widget_type')}
            </Dialog.Title>
            <div className="grid grid-cols-3 gap-3">
              {WIDGET_TYPES.map(wt => (
                <button
                  key={wt.type}
                  onClick={() => addWidgetMut.mutate(wt.type as WidgetType)}
                  className="flex flex-col items-center gap-2 p-3 border border-[#e8eaed] rounded-xl hover:border-[#1a73e8] hover:bg-[#f8f9ff] transition-colors"
                >
                  <wt.Icon size={22} className="text-[#1a73e8]" />
                  <span className="text-xs text-[#5f6368] text-center">{t(wt.labelKey)}</span>
                </button>
              ))}
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  )
}

const WIDGET_TYPES = [
  { type: 'kpi_card',    labelKey: 'data_widget_kpi_card',    Icon: TrendingUp },
  { type: 'bar_chart',   labelKey: 'data_widget_bar_chart',   Icon: BarChart3 },
  { type: 'line_chart',  labelKey: 'data_widget_line_chart',  Icon: TrendingUp },
  { type: 'pie_chart',   labelKey: 'data_widget_pie_chart',   Icon: PieChart },
  { type: 'donut_chart', labelKey: 'data_widget_donut_chart', Icon: PieChart },
  { type: 'data_table',  labelKey: 'data_widget_data_table',  Icon: Table },
  { type: 'text',        labelKey: 'data_widget_text',        Icon: AlignLeft },
]

function widgetLabelKey(type: string): string {
  return WIDGET_TYPES.find(w => w.type === type)?.labelKey ?? 'data_widget_text'
}

// ── Widget Card (draggable) ───────────────────────────────────────────────────

function WidgetCard({ widget, isSelected, onSelect, reportId }: {
  widget: Widget
  isSelected: boolean
  onSelect: () => void
  reportId: string
}) {
  const qc = useQueryClient()
  const { data: execData, isLoading, error } = useQuery({
    queryKey: ['widget-data', widget.id, widget.config],
    queryFn: async () => {
      const cfg = widget.config
      if (!cfg.dataset_id) return { columns: [], rows: [], total: 0 }
      return executeApi.query({
        dataset_id: cfg.dataset_id as string,
        dimensions: cfg.dimensions as string[],
        metrics: cfg.metrics as { column: string; function: string; alias?: string }[],
        filters: cfg.filters as { column: string; operator: string; value: unknown }[],
        sort: cfg.sort as { column: string; direction: string }[],
        limit: (cfg.limit as number) ?? 100,
      })
    },
    enabled: !!widget.config.dataset_id,
  })

  // Drag logic
  const dragRef = useRef<{ startX: number; startY: number; origX: number; origY: number } | null>(null)

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('[data-no-drag]')) return
    e.preventDefault()
    dragRef.current = { startX: e.clientX, startY: e.clientY, origX: widget.x, origY: widget.y }
    onSelect()

    const onMove = (ev: MouseEvent) => {
      if (!dragRef.current) return
      const dx = ev.clientX - dragRef.current.startX
      const dy = ev.clientY - dragRef.current.startY
      const el = document.getElementById(`widget-${widget.id}`)
      if (el) {
        el.style.left = `${dragRef.current.origX + dx}px`
        el.style.top  = `${dragRef.current.origY + dy}px`
      }
    }

    const onUp = (ev: MouseEvent) => {
      if (!dragRef.current) return
      const dx = ev.clientX - dragRef.current.startX
      const dy = ev.clientY - dragRef.current.startY
      const newX = Math.max(0, Math.round((dragRef.current.origX + dx) / 8) * 8)
      const newY = Math.max(0, Math.round((dragRef.current.origY + dy) / 8) * 8)
      widgetsApi.update(widget.id, { x: newX, y: newY }).then(() => {
        qc.invalidateQueries({ queryKey: ['data-report', reportId] })
      })
      dragRef.current = null
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }

    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }, [widget, onSelect, qc, reportId])

  return (
    <div
      id={`widget-${widget.id}`}
      className={clsx(
        'absolute border rounded-lg overflow-hidden bg-white cursor-move select-none',
        isSelected ? 'border-[#1a73e8] shadow-lg ring-1 ring-[#1a73e8]' : 'border-[#e8eaed] hover:border-[#bdc1c6]',
      )}
      style={{ left: widget.x, top: widget.y, width: widget.width, height: widget.height, zIndex: isSelected ? 100 : widget.z_index }}
      onMouseDown={onMouseDown}
    >
      {/* Drag handle */}
      <div className="absolute top-1 left-1 cursor-move text-[#dadce0] hover:text-[#9aa0a6] z-10" data-no-drag="false">
        <GripVertical size={14} />
      </div>
      <WidgetRenderer
        widgetType={widget.widget_type}
        config={widget.config as Record<string, unknown>}
        data={execData?.rows ?? []}
        isLoading={isLoading && !!widget.config.dataset_id}
        error={error ? String(error) : undefined}
      />
    </div>
  )
}

// ── Widget Properties Panel ───────────────────────────────────────────────────

function WidgetPropertiesPanel({ widget, reportId, onClose }: {
  widget: Widget
  reportId: string
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const { data: datasetsData } = useQuery({
    queryKey: ['data-datasets'],
    queryFn: datasetsApi.list,
  })
  const datasets = datasetsData?.datasets ?? []

  const cfg = widget.config
  const selectedDataset = datasets.find(d => d.id === cfg.dataset_id)

  const updateConfig = (partial: Record<string, unknown>) => {
    widgetsApi.update(widget.id, { config: { ...cfg, ...partial } }).then(() => {
      qc.invalidateQueries({ queryKey: ['data-report', reportId] })
    })
  }

  const columns = selectedDataset?.schema_cache?.map(c => c.name) ?? []

  return (
    <div className="w-72 shrink-0 border-l border-[#e8eaed] flex flex-col bg-white overflow-y-auto">
      <div className="flex items-center justify-between px-3 py-2 border-b border-[#e8eaed] shrink-0">
        <span className="text-xs font-medium text-[#5f6368] uppercase tracking-wide">{t('data_properties')}</span>
        <button onClick={onClose} className="p-1 rounded hover:bg-[#f1f3f4] text-[#9aa0a6]">
          <X size={14} />
        </button>
      </div>

      <div className="flex-1 p-3 space-y-4">
        {/* Titre */}
        <div>
          <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('data_label_title')}</label>
          <Input
            defaultValue={cfg.title as string ?? ''}
            onBlur={e => updateConfig({ title: e.target.value })}
          />
        </div>

        {/* Dataset */}
        <div>
          <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('data_label_datasource')}</label>
          <Dropdown
            value={cfg.dataset_id as string ?? ''}
            onChange={v => updateConfig({ dataset_id: v, dimensions: [], metrics: [] })}
            width="100%"
            placeholder={t('data_choose_dataset')}
            options={[
              { value: '', label: t('data_choose_dataset') },
              ...datasets.map(d => ({ value: d.id, label: d.name })),
            ]}
          />
          {selectedDataset?.status !== 'ready' && cfg.dataset_id && (
            <p className="text-xs text-[#f9ab00] mt-1">
              {t('data_dataset_not_loaded')}
            </p>
          )}
        </div>

        {columns.length > 0 && (
          <>
            {/* Dimension */}
            <div>
              <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('data_label_dimension')}</label>
              <Dropdown
                value={(cfg.dimensions as string[] | undefined)?.[0] ?? ''}
                onChange={v => updateConfig({ dimensions: v ? [v] : [] })}
                width="100%"
                options={[
                  { value: '', label: t('data_option_none') },
                  ...columns.map(c => ({ value: c, label: c })),
                ]}
              />
            </div>

            {/* Metric */}
            <div>
              <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('data_label_measure')}</label>
              <div className="flex gap-1">
                <Dropdown
                  value={(cfg.metrics as { function: string }[] | undefined)?.[0]?.function ?? 'SUM'}
                  onChange={v => {
                    const mets = (cfg.metrics as { column: string; function: string }[] | undefined) ?? []
                    const col = mets[0]?.column ?? ''
                    updateConfig({ metrics: [{ column: col, function: v }] })
                  }}
                  width={80}
                  fontSize={12}
                  options={['SUM', 'COUNT', 'AVG', 'MIN', 'MAX'].map(f => ({ value: f, label: f }))}
                />
                <Dropdown
                  value={(cfg.metrics as { column: string }[] | undefined)?.[0]?.column ?? ''}
                  onChange={v => {
                    const mets = (cfg.metrics as { function: string }[] | undefined) ?? []
                    const fn = mets[0]?.function ?? 'SUM'
                    updateConfig({ metrics: v ? [{ column: v, function: fn }] : [] })
                  }}
                  className="flex-1"
                  fontSize={12}
                  options={[
                    { value: '', label: t('data_option_column') },
                    ...columns.map(c => ({ value: c, label: c })),
                  ]}
                />
              </div>
            </div>

            {/* Limit */}
            <div>
              <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('data_label_row_limit')}</label>
              <Input
                type="number"
                defaultValue={(cfg.limit as number) ?? 100}
                min={1} max={10000}
                onBlur={e => updateConfig({ limit: parseInt(e.target.value) || 100 })}
              />
            </div>
          </>
        )}
      </div>
    </div>
  )
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
                {/* Mesures de ce dataset */}
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
