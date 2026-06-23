// DataReportEditor — Power BI-style report authoring surface for the Office Data
// sub-module. Uses the shared core docking system (`DockArea` / `DockController`
// from @kubuno/sdk — the same dock the diagram editor uses): the Fields,
// Visualizations, Format and Filters panes are real dockable/floatable panels
// (re-dock left/right, tab-group, tear off, resize, close/reopen, persisted).
// Everything is config-driven (JSONB) so new visuals/options need no backend.
import { useState, useRef, useCallback, useMemo, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Plus, Trash2, Type as TypeIcon, Image as ImageIcon, Square, RefreshCw,
  Filter as FilterIcon, LayoutDashboard, Grid2x2, Magnet,
  Maximize2, Copy, ClipboardPaste, BringToFront, SendToBack, Lock, Unlock,
  AlignStartVertical, AlignCenterVertical, AlignEndVertical, Palette, Sigma,
  Network, Database, ChevronDown, GripVertical, X, Search,
  AlignHorizontalDistributeCenter, ListFilter,
} from 'lucide-react'
import clsx from 'clsx'
import { Button, Input, Dropdown, MenuDropdown } from '@ui'
import type { MenuItem, MenuDropdownPos } from '@ui'
import { DockArea, type DockController, type DockPanel } from '@kubuno/sdk'
import type { RibbonTab } from './ribbon/types'
import {
  widgetsApi, pagesApi, datasetsApi, executeApi,
  type Report, type ReportPage, type Widget, type Dataset,
} from './data-api'
import { WidgetRenderer } from './DataCharts'
import { VISUALS, type VisualCategory } from './data/visuals'
import { STATIC_VISUALS, SLICER_VISUALS } from './data/visuals'
import { wellsFor, AGG_FUNCTIONS, FILTER_OPERATORS, type Well } from './data/wells'
import { NUMBER_FORMATS } from './data/format'
import { PALETTES, REPORT_THEMES } from './data/palettes'

const GRID = 8
const snap = (v: number, on: boolean) => on ? Math.round(v / GRID) * GRID : Math.round(v)

// ── Shared editor state (owned here, surfaced to the ribbon) ──────────────────
// Panel docking/visibility is owned by DockArea; this state is editor-only.

export interface EditorView {
  activePageId: string
  selectedIds: string[]
  showGrid: boolean
  snapGrid: boolean
  focusId: string | null
}

type RenderShell = (ribbon: RibbonTab[], body: ReactNode) => ReactNode

interface FilterRule { column: string; operator: string; value: unknown }

const VISUAL_CATEGORIES: { id: VisualCategory; label: string }[] = [
  { id: 'cards', label: 'Cartes & KPI' },
  { id: 'bars', label: 'Barres & colonnes' },
  { id: 'lines', label: 'Courbes & aires' },
  { id: 'parts', label: 'Proportions' },
  { id: 'tables', label: 'Tables' },
  { id: 'distribution', label: 'Distribution' },
  { id: 'filters', label: 'Segments' },
  { id: 'other', label: 'Autres' },
]

// ════════════════════════════════════════════════════════════════════════════

export function DataReportEditor({ report, pages, widgets, reportId, renderShell, onSwitchView }: {
  report: Report
  pages: ReportPage[]
  widgets: Widget[]
  reportId: string
  renderShell: RenderShell
  onSwitchView?: (v: 'query' | 'model') => void
}) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const invalidate = useCallback(() => qc.invalidateQueries({ queryKey: ['data-report', reportId] }), [qc, reportId])

  const [ed, setEd] = useState<EditorView>(() => ({
    activePageId: pages[0]?.id ?? '',
    selectedIds: [],
    showGrid: true,
    snapGrid: true,
    focusId: null,
  }))
  const patch = useCallback((p: Partial<EditorView>) => setEd(e => ({ ...e, ...p })), [])
  const [ctxMenu, setCtxMenu] = useState<{ id: string; pos: MenuDropdownPos } | null>(null)
  const [pageCtx, setPageCtx] = useState<{ id: string; pos: MenuDropdownPos } | null>(null)
  // Shared dock controller (activate/open/close/reset panels from ribbon & menus).
  const dockRef = useRef<DockController | null>(null)

  // Keep activePageId valid.
  const activePageId = pages.some(p => p.id === ed.activePageId) ? ed.activePageId : (pages[0]?.id ?? '')
  const page = pages.find(p => p.id === activePageId)
  const pageWidgets = widgets.filter(w => w.page_id === activePageId)
  const selected = widgets.find(w => ed.selectedIds.includes(w.id) && w.page_id === activePageId) ?? null

  const { data: datasetsData } = useQuery({ queryKey: ['data-datasets'], queryFn: datasetsApi.list })
  const datasets = useMemo(() => datasetsData?.datasets ?? [], [datasetsData])

  // Page-level filters + slicer selections drive cross-filtering.
  const [pageFilters, setPageFilters] = useState<Record<string, FilterRule[]>>({})
  const [slicerSel, setSlicerSel] = useState<Record<string, string[]>>({})

  // ── Mutations ──────────────────────────────────────────────────────────────
  const addWidgetMut = useMutation({
    mutationFn: (type: string) => {
      const def = VISUALS.find(v => v.type === type)
      const big = !['card', 'kpi_card', 'gauge', 'bullet', 'shape', 'progress_ring'].includes(type)
      return widgetsApi.create(activePageId, {
        widget_type: type as Widget['widget_type'],
        x: 48, y: 64,
        width: big ? 400 : 220, height: big ? 280 : 130,
        config: { title: def?.label ?? '', paletteId: report.theme?.['paletteId' as keyof typeof report.theme] ?? 'kubuno' },
      })
    },
    onSuccess: (d) => { invalidate(); patch({ selectedIds: [d.widget.id] }) },
  })
  const updateWidgetMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<Widget> }) => widgetsApi.update(id, data),
    onSuccess: invalidate,
  })
  const deleteWidgetMut = useMutation({
    mutationFn: (id: string) => widgetsApi.delete(id),
    onSuccess: () => { invalidate(); patch({ selectedIds: [] }) },
  })
  const addPageMut = useMutation({
    mutationFn: () => pagesApi.create(reportId),
    onSuccess: (d) => { invalidate(); patch({ activePageId: d.page.id, selectedIds: [] }) },
  })
  const updatePageMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<ReportPage> }) => pagesApi.update(reportId, id, data),
    onSuccess: invalidate,
  })
  const deletePageMut = useMutation({
    mutationFn: (id: string) => pagesApi.delete(reportId, id),
    onSuccess: invalidate,
  })
  const updateReportMut = useMutation({
    mutationFn: (data: Partial<Report>) => import('./data-api').then(({ reportsApi }) => reportsApi.update(reportId, data)),
    onSuccess: () => { invalidate(); qc.invalidateQueries({ queryKey: ['data-report', reportId] }) },
  })

  const updateConfig = useCallback((id: string, partial: Record<string, unknown>) => {
    const w = widgets.find(x => x.id === id); if (!w) return
    updateWidgetMut.mutate({ id, data: { config: { ...w.config, ...partial } } })
  }, [widgets, updateWidgetMut])

  // ── Clipboard ────────────────────────────────────────────────────────────────
  const clipboard = useRef<Widget | null>(null)
  const copySel = () => { if (selected) clipboard.current = selected }
  const pasteClip = () => {
    const c = clipboard.current; if (!c) return
    widgetsApi.create(activePageId, {
      widget_type: c.widget_type, x: snap(c.x + 24, ed.snapGrid), y: snap(c.y + 24, ed.snapGrid),
      width: c.width, height: c.height, config: c.config,
    }).then(d => { invalidate(); patch({ selectedIds: [d.widget.id] }) })
  }

  const alignSel = (mode: 'left' | 'center' | 'right' | 'distribute') => {
    if (!selected) return
    if (mode === 'distribute') return
    const x = mode === 'left' ? 0 : mode === 'right' ? (page?.width ?? 1200) - selected.width : ((page?.width ?? 1200) - selected.width) / 2
    updateWidgetMut.mutate({ id: selected.id, data: { x: snap(x, ed.snapGrid) } })
  }
  const zOrder = (dir: 'front' | 'back') => {
    if (!selected) return
    const zs = pageWidgets.map(w => w.z_index)
    const z = dir === 'front' ? Math.max(0, ...zs) + 1 : Math.min(0, ...zs) - 1
    updateWidgetMut.mutate({ id: selected.id, data: { z_index: z } })
  }
  const toggleLock = () => { if (selected) updateConfig(selected.id, { locked: !selected.config.locked }) }

  // Apply a report theme: persist on the report + repaint every visual's palette.
  const setTheme = (themeId: string) => {
    const th = REPORT_THEMES.find(x => x.id === themeId); if (!th) return
    updateReportMut.mutate({ theme: { primaryColor: th.primaryColor, fontFamily: th.fontFamily, background: th.pageBackground, chartPalette: th.chartPalette } })
    widgets.forEach(w => updateWidgetMut.mutate({ id: w.id, data: { config: { ...w.config, paletteId: th.paletteId, palette: th.chartPalette } } }))
  }

  const openPane = useCallback((id: string) => { dockRef.current?.open(id); dockRef.current?.activate(id) }, [])

  // ── Ribbon ──────────────────────────────────────────────────────────────────
  const ribbon = useDataRibbon({
    t, ed, patch, selected, datasets, openPane,
    onAddVisual: (type) => addWidgetMut.mutate(type),
    onRefresh: () => qc.invalidateQueries({ queryKey: ['widget-data'] }),
    onSwitchView,
    onCopy: copySel, onPaste: pasteClip, onDelete: () => selected && deleteWidgetMut.mutate(selected.id),
    onAlign: alignSel, onZ: zOrder, onLock: toggleLock,
    onTheme: setTheme,
    report, reportId, qc,
  })

  // ── Dockable panes (shared core DockArea) ─────────────────────────────────────
  const panels: Record<string, DockPanel> = {
    fields: {
      label: t('data_pane_fields', { defaultValue: 'Champs' }),
      render: () => <FieldsPane datasets={datasets} selected={selected} onAddField={(col, kind) => selected && assignField(selected, col, kind, updateConfig)} onSwitchView={onSwitchView} />,
    },
    filter: {
      label: t('data_pane_filters', { defaultValue: 'Filtres' }),
      render: () => <FiltersPane datasets={datasets} rules={pageFilters[activePageId] ?? []} onChange={(rules) => setPageFilters(f => ({ ...f, [activePageId]: rules }))} selected={selected} onConfig={updateConfig} />,
    },
    visual: {
      label: t('data_pane_viz', { defaultValue: 'Visualisations' }),
      render: () => <VisualizationsPane selected={selected} onAddVisual={(t2) => addWidgetMut.mutate(t2)} onChangeType={(t2) => selected && updateWidgetMut.mutate({ id: selected.id, data: { widget_type: t2 as Widget['widget_type'] } })}
        datasets={datasets} onConfig={updateConfig} onAssign={(col, kind) => selected && assignField(selected, col, kind, updateConfig)} />,
    },
    format: {
      label: t('data_format', { defaultValue: 'Format' }),
      render: () => <FormatPane selected={selected} onConfig={updateConfig} />,
    },
  }

  // ── Canvas body ───────────────────────────────────────────────────────────────
  const body = (
    <div className="flex flex-1 min-w-0 overflow-hidden relative">
      <DockArea
        panels={panels}
        storageKey="kubuno:office:dataDock"
        defaultArrangement={{ left: [['fields']], right: [['filter', 'visual', 'format']] }}
        controllerRef={dockRef}
        viewportBg="#f8f9fa"
        className="flex flex-1 min-w-0 overflow-hidden"
      >
        {/* Center — canvas + pages */}
        <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
          <CanvasToolbar
            ed={ed} patch={patch}
            onAdd={() => openPane('visual')}
            onDelete={() => selected && deleteWidgetMut.mutate(selected.id)}
            hasSel={!!selected}
          />
          <div className="flex-1 overflow-auto p-6">
            <CanvasSurface
              page={page} widgets={pageWidgets} ed={ed} patch={patch}
              datasets={datasets} pageFilters={pageFilters[activePageId] ?? []} slicerSel={slicerSel}
              onMove={(id, x, y) => updateWidgetMut.mutate({ id, data: { x, y } })}
              onResize={(id, w, h) => updateWidgetMut.mutate({ id, data: { width: w, height: h } })}
              onContext={(id, pos) => setCtxMenu({ id, pos })}
              onSlicer={(id, sel) => setSlicerSel(s => ({ ...s, [id]: sel }))}
            />
          </div>
          <PagesBar
            pages={pages} activePageId={activePageId}
            onSelect={(id) => patch({ activePageId: id, selectedIds: [] })}
            onAdd={() => addPageMut.mutate()}
            onContext={(id, pos) => setPageCtx({ id, pos })}
          />
        </div>
      </DockArea>

      {/* Context menus */}
      {ctxMenu && (
        <MenuDropdown pos={{ ...ctxMenu.pos, minWidth: 210 }} onClose={() => setCtxMenu(null)}
          items={widgetMenu(widgets.find(w => w.id === ctxMenu.id), {
            t, onCopy: () => { clipboard.current = widgets.find(w => w.id === ctxMenu.id) ?? null }, onPaste: pasteClip,
            onDelete: () => deleteWidgetMut.mutate(ctxMenu.id),
            onFront: () => { patch({ selectedIds: [ctxMenu.id] }); zOrder('front') },
            onBack: () => { patch({ selectedIds: [ctxMenu.id] }); zOrder('back') },
            onDuplicate: () => { const w = widgets.find(x => x.id === ctxMenu.id); if (w) widgetsApi.create(activePageId, { widget_type: w.widget_type, x: w.x + 24, y: w.y + 24, width: w.width, height: w.height, config: w.config }).then(invalidate) },
            onLock: () => { const w = widgets.find(x => x.id === ctxMenu.id); if (w) updateConfig(w.id, { locked: !w.config.locked }) },
            onFocus: () => patch({ focusId: ctxMenu.id }),
            onFormat: () => { patch({ selectedIds: [ctxMenu.id] }); openPane('format') },
            onChangeType: (ty) => updateWidgetMut.mutate({ id: ctxMenu.id, data: { widget_type: ty as Widget['widget_type'] } }),
          })} />
      )}
      {pageCtx && (
        <MenuDropdown pos={{ ...pageCtx.pos, minWidth: 190 }} onClose={() => setPageCtx(null)}
          items={pageMenu(pages.find(p => p.id === pageCtx.id), {
            t,
            onDuplicate: () => addPageMut.mutate(),
            onDelete: () => pages.length > 1 && deletePageMut.mutate(pageCtx.id),
            onRenameInline: (name) => updatePageMut.mutate({ id: pageCtx.id, data: { title: name } }),
          })} />
      )}

      {/* Focus mode overlay */}
      {ed.focusId && (() => {
        const w = widgets.find(x => x.id === ed.focusId); if (!w) return null
        return <FocusOverlay widget={w} datasets={datasets} pageFilters={pageFilters[activePageId] ?? []} slicerSel={slicerSel} onClose={() => patch({ focusId: null })} />
      })()}
    </div>
  )

  return <>{renderShell(ribbon, body)}</>
}

// ── Field assignment helper ────────────────────────────────────────────────────

function assignField(widget: Widget, column: string, kind: 'dimension' | 'metric', updateConfig: (id: string, p: Record<string, unknown>) => void) {
  const wells = wellsFor(widget.widget_type)
  const cfg = widget.config as Record<string, unknown>
  const well = wells.find(w => w.kind === kind) ?? wells[0]
  if (!well) return
  if (well.kind === 'metric') {
    const metrics = (cfg.metrics as { column: string; function: string }[] | undefined) ?? []
    const next = well.multi ? [...metrics, { column, function: 'SUM' }] : [{ column, function: 'SUM' }]
    updateConfig(widget.id, { metrics: next, dataset_id: cfg.dataset_id })
  } else {
    const dims = (cfg.dimensions as string[] | undefined) ?? []
    let next: string[]
    if (well.id === 'legend') next = [dims[0] ?? '', column]
    else next = well.multi ? [...dims, column] : [column, dims[1] ?? ''].filter(Boolean)
    updateConfig(widget.id, { dimensions: next.filter((v, i, a) => v && a.indexOf(v) === i) })
  }
}

// ════════════════════════════════════════════════════════════════════════════
// Canvas
// ════════════════════════════════════════════════════════════════════════════

function CanvasToolbar({ ed, patch, onAdd, onDelete, hasSel }: {
  ed: EditorView; patch: (p: Partial<EditorView>) => void; onAdd: () => void; onDelete: () => void; hasSel: boolean
}) {
  const { t } = useTranslation('office')
  return (
    <div className="flex items-center gap-2 px-4 py-1.5 border-b border-[#e8eaed] bg-white shrink-0">
      <Button variant="secondary" size="sm" icon={<Plus size={15} />} onClick={onAdd}>{t('data_add_chart', { defaultValue: 'Ajouter un visuel' })}</Button>
      {hasSel && <Button variant="danger" size="sm" icon={<Trash2 size={14} />} onClick={onDelete}>{t('common_delete', { defaultValue: 'Supprimer' })}</Button>}
      <div className="flex-1" />
      <IconToggle on={ed.showGrid} icon={<Grid2x2 size={15} />} title={t('data_view_grid', { defaultValue: 'Quadrillage' })} onClick={() => patch({ showGrid: !ed.showGrid })} />
      <IconToggle on={ed.snapGrid} icon={<Magnet size={15} />} title={t('data_view_snap', { defaultValue: 'Aligner sur la grille' })} onClick={() => patch({ snapGrid: !ed.snapGrid })} />
    </div>
  )
}

function IconToggle({ on, icon, title, onClick }: { on: boolean; icon: ReactNode; title: string; onClick: () => void }) {
  return (
    <button onClick={onClick} title={title}
      className={clsx('p-1.5 rounded transition-colors', on ? 'bg-[#e8f0fe] text-[#1a73e8]' : 'text-[#5f6368] hover:bg-[#f1f3f4]')}>
      {icon}
    </button>
  )
}

function CanvasSurface({ page, widgets, ed, patch, datasets, pageFilters, slicerSel, onMove, onResize, onContext, onSlicer }: {
  page?: ReportPage; widgets: Widget[]; ed: EditorView; patch: (p: Partial<EditorView>) => void
  datasets: Dataset[]; pageFilters: FilterRule[]; slicerSel: Record<string, string[]>
  onMove: (id: string, x: number, y: number) => void
  onResize: (id: string, w: number, h: number) => void
  onContext: (id: string, pos: MenuDropdownPos) => void
  onSlicer: (id: string, sel: string[]) => void
}) {
  const { t } = useTranslation('office')
  const w = page?.width ?? 1200, h = page?.height ?? 800
  // Slicer selections on this page become cross-filters for the other visuals.
  const slicerFilters: SlicerFilter[] = useMemo(() => widgets
    .filter(wd => SLICER_VISUALS.has(wd.widget_type))
    .map((wd): SlicerFilter | null => {
      const cfg = wd.config as Record<string, unknown>
      const dim = (cfg.dimensions as string[] | undefined)?.[0]
      const sel = slicerSel[wd.id] ?? []
      if (!dim || !sel.length) return null
      return { datasetId: cfg.dataset_id as string | undefined, column: dim, operator: sel.length > 1 ? 'in' : 'eq', value: sel.length > 1 ? sel : sel[0] }
    })
    .filter((x): x is SlicerFilter => x != null), [widgets, slicerSel])
  return (
    <div
      className="relative bg-white shadow-md mx-auto"
      style={{
        width: w, height: h, minHeight: 400, background: page?.background ?? '#ffffff',
        backgroundImage: ed.showGrid ? 'radial-gradient(circle, #e0e3e7 1px, transparent 1px)' : undefined,
        backgroundSize: ed.showGrid ? `${GRID * 2}px ${GRID * 2}px` : undefined,
      }}
      onMouseDown={(e) => { if (e.target === e.currentTarget) patch({ selectedIds: [] }) }}
    >
      {widgets.length === 0 && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 text-[#9aa0a6] pointer-events-none">
          <LayoutDashboard size={48} className="opacity-30" />
          <p className="text-sm">{t('data_canvas_empty', { defaultValue: 'Ajoutez un visuel depuis le volet Visualisations' })}</p>
        </div>
      )}
      {widgets.map(widget => (
        <CanvasWidget
          key={widget.id} widget={widget}
          selected={ed.selectedIds.includes(widget.id)}
          snapOn={ed.snapGrid}
          datasets={datasets} pageFilters={pageFilters} slicerFilters={slicerFilters} slicerSel={slicerSel[widget.id] ?? []}
          onSelect={(additive) => patch({ selectedIds: additive ? [...ed.selectedIds, widget.id] : [widget.id] })}
          onMove={onMove} onResize={onResize}
          onContext={(pos) => { patch({ selectedIds: [widget.id] }); onContext(widget.id, pos) }}
          onSlicer={(sel) => onSlicer(widget.id, sel)}
        />
      ))}
    </div>
  )
}

/** A cross-filter contributed by a slicer: applies to visuals sharing its dataset. */
interface SlicerFilter { datasetId?: string; column: string; operator: string; value: unknown }

function mergeFilters(widget: Widget, datasets: Dataset[], pageFilters: FilterRule[], slicerFilters: SlicerFilter[]): FilterRule[] {
  const cfg = widget.config as Record<string, unknown>
  const own = (cfg.filters as FilterRule[] | undefined) ?? []
  const ds = datasets.find(d => d.id === cfg.dataset_id)
  const cols = new Set(ds?.schema_cache?.map(c => c.name) ?? [])
  // Only apply page filters whose column exists in this widget's dataset.
  const page = pageFilters.filter(f => cols.size === 0 || cols.has(f.column))
  // Slicer cross-filters: same dataset + column present, and never on a slicer itself.
  const cross = SLICER_VISUALS.has(widget.widget_type) ? [] : slicerFilters
    .filter(sf => sf.datasetId === cfg.dataset_id && (cols.size === 0 || cols.has(sf.column)))
    .map(sf => ({ column: sf.column, operator: sf.operator, value: sf.value }))
  return [...own, ...page, ...cross]
}

function useWidgetData(widget: Widget, datasets: Dataset[], pageFilters: FilterRule[], slicerFilters: SlicerFilter[] = []) {
  const cfg = widget.config as Record<string, unknown>
  const filters = mergeFilters(widget, datasets, pageFilters, slicerFilters)
  return useQuery({
    queryKey: ['widget-data', widget.id, cfg, filters],
    enabled: !!cfg.dataset_id && !STATIC_VISUALS.has(widget.widget_type),
    queryFn: () => executeApi.query({
      dataset_id: cfg.dataset_id as string,
      dimensions: cfg.dimensions as string[],
      metrics: cfg.metrics as { column: string; function: string; alias?: string }[],
      filters: filters as { column: string; operator: string; value: unknown }[],
      sort: cfg.sort as { column: string; direction: string }[],
      limit: (cfg.limit as number) ?? 200,
    }),
  })
}

function CanvasWidget({ widget, selected, snapOn, datasets, pageFilters, slicerFilters, slicerSel, onSelect, onMove, onResize, onContext, onSlicer }: {
  widget: Widget; selected: boolean; snapOn: boolean
  datasets: Dataset[]; pageFilters: FilterRule[]; slicerFilters: SlicerFilter[]; slicerSel: string[]
  onSelect: (additive: boolean) => void
  onMove: (id: string, x: number, y: number) => void
  onResize: (id: string, w: number, h: number) => void
  onContext: (pos: MenuDropdownPos) => void
  onSlicer: (sel: string[]) => void
}) {
  const locked = !!widget.config.locked
  const { data: execData, isLoading, error } = useWidgetData(widget, datasets, pageFilters, slicerFilters)

  const drag = useRef<{ sx: number; sy: number; ox: number; oy: number } | null>(null)
  const onMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return
    if ((e.target as HTMLElement).closest('[data-no-drag]')) return
    onSelect(e.shiftKey || e.ctrlKey || e.metaKey)
    if (locked) return
    e.preventDefault()
    drag.current = { sx: e.clientX, sy: e.clientY, ox: widget.x, oy: widget.y }
    const el = document.getElementById(`w-${widget.id}`)
    const move = (ev: MouseEvent) => {
      if (!drag.current || !el) return
      el.style.left = `${snap(drag.current.ox + ev.clientX - drag.current.sx, snapOn)}px`
      el.style.top = `${snap(drag.current.oy + ev.clientY - drag.current.sy, snapOn)}px`
    }
    const up = (ev: MouseEvent) => {
      if (drag.current) {
        onMove(widget.id, Math.max(0, snap(drag.current.ox + ev.clientX - drag.current.sx, snapOn)), Math.max(0, snap(drag.current.oy + ev.clientY - drag.current.sy, snapOn)))
      }
      drag.current = null
      window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up)
    }
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up)
  }, [widget, locked, snapOn, onSelect, onMove])

  const resize = useRef<{ sx: number; sy: number; ow: number; oh: number } | null>(null)
  const onResizeDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault(); e.stopPropagation()
    resize.current = { sx: e.clientX, sy: e.clientY, ow: widget.width, oh: widget.height }
    const el = document.getElementById(`w-${widget.id}`)
    const move = (ev: MouseEvent) => {
      if (!resize.current || !el) return
      el.style.width = `${Math.max(80, snap(resize.current.ow + ev.clientX - resize.current.sx, snapOn))}px`
      el.style.height = `${Math.max(60, snap(resize.current.oh + ev.clientY - resize.current.sy, snapOn))}px`
    }
    const up = (ev: MouseEvent) => {
      if (resize.current) onResize(widget.id, Math.max(80, snap(resize.current.ow + ev.clientX - resize.current.sx, snapOn)), Math.max(60, snap(resize.current.oh + ev.clientY - resize.current.sy, snapOn)))
      resize.current = null
      window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up)
    }
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up)
  }, [widget, snapOn, onResize])

  // Slicer interactivity: clicking values toggles selection (drives cross-filter).
  const isSlicer = SLICER_VISUALS.has(widget.widget_type)
  const cfg = { ...widget.config, _selected: slicerSel }

  return (
    <div
      id={`w-${widget.id}`}
      className={clsx('absolute rounded-lg overflow-hidden bg-white border select-none transition-shadow',
        selected ? 'border-[#1a73e8] shadow-lg ring-1 ring-[#1a73e8]' : 'border-[#e8eaed] hover:border-[#bdc1c6]',
        locked ? 'cursor-default' : 'cursor-move')}
      style={{ left: widget.x, top: widget.y, width: widget.width, height: widget.height, zIndex: selected ? 100 : widget.z_index }}
      onMouseDown={onMouseDown}
      onContextMenu={(e) => { e.preventDefault(); onContext({ top: e.clientY, left: e.clientX }) }}
    >
      {selected && !locked && (
        <div className="absolute top-1 left-1 text-[#bdc1c6] z-10 pointer-events-none"><GripVertical size={13} /></div>
      )}
      {locked && <div className="absolute top-1 right-1 text-[#9aa0a6] z-10"><Lock size={11} /></div>}
      <div
        className="w-full h-full"
        data-no-drag={isSlicer ? '' : undefined}
        onClick={isSlicer ? undefined : undefined}
      >
        {isSlicer ? (
          <SlicerInteractive widget={widget} datasets={datasets} pageFilters={pageFilters} selected={slicerSel} onChange={onSlicer} />
        ) : (
          <WidgetRenderer widgetType={widget.widget_type} config={cfg as Record<string, unknown>} data={execData?.rows ?? []} isLoading={isLoading && !!widget.config.dataset_id} error={error ? String((error as Error).message ?? error) : undefined} />
        )}
      </div>
      {selected && !locked && (
        <div data-no-drag className="absolute bottom-0 right-0 w-3.5 h-3.5 cursor-se-resize z-20" onMouseDown={onResizeDown}>
          <div className="absolute bottom-0.5 right-0.5 w-2 h-2 border-r-2 border-b-2 border-[#1a73e8]" />
        </div>
      )}
    </div>
  )
}

function SlicerInteractive({ widget, datasets, pageFilters, selected, onChange }: {
  widget: Widget; datasets: Dataset[]; pageFilters: FilterRule[]; selected: string[]; onChange: (sel: string[]) => void
}) {
  const cfg = widget.config as Record<string, unknown>
  const dim = (cfg.dimensions as string[] | undefined)?.[0] ?? ''
  const { data } = useWidgetData({ ...widget, config: { ...cfg, dimensions: dim ? [dim] : [], metrics: [], limit: 500 } }, datasets, pageFilters)
  const values = useMemo(() => [...new Set((data?.rows ?? []).map(r => String(r[dim] ?? '')))].filter(Boolean), [data, dim])
  const toggle = (v: string) => onChange(selected.includes(v) ? selected.filter(x => x !== v) : [...selected, v])
  if (widget.widget_type === 'slicer_dropdown') {
    return (
      <div className="h-full p-3 flex flex-col gap-2">
        <p className="text-xs font-medium text-[#5f6368]">{(cfg.title as string) || dim || 'Segment'}</p>
        <select data-no-drag value={selected[0] ?? ''} onChange={e => onChange(e.target.value ? [e.target.value] : [])} className="border border-[#dadce0] rounded px-2 py-1.5 text-xs text-[#202124]">
          <option value="">Tous</option>{values.map(v => <option key={v} value={v}>{v}</option>)}
        </select>
      </div>
    )
  }
  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex items-center justify-between px-3 pt-2 pb-1 shrink-0">
        <p className="text-xs font-medium text-[#5f6368] truncate">{(cfg.title as string) || dim || 'Segment'}</p>
        {selected.length > 0 && <button data-no-drag onClick={() => onChange([])} className="text-[10px] text-[#1a73e8] hover:underline">Effacer</button>}
      </div>
      <div data-no-drag className="flex-1 overflow-auto px-2 pb-2">
        {values.map(v => (
          <label key={v} className="flex items-center gap-2 px-2 py-1 rounded hover:bg-[#f1f3f4] cursor-pointer text-xs text-[#202124]">
            <input type="checkbox" checked={selected.includes(v)} onChange={() => toggle(v)} className="accent-[#1a73e8]" />
            <span className="truncate">{v}</span>
          </label>
        ))}
        {!values.length && <p className="text-xs text-[#9aa0a6] px-2 py-2">Choisissez un champ</p>}
      </div>
    </div>
  )
}

function FocusOverlay({ widget, datasets, pageFilters, slicerSel, onClose }: {
  widget: Widget; datasets: Dataset[]; pageFilters: FilterRule[]; slicerSel: Record<string, string[]>; onClose: () => void
}) {
  const { data, isLoading, error } = useWidgetData(widget, datasets, pageFilters)
  void slicerSel
  return (
    <div className="absolute inset-0 z-[200] bg-black/40 flex items-center justify-center p-8" onClick={onClose}>
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-4xl h-full max-h-[80vh] flex flex-col overflow-hidden" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-4 py-2 border-b border-[#e8eaed]">
          <span className="text-sm font-medium text-[#202124]">{(widget.config.title as string) || 'Mode Focus'}</span>
          <button onClick={onClose} className="p-1 rounded hover:bg-[#f1f3f4] text-[#5f6368]"><X size={16} /></button>
        </div>
        <div className="flex-1 min-h-0 p-4">
          <WidgetRenderer widgetType={widget.widget_type} config={widget.config as Record<string, unknown>} data={data?.rows ?? []} isLoading={isLoading} error={error ? String(error) : undefined} />
        </div>
      </div>
    </div>
  )
}

// ── Pages bar ──────────────────────────────────────────────────────────────────

function PagesBar({ pages, activePageId, onSelect, onAdd, onContext }: {
  pages: ReportPage[]; activePageId: string; onSelect: (id: string) => void; onAdd: () => void; onContext: (id: string, pos: MenuDropdownPos) => void
}) {
  return (
    <div className="flex items-center gap-1 px-4 py-1.5 bg-white border-t border-[#e8eaed] shrink-0 overflow-x-auto">
      {pages.map(p => (
        <button key={p.id}
          onClick={() => onSelect(p.id)}
          onContextMenu={(e) => { e.preventDefault(); onContext(p.id, { top: e.clientY, left: e.clientX }) }}
          className={clsx('px-3 py-1 text-xs rounded whitespace-nowrap transition-colors',
            p.id === activePageId ? 'bg-[#e8f0fe] text-[#1a73e8] font-medium' : 'text-[#5f6368] hover:bg-[#f1f3f4]')}>
          {p.title}
        </button>
      ))}
      <button onClick={onAdd} className="p-1 text-[#9aa0a6] hover:text-[#5f6368] hover:bg-[#f1f3f4] rounded shrink-0" title="Nouvelle page"><Plus size={14} /></button>
    </div>
  )
}

// ════════════════════════════════════════════════════════════════════════════
// Panes
// ════════════════════════════════════════════════════════════════════════════

function FieldsPane({ datasets, selected, onAddField, onSwitchView }: {
  datasets: Dataset[]; selected: Widget | null; onAddField: (col: string, kind: 'dimension' | 'metric') => void; onSwitchView?: (v: 'query' | 'model') => void
}) {
  const [open, setOpen] = useState<Record<string, boolean>>({})
  const [q, setQ] = useState('')
  return (
    <div className="h-full overflow-y-auto bg-white">
      <div className="p-2 sticky top-0 bg-white z-10 border-b border-[#f1f3f4] flex items-center gap-1.5">
        <div className="flex items-center gap-1.5 px-2 py-1 rounded bg-[#f1f3f4] flex-1">
          <Search size={13} className="text-[#9aa0a6]" />
          <input value={q} onChange={e => setQ(e.target.value)} placeholder="Rechercher un champ" className="bg-transparent text-xs outline-none flex-1 text-[#202124]" />
        </div>
        <button onClick={() => onSwitchView?.('query')} className="p-1 rounded hover:bg-[#e8eaed] text-[#5f6368] shrink-0" title="Gérer les données"><Plus size={14} /></button>
      </div>
      {datasets.length === 0 && <p className="text-xs text-[#9aa0a6] p-3">Aucun jeu de données. Ouvrez l'onglet « Données » pour en créer un.</p>}
      {datasets.map(ds => {
        const cols = (ds.schema_cache ?? []).filter(c => c.name.toLowerCase().includes(q.toLowerCase()))
        const isOpen = open[ds.id] ?? true
        return (
          <div key={ds.id} className="border-b border-[#f1f3f4]">
            <button onClick={() => setOpen(o => ({ ...o, [ds.id]: !isOpen }))} className="w-full flex items-center gap-1.5 px-2.5 py-1.5 hover:bg-[#f8f9fa] text-left">
              <ChevronDown size={13} className={clsx('text-[#9aa0a6] transition-transform', !isOpen && '-rotate-90')} />
              <span className="text-xs font-medium text-[#202124] truncate">{ds.name}</span>
              <span className="ml-auto text-[10px] text-[#9aa0a6]">{cols.length}</span>
            </button>
            {isOpen && cols.map(c => {
              const numeric = /int|num|float|double|decimal|money|real/i.test(c.type)
              return (
                <div key={c.name} draggable
                  onDragStart={e => e.dataTransfer.setData('text/kbfield', JSON.stringify({ column: c.name, kind: numeric ? 'metric' : 'dimension' }))}
                  className="group flex items-center gap-2 pl-7 pr-2 py-1 hover:bg-[#e8f0fe] cursor-grab text-xs text-[#202124]">
                  {numeric ? <Sigma size={12} className="text-[#1a73e8] shrink-0" /> : <TypeIcon size={12} className="text-[#9aa0a6] shrink-0" />}
                  <span className="truncate flex-1">{c.name}</span>
                  {selected && (
                    <button onClick={() => onAddField(c.name, numeric ? 'metric' : 'dimension')} title="Ajouter au visuel"
                      className="opacity-0 group-hover:opacity-100 text-[#1a73e8] shrink-0"><Plus size={13} /></button>
                  )}
                </div>
              )
            })}
          </div>
        )
      })}
    </div>
  )
}

function VisualizationsPane({ selected, onAddVisual, onChangeType, datasets, onConfig, onAssign }: {
  selected: Widget | null; onAddVisual: (type: string) => void; onChangeType: (type: string) => void
  datasets: Dataset[]; onConfig: (id: string, p: Record<string, unknown>) => void; onAssign: (col: string, kind: 'dimension' | 'metric') => void
}) {
  return (
    <div className="h-full overflow-y-auto bg-white">
      {/* Visual type gallery */}
      <div className="p-2.5 border-b border-[#e8eaed]">
        <p className="text-[11px] font-semibold text-[#5f6368] uppercase tracking-wide mb-2">Type de visuel</p>
        {VISUAL_CATEGORIES.map(cat => (
          <div key={cat.id} className="mb-2">
            <p className="text-[10px] text-[#9aa0a6] mb-1">{cat.label}</p>
            <div className="grid grid-cols-6 gap-1">
              {VISUALS.filter(v => v.category === cat.id).map(v => (
                <button key={v.type} title={v.label}
                  onClick={() => selected ? onChangeType(v.type) : onAddVisual(v.type)}
                  className={clsx('flex items-center justify-center aspect-square rounded border transition-colors',
                    selected?.widget_type === v.type ? 'border-[#1a73e8] bg-[#e8f0fe] text-[#1a73e8]' : 'border-[#e8eaed] text-[#5f6368] hover:border-[#1a73e8] hover:bg-[#f8f9ff]')}>
                  {v.icon}
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
      {/* Field wells */}
      {selected ? (
        <WellsEditor selected={selected} datasets={datasets} onConfig={onConfig} onAssign={onAssign} />
      ) : (
        <p className="text-xs text-[#9aa0a6] p-3">Sélectionnez un visuel pour configurer ses champs, ou cliquez sur un type pour l'ajouter.</p>
      )}
    </div>
  )
}

function WellsEditor({ selected, datasets, onConfig, onAssign }: {
  selected: Widget; datasets: Dataset[]; onConfig: (id: string, p: Record<string, unknown>) => void; onAssign: (col: string, kind: 'dimension' | 'metric') => void
}) {
  const cfg = selected.config as Record<string, unknown>
  const wells = wellsFor(selected.widget_type)
  const ds = datasets.find(d => d.id === cfg.dataset_id)
  const dims = (cfg.dimensions as string[] | undefined) ?? []
  const metrics = (cfg.metrics as { column: string; function: string; alias?: string }[] | undefined) ?? []

  const dropField = (well: Well, e: React.DragEvent) => {
    e.preventDefault()
    const raw = e.dataTransfer.getData('text/kbfield'); if (!raw) return
    const { column } = JSON.parse(raw) as { column: string; kind: string }
    onAssign(column, well.kind)
  }

  const chips = (well: Well): { label: string; onRemove: () => void; agg?: string; onAgg?: (v: string) => void }[] => {
    if (well.kind === 'metric') {
      const list = well.id === 'size' ? metrics.slice(1, 2) : (well.multi ? metrics : metrics.slice(0, 1))
      return list.map((m) => ({
        label: m.alias || m.column, agg: m.function,
        onAgg: (v: string) => onConfig(selected.id, { metrics: metrics.map(x => x === m ? { ...x, function: v } : x) }),
        onRemove: () => onConfig(selected.id, { metrics: metrics.filter(x => x !== m) }),
      }))
    }
    const val = well.id === 'legend' ? dims[1] : dims[0]
    if (!val) return []
    return [{ label: val, onRemove: () => onConfig(selected.id, { dimensions: well.id === 'legend' ? [dims[0]] : [dims[1]].filter(Boolean) }) }]
  }

  return (
    <div className="p-2.5 space-y-3">
      <div>
        <label className="text-[11px] font-semibold text-[#5f6368] uppercase block mb-1">Jeu de données</label>
        <Dropdown value={(cfg.dataset_id as string) ?? ''} onChange={v => onConfig(selected.id, { dataset_id: v, dimensions: [], metrics: [] })}
          width="100%" placeholder="Choisir…" options={[{ value: '', label: 'Choisir…' }, ...datasets.map(d => ({ value: d.id, label: d.name }))]} />
        {!!cfg.dataset_id && ds?.status !== 'ready' && <p className="text-[10px] text-[#f9ab00] mt-1">Jeu non chargé — actualisez-le.</p>}
      </div>
      {wells.map(well => (
        <div key={well.id + well.label}
          onDragOver={e => e.preventDefault()} onDrop={e => dropField(well, e)}
          className="rounded border border-dashed border-[#dadce0] p-1.5 min-h-[40px]">
          <p className="text-[10px] font-medium text-[#5f6368] mb-1 flex items-center gap-1">{well.label}{well.kind === 'metric' ? <Sigma size={10} /> : <TypeIcon size={10} />}</p>
          <div className="space-y-1">
            {chips(well).map((c, i) => (
              <div key={i} className="flex items-center gap-1 bg-[#e8f0fe] rounded px-1.5 py-1 text-xs text-[#1a73e8]">
                {c.onAgg && (
                  <select value={c.agg} onChange={e => c.onAgg!(e.target.value)} className="bg-transparent text-[10px] outline-none text-[#1a73e8] font-medium">
                    {AGG_FUNCTIONS.map(f => <option key={f.value} value={f.value}>{f.label}</option>)}
                  </select>
                )}
                <span className="truncate flex-1">{c.label}</span>
                <button onClick={c.onRemove} className="text-[#5f6368] hover:text-[#d93025]"><X size={12} /></button>
              </div>
            ))}
            {chips(well).length === 0 && <p className="text-[10px] text-[#bdc1c6] px-1">Glissez un champ ici</p>}
          </div>
        </div>
      ))}
    </div>
  )
}

// ── Format pane ─────────────────────────────────────────────────────────────────

function PropGroup({ title, children, defaultOpen = true }: { title: string; children: ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <div className="border-b border-[#e8eaed]">
      <button onClick={() => setOpen(o => !o)} className="w-full flex items-center justify-between px-3 py-2 hover:bg-[#f8f9fa]">
        <span className="text-[11px] font-semibold text-[#5f6368] uppercase tracking-wide">{title}</span>
        <ChevronDown size={13} className={clsx('text-[#9aa0a6] transition-transform', !open && '-rotate-90')} />
      </button>
      {open && <div className="px-3 pb-3 space-y-2.5">{children}</div>}
    </div>
  )
}
function PropRow({ label, children }: { label: string; children: ReactNode }) {
  return <div className="flex items-center justify-between gap-2"><span className="text-xs text-[#5f6368]">{label}</span><div className="shrink-0">{children}</div></div>
}
function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button onClick={() => onChange(!on)} className={clsx('w-9 h-5 rounded-full transition-colors relative', on ? 'bg-[#1a73e8]' : 'bg-[#dadce0]')}>
      <span className={clsx('absolute top-0.5 w-4 h-4 rounded-full bg-white transition-all', on ? 'left-[18px]' : 'left-0.5')} />
    </button>
  )
}
function ColorDot({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return <input type="color" value={value} onChange={e => onChange(e.target.value)} className="w-7 h-7 rounded border border-[#dadce0] cursor-pointer p-0" />
}

function FormatPane({ selected, onConfig }: { selected: Widget | null; onConfig: (id: string, p: Record<string, unknown>) => void }) {
  if (!selected) return <p className="text-xs text-[#9aa0a6] p-3">Sélectionnez un visuel pour le mettre en forme.</p>
  const cfg = selected.config as Record<string, unknown>
  const set = (p: Record<string, unknown>) => onConfig(selected.id, p)
  const ds = cfg.dataset_id as string | undefined
  void ds
  const isChart = !['text', 'image', 'shape'].includes(selected.widget_type)
  return (
    <div className="h-full overflow-y-auto bg-white">
      <PropGroup title="Général">
        <div>
          <label className="text-xs text-[#5f6368] block mb-1">Titre</label>
          <Input defaultValue={(cfg.title as string) ?? ''} onBlur={e => set({ title: e.target.value })} />
        </div>
        <PropRow label="Afficher le titre"><Toggle on={cfg.showTitle !== false} onChange={v => set({ showTitle: v })} /></PropRow>
        <PropRow label="Arrière-plan"><ColorDot value={(cfg.background as string) ?? '#ffffff'} onChange={v => set({ background: v })} /></PropRow>
        <PropRow label="Bordure"><Toggle on={cfg.border !== false} onChange={v => set({ border: v })} /></PropRow>
      </PropGroup>

      {isChart && (
        <>
          <PropGroup title="Données">
            <PropRow label="Format numérique">
              <select value={(cfg.format as string) ?? 'auto'} onChange={e => set({ format: e.target.value })} className="border border-[#dadce0] rounded px-1.5 py-1 text-xs text-[#202124] max-w-[130px]">
                {NUMBER_FORMATS.map(f => <option key={f.value} value={f.value}>{f.label}</option>)}
              </select>
            </PropRow>
            <div>
              <label className="text-xs text-[#5f6368] block mb-1">Nb de lignes max</label>
              <Input type="number" defaultValue={(cfg.limit as number) ?? 200} onBlur={e => set({ limit: parseInt(e.target.value) || 200 })} />
            </div>
            <div className="flex gap-2">
              <div className="flex-1"><label className="text-xs text-[#5f6368] block mb-1">Préfixe</label><Input defaultValue={(cfg.prefix as string) ?? ''} onBlur={e => set({ prefix: e.target.value })} /></div>
              <div className="flex-1"><label className="text-xs text-[#5f6368] block mb-1">Suffixe</label><Input defaultValue={(cfg.suffix as string) ?? ''} onBlur={e => set({ suffix: e.target.value })} /></div>
            </div>
          </PropGroup>

          <PropGroup title="Couleurs">
            <label className="text-xs text-[#5f6368] block mb-1">Palette</label>
            <div className="grid grid-cols-2 gap-1.5">
              {PALETTES.map(p => (
                <button key={p.id} onClick={() => set({ paletteId: p.id, palette: p.colors })}
                  className={clsx('flex items-center gap-1 p-1 rounded border', cfg.paletteId === p.id ? 'border-[#1a73e8]' : 'border-[#e8eaed] hover:border-[#bdc1c6]')}>
                  <span className="flex">{p.colors.slice(0, 5).map((c, i) => <span key={i} className="w-2.5 h-3.5" style={{ background: c }} />)}</span>
                  <span className="text-[9px] text-[#5f6368] truncate">{p.name}</span>
                </button>
              ))}
            </div>
          </PropGroup>

          <PropGroup title="Éléments visuels">
            <PropRow label="Étiquettes de données"><Toggle on={!!cfg.dataLabels} onChange={v => set({ dataLabels: v })} /></PropRow>
            <PropRow label="Légende"><Toggle on={cfg.legend !== false} onChange={v => set({ legend: v })} /></PropRow>
            <PropRow label="Quadrillage"><Toggle on={cfg.gridlines !== false} onChange={v => set({ gridlines: v })} /></PropRow>
            {selected.widget_type === 'scatter_chart' && <PropRow label="Courbe de tendance"><Toggle on={!!cfg.trendline} onChange={v => set({ trendline: v })} /></PropRow>}
            {(selected.widget_type === 'data_table' || selected.widget_type === 'matrix') && <PropRow label="Lignes alternées"><Toggle on={cfg.banded !== false} onChange={v => set({ banded: v })} /></PropRow>}
            {selected.widget_type === 'matrix' && <PropRow label="Totaux"><Toggle on={cfg.showTotals !== false} onChange={v => set({ showTotals: v })} /></PropRow>}
          </PropGroup>

          {(selected.widget_type === 'gauge' || selected.widget_type === 'bullet' || selected.widget_type === 'progress_ring') && (
            <PropGroup title="Jauge">
              <div className="flex gap-2">
                <div className="flex-1"><label className="text-xs text-[#5f6368] block mb-1">Min</label><Input type="number" defaultValue={(cfg.gaugeMin as number) ?? 0} onBlur={e => set({ gaugeMin: parseFloat(e.target.value) || 0 })} /></div>
                <div className="flex-1"><label className="text-xs text-[#5f6368] block mb-1">Max</label><Input type="number" defaultValue={(cfg.gaugeMax as number) ?? 100} onBlur={e => set({ gaugeMax: parseFloat(e.target.value) || 100 })} /></div>
              </div>
              <div><label className="text-xs text-[#5f6368] block mb-1">Objectif</label><Input type="number" defaultValue={(cfg.target as number) ?? 0} onBlur={e => set({ target: parseFloat(e.target.value) || 0 })} /></div>
            </PropGroup>
          )}
        </>
      )}

      {selected.widget_type === 'text' && (
        <PropGroup title="Texte">
          <div><label className="text-xs text-[#5f6368] block mb-1">Contenu</label><textarea defaultValue={(cfg.content as string) ?? ''} onBlur={e => set({ content: e.target.value })} className="w-full border border-[#dadce0] rounded p-2 text-xs h-24 resize-none" /></div>
          <PropRow label="Taille"><Input type="number" className="w-16" defaultValue={(cfg.fontSize as number) ?? 14} onBlur={e => set({ fontSize: parseInt(e.target.value) || 14 })} /></PropRow>
          <PropRow label="Gras"><Toggle on={!!cfg.bold} onChange={v => set({ bold: v })} /></PropRow>
          <PropRow label="Couleur"><ColorDot value={(cfg.textColor as string) ?? '#202124'} onChange={v => set({ textColor: v })} /></PropRow>
          <PropRow label="Alignement">
            <select value={(cfg.align as string) ?? 'left'} onChange={e => set({ align: e.target.value })} className="border border-[#dadce0] rounded px-1.5 py-1 text-xs"><option value="left">Gauche</option><option value="center">Centre</option><option value="right">Droite</option></select>
          </PropRow>
        </PropGroup>
      )}
      {selected.widget_type === 'image' && (
        <PropGroup title="Image">
          <div><label className="text-xs text-[#5f6368] block mb-1">URL</label><Input defaultValue={(cfg.imageUrl as string) ?? ''} onBlur={e => set({ imageUrl: e.target.value })} placeholder="https://…" /></div>
          <PropRow label="Ajustement"><select value={(cfg.fit as string) ?? 'contain'} onChange={e => set({ fit: e.target.value })} className="border border-[#dadce0] rounded px-1.5 py-1 text-xs"><option value="contain">Contenir</option><option value="cover">Couvrir</option></select></PropRow>
        </PropGroup>
      )}
      {selected.widget_type === 'shape' && (
        <PropGroup title="Forme">
          <PropRow label="Type"><select value={(cfg.shapeKind as string) ?? 'rectangle'} onChange={e => set({ shapeKind: e.target.value })} className="border border-[#dadce0] rounded px-1.5 py-1 text-xs"><option value="rectangle">Rectangle</option><option value="ellipse">Ellipse</option><option value="triangle">Triangle</option><option value="line">Ligne</option></select></PropRow>
          <PropRow label="Remplissage"><ColorDot value={(cfg.fillColor as string) ?? '#e8f0fe'} onChange={v => set({ fillColor: v })} /></PropRow>
          <PropRow label="Contour"><ColorDot value={(cfg.strokeColor as string) ?? '#1a73e8'} onChange={v => set({ strokeColor: v })} /></PropRow>
        </PropGroup>
      )}
    </div>
  )
}

// ── Filters pane ─────────────────────────────────────────────────────────────────

function FiltersPane({ datasets, rules, onChange, selected, onConfig }: {
  datasets: Dataset[]; rules: FilterRule[]; onChange: (rules: FilterRule[]) => void; selected: Widget | null; onConfig: (id: string, p: Record<string, unknown>) => void
}) {
  const allCols = useMemo(() => {
    const set = new Map<string, string>()
    datasets.forEach(d => (d.schema_cache ?? []).forEach(c => set.set(c.name, c.type)))
    return [...set.keys()]
  }, [datasets])
  const add = () => onChange([...rules, { column: allCols[0] ?? '', operator: 'eq', value: '' }])
  const upd = (i: number, p: Partial<FilterRule>) => onChange(rules.map((r, j) => j === i ? { ...r, ...p } : r))
  const del = (i: number) => onChange(rules.filter((_, j) => j !== i))

  return (
    <div className="h-full overflow-y-auto bg-white">
      <div className="flex items-center justify-between px-3 h-8 border-b border-[#e8eaed] bg-[#f8f9fa] sticky top-0 z-10">
        <span className="text-[11px] font-semibold text-[#5f6368] uppercase tracking-wide flex items-center gap-1.5"><ListFilter size={13} /> Filtres sur cette page</span>
        <button onClick={add} className="p-0.5 rounded hover:bg-[#e8eaed] text-[#5f6368]" title="Ajouter un filtre"><Plus size={14} /></button>
      </div>
      <div className="p-2.5">
        {rules.length === 0 && <p className="text-xs text-[#9aa0a6]">Aucun filtre. Les filtres s'appliquent à tous les visuels de la page.</p>}
        {rules.map((r, i) => (
          <div key={i} className="rounded border border-[#e8eaed] p-2 mb-2 space-y-1.5">
            <div className="flex items-center gap-1">
              <select value={r.column} onChange={e => upd(i, { column: e.target.value })} className="flex-1 border border-[#dadce0] rounded px-1.5 py-1 text-xs min-w-0">
                {allCols.map(c => <option key={c} value={c}>{c}</option>)}
              </select>
              <button onClick={() => del(i)} className="text-[#9aa0a6] hover:text-[#d93025] shrink-0"><Trash2 size={13} /></button>
            </div>
            <select value={r.operator} onChange={e => upd(i, { operator: e.target.value })} className="w-full border border-[#dadce0] rounded px-1.5 py-1 text-xs">
              {FILTER_OPERATORS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
            {!['is_null', 'is_not_null'].includes(r.operator) && (
              <Input defaultValue={String(r.value ?? '')} onBlur={e => upd(i, { value: e.target.value })} placeholder="Valeur" />
            )}
          </div>
        ))}
      </div>
      {selected && (
        <div className="border-t border-[#e8eaed]">
          <div className="flex items-center px-3 h-8 bg-[#f8f9fa]">
            <span className="text-[11px] font-semibold text-[#5f6368] uppercase tracking-wide flex items-center gap-1.5"><FilterIcon size={13} /> Filtres sur ce visuel</span>
          </div>
          <div className="p-2.5"><VisualFilters selected={selected} datasets={datasets} onConfig={onConfig} /></div>
        </div>
      )}
    </div>
  )
}

function VisualFilters({ selected, datasets, onConfig }: { selected: Widget; datasets: Dataset[]; onConfig: (id: string, p: Record<string, unknown>) => void }) {
  const cfg = selected.config as Record<string, unknown>
  const ds = datasets.find(d => d.id === cfg.dataset_id)
  const cols = (ds?.schema_cache ?? []).map(c => c.name)
  const filters = (cfg.filters as FilterRule[] | undefined) ?? []
  const add = () => onConfig(selected.id, { filters: [...filters, { column: cols[0] ?? '', operator: 'eq', value: '' }] })
  const upd = (i: number, p: Partial<FilterRule>) => onConfig(selected.id, { filters: filters.map((r, j) => j === i ? { ...r, ...p } : r) })
  const del = (i: number) => onConfig(selected.id, { filters: filters.filter((_, j) => j !== i) })
  return (
    <div>
      <button onClick={add} className="text-xs text-[#1a73e8] mb-2 flex items-center gap-1"><Plus size={12} /> Ajouter un filtre</button>
      {filters.map((r, i) => (
        <div key={i} className="rounded border border-[#e8eaed] p-2 mb-2 space-y-1.5">
          <div className="flex items-center gap-1">
            <select value={r.column} onChange={e => upd(i, { column: e.target.value })} className="flex-1 border border-[#dadce0] rounded px-1.5 py-1 text-xs min-w-0">{cols.map(c => <option key={c} value={c}>{c}</option>)}</select>
            <button onClick={() => del(i)} className="text-[#9aa0a6] hover:text-[#d93025]"><Trash2 size={13} /></button>
          </div>
          <select value={r.operator} onChange={e => upd(i, { operator: e.target.value })} className="w-full border border-[#dadce0] rounded px-1.5 py-1 text-xs">{FILTER_OPERATORS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select>
          {!['is_null', 'is_not_null'].includes(r.operator) && <Input defaultValue={String(r.value ?? '')} onBlur={e => upd(i, { value: e.target.value })} placeholder="Valeur" />}
        </div>
      ))}
    </div>
  )
}

// ════════════════════════════════════════════════════════════════════════════
// Context menus
// ════════════════════════════════════════════════════════════════════════════

type TFn = (k: string, o?: Record<string, unknown>) => string

function widgetMenu(widget: Widget | undefined, h: {
  t: TFn; onCopy: () => void; onPaste: () => void; onDelete: () => void; onFront: () => void; onBack: () => void
  onDuplicate: () => void; onLock: () => void; onFocus: () => void; onFormat: () => void; onChangeType: (t: string) => void
}): MenuItem[] {
  if (!widget) return []
  const locked = !!widget.config.locked
  const typeSubmenu: MenuItem[] = VISUAL_CATEGORIES.map(cat => ({
    type: 'submenu', label: cat.label,
    items: VISUALS.filter(v => v.category === cat.id).map(v => ({ type: 'action' as const, label: v.label, icon: v.icon, checked: widget.widget_type === v.type, onClick: () => h.onChangeType(v.type) })),
  }))
  return [
    { type: 'action', label: h.t('common_copy', { defaultValue: 'Copier' }), icon: <Copy size={15} />, shortcut: 'Ctrl+C', onClick: h.onCopy },
    { type: 'action', label: h.t('common_duplicate', { defaultValue: 'Dupliquer' }), icon: <ClipboardPaste size={15} />, onClick: h.onDuplicate },
    { type: 'action', label: h.t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={15} />, danger: true, shortcut: 'Suppr', onClick: h.onDelete },
    { type: 'separator' },
    { type: 'submenu', label: h.t('data_change_type', { defaultValue: 'Changer le type de visuel' }), icon: <LayoutDashboard size={15} />, items: typeSubmenu },
    { type: 'action', label: h.t('data_format', { defaultValue: 'Mettre en forme' }), icon: <Palette size={15} />, onClick: h.onFormat },
    { type: 'action', label: h.t('data_focus_mode', { defaultValue: 'Mode Focus' }), icon: <Maximize2 size={15} />, onClick: h.onFocus },
    { type: 'separator' },
    { type: 'action', label: h.t('data_bring_front', { defaultValue: 'Premier plan' }), icon: <BringToFront size={15} />, onClick: h.onFront },
    { type: 'action', label: h.t('data_send_back', { defaultValue: 'Arrière-plan' }), icon: <SendToBack size={15} />, onClick: h.onBack },
    { type: 'action', label: locked ? h.t('data_unlock', { defaultValue: 'Déverrouiller' }) : h.t('data_lock', { defaultValue: 'Verrouiller' }), icon: locked ? <Unlock size={15} /> : <Lock size={15} />, onClick: h.onLock },
  ]
}

function pageMenu(page: ReportPage | undefined, h: {
  t: TFn; onDuplicate: () => void; onDelete: () => void; onRenameInline: (name: string) => void
}): MenuItem[] {
  if (!page) return []
  return [
    { type: 'custom', render: (close) => (
      <div className="px-2 py-1.5">
        <input autoFocus defaultValue={page.title} onKeyDown={e => { if (e.key === 'Enter') { h.onRenameInline((e.target as HTMLInputElement).value); close() } }}
          onBlur={e => { h.onRenameInline(e.target.value); }} className="w-full border border-[#dadce0] rounded px-2 py-1 text-xs" placeholder="Renommer la page" />
      </div>
    ) },
    { type: 'separator' },
    { type: 'action', label: h.t('common_duplicate', { defaultValue: 'Dupliquer la page' }), icon: <Copy size={15} />, onClick: h.onDuplicate },
    { type: 'action', label: h.t('common_delete', { defaultValue: 'Supprimer la page' }), icon: <Trash2 size={15} />, danger: true, onClick: h.onDelete },
  ]
}

// ════════════════════════════════════════════════════════════════════════════
// Ribbon
// ════════════════════════════════════════════════════════════════════════════

function useDataRibbon(a: {
  t: TFn; ed: EditorView; patch: (p: Partial<EditorView>) => void; selected: Widget | null; datasets: Dataset[]
  openPane: (id: string) => void
  onAddVisual: (type: string) => void; onRefresh: () => void; onSwitchView?: (v: 'query' | 'model') => void
  onCopy: () => void; onPaste: () => void; onDelete: () => void
  onAlign: (m: 'left' | 'center' | 'right' | 'distribute') => void; onZ: (d: 'front' | 'back') => void; onLock: () => void
  onTheme: (id: string) => void; report: Report; reportId: string; qc: ReturnType<typeof useQueryClient>
}): RibbonTab[] {
  const { t, ed, patch, selected, openPane } = a
  const hasSel = !!selected

  const home: RibbonTab = {
    id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
    groups: [
      { id: 'clip', label: t('doc_grp_clipboard', { defaultValue: 'Presse-papiers' }), items: [
        { id: 'paste', kind: 'button', size: 'large', icon: <ClipboardPaste size={20} />, label: t('common_paste', { defaultValue: 'Coller' }), onClick: a.onPaste },
        { id: 'copy', kind: 'button', icon: <Copy size={15} />, label: t('common_copy', { defaultValue: 'Copier' }), onClick: a.onCopy, disabled: !hasSel },
        { id: 'del', kind: 'button', icon: <Trash2 size={15} />, label: t('common_delete', { defaultValue: 'Supprimer' }), onClick: a.onDelete, disabled: !hasSel },
      ] },
      { id: 'insert', label: t('data_grp_insert', { defaultValue: 'Insérer' }), items: [
        { id: 'visual', kind: 'split', size: 'large', icon: <LayoutDashboard size={20} />, label: t('data_new_visual', { defaultValue: 'Nouveau visuel' }), onClick: () => openPane('visual'),
          splitItems: ['bar_chart', 'line_chart', 'pie_chart', 'kpi_card', 'data_table', 'matrix', 'treemap', 'gauge'].map(ty => ({ id: ty, kind: 'button' as const, label: VISUALS.find(v => v.type === ty)?.label ?? ty, onClick: () => a.onAddVisual(ty) })) },
        { id: 'text', kind: 'button', icon: <TypeIcon size={15} />, label: t('data_text_box', { defaultValue: 'Zone de texte' }), onClick: () => a.onAddVisual('text') },
        { id: 'image', kind: 'button', icon: <ImageIcon size={15} />, label: t('data_image', { defaultValue: 'Image' }), onClick: () => a.onAddVisual('image') },
        { id: 'shape', kind: 'button', icon: <Square size={15} />, label: t('data_shape', { defaultValue: 'Forme' }), onClick: () => a.onAddVisual('shape') },
        { id: 'slicer', kind: 'button', icon: <FilterIcon size={15} />, label: t('data_slicer', { defaultValue: 'Segment' }), onClick: () => a.onAddVisual('slicer') },
      ] },
      { id: 'data', label: t('data_tab_data', { defaultValue: 'Données' }), items: [
        { id: 'refresh', kind: 'button', size: 'large', icon: <RefreshCw size={20} />, label: t('data_refresh', { defaultValue: 'Actualiser' }), onClick: a.onRefresh },
        { id: 'transform', kind: 'button', icon: <Database size={15} />, label: t('data_transform', { defaultValue: 'Transformer' }), onClick: () => a.onSwitchView?.('query') },
        { id: 'model', kind: 'button', icon: <Network size={15} />, label: t('data_tab_model', { defaultValue: 'Modèle' }), onClick: () => a.onSwitchView?.('model') },
      ] },
      { id: 'themes', label: t('data_grp_theme', { defaultValue: 'Thème' }), items: [
        { id: 'theme', kind: 'gallery', options: REPORT_THEMES.slice(0, 6).map(th => ({ value: th.id, label: th.name, icon: <span className="w-5 h-5 rounded-full block" style={{ background: th.primaryColor }} title={th.name} /> })), onChange: a.onTheme },
      ] },
    ],
  }

  const insert: RibbonTab = {
    id: 'insert', label: t('data_tab_insert', { defaultValue: 'Insertion' }),
    groups: VISUAL_CATEGORIES.slice(0, 6).map(cat => ({
      id: cat.id, label: cat.label,
      items: VISUALS.filter(v => v.category === cat.id).slice(0, 6).map(v => ({ id: v.type, kind: 'button' as const, icon: v.icon, tooltip: v.label, onClick: () => a.onAddVisual(v.type) })),
    })),
  }

  const model: RibbonTab = {
    id: 'modeling', label: t('data_tab_modeling', { defaultValue: 'Modélisation' }),
    groups: [
      { id: 'calc', label: t('data_grp_calc', { defaultValue: 'Calculs' }), items: [
        { id: 'measure', kind: 'button', size: 'large', icon: <Sigma size={20} />, label: t('data_new_measure', { defaultValue: 'Nouvelle mesure' }), onClick: () => a.onSwitchView?.('model') },
        { id: 'column', kind: 'button', icon: <Plus size={15} />, label: t('data_new_column', { defaultValue: 'Colonne calculée' }), onClick: () => a.onSwitchView?.('query') },
      ] },
      { id: 'rel', label: t('data_grp_relations', { defaultValue: 'Relations' }), items: [
        { id: 'manage', kind: 'button', size: 'large', icon: <Network size={20} />, label: t('data_manage_relations', { defaultValue: 'Gérer les relations' }), onClick: () => a.onSwitchView?.('model') },
      ] },
      { id: 'src', label: t('data_grp_sources', { defaultValue: 'Sources' }), items: [
        { id: 'newtable', kind: 'button', size: 'large', icon: <Database size={20} />, label: t('data_new_dataset', { defaultValue: 'Nouvelle table' }), onClick: () => a.onSwitchView?.('query') },
      ] },
    ],
  }

  const view: RibbonTab = {
    id: 'view', label: t('data_tab_view', { defaultValue: 'Affichage' }),
    groups: [
      { id: 'show', label: t('data_grp_show', { defaultValue: 'Afficher' }), items: [
        { id: 'grid', kind: 'toggle', icon: <Grid2x2 size={15} />, label: t('data_view_grid', { defaultValue: 'Quadrillage' }), active: ed.showGrid, onClick: () => patch({ showGrid: !ed.showGrid }) },
        { id: 'snap', kind: 'toggle', icon: <Magnet size={15} />, label: t('data_view_snap', { defaultValue: 'Aligner' }), active: ed.snapGrid, onClick: () => patch({ snapGrid: !ed.snapGrid }) },
      ] },
      { id: 'panes', label: t('data_grp_panes', { defaultValue: 'Volets' }), items: [
        { id: 'fields', kind: 'button', icon: <Database size={15} />, label: t('data_pane_fields', { defaultValue: 'Champs' }), onClick: () => openPane('fields') },
        { id: 'viz', kind: 'button', icon: <LayoutDashboard size={15} />, label: t('data_pane_viz', { defaultValue: 'Visualisations' }), onClick: () => openPane('visual') },
        { id: 'filters', kind: 'button', icon: <ListFilter size={15} />, label: t('data_pane_filters', { defaultValue: 'Filtres' }), onClick: () => openPane('filter') },
        { id: 'fmt', kind: 'button', icon: <Palette size={15} />, label: t('data_format', { defaultValue: 'Format' }), onClick: () => openPane('format') },
      ] },
      { id: 'fit', label: t('data_grp_fit', { defaultValue: 'Affichage' }), items: [
        { id: 'focus', kind: 'button', size: 'large', icon: <Maximize2 size={20} />, label: t('data_focus_mode', { defaultValue: 'Mode Focus' }), onClick: () => selected && patch({ focusId: selected.id }), disabled: !hasSel },
      ] },
    ],
  }

  const format: RibbonTab = {
    id: 'format', label: t('data_tab_format', { defaultValue: 'Format' }),
    contextual: { accent: '#a142f4' }, visible: hasSel,
    groups: [
      { id: 'align', label: t('data_grp_align', { defaultValue: 'Aligner' }), items: [
        { id: 'l', kind: 'button', icon: <AlignStartVertical size={15} />, tooltip: 'Gauche', onClick: () => a.onAlign('left') },
        { id: 'c', kind: 'button', icon: <AlignCenterVertical size={15} />, tooltip: 'Centrer', onClick: () => a.onAlign('center') },
        { id: 'r', kind: 'button', icon: <AlignEndVertical size={15} />, tooltip: 'Droite', onClick: () => a.onAlign('right') },
        { id: 'd', kind: 'button', icon: <AlignHorizontalDistributeCenter size={15} />, tooltip: 'Répartir', onClick: () => a.onAlign('distribute') },
      ] },
      { id: 'arrange', label: t('data_grp_arrange', { defaultValue: 'Disposer' }), items: [
        { id: 'front', kind: 'button', icon: <BringToFront size={15} />, label: 'Premier plan', onClick: () => a.onZ('front') },
        { id: 'back', kind: 'button', icon: <SendToBack size={15} />, label: 'Arrière-plan', onClick: () => a.onZ('back') },
        { id: 'lock', kind: 'toggle', icon: <Lock size={15} />, label: 'Verrouiller', active: !!selected?.config.locked, onClick: a.onLock },
      ] },
      { id: 'style', label: t('data_grp_style', { defaultValue: 'Style' }), items: [
        { id: 'fmt', kind: 'button', size: 'large', icon: <Palette size={20} />, label: t('data_format', { defaultValue: 'Mise en forme' }), onClick: () => openPane('format') },
      ] },
    ],
  }

  return [home, insert, model, view, format]
}
