/**
 * Point d'entrée du bundle MODULE office (suite bureautique), chargé à
 * l'exécution. Buildé séparément via `vite.module.config.ts` : les specifiers
 * partagés (`@kubuno/sdk`, `@kubuno/drive`, `@ui`, react…) sont externes et
 * résolus au runtime par l'import map du host ; TipTap / Monaco / KaTeX /
 * perfect-freehand + yjs (et la glue collab locale `./collab`, qui partage le
 * yjs bundlé du module) restent bundlés en chunks lazy par sous-éditeur. Le host
 * importe ce fichier puis appelle `register()` ; `sdkVersion` rejette une
 * incompatibilité de contrat.
 */
import { lazy } from 'react'
import {
  RouteRegistry,
  CollapseSidebarRegistry,
  SlotRegistry,
  WidgetRegistry,
  ModuleServiceRegistry,
  ModuleSettingsRegistry,
  WaffleAppRegistry,
  FaviconRegistry,
  FileTypeRegistry,
  useSidebarStore,
  useToolbarStore,
  SDK_VERSION,
} from '@kubuno/sdk'
import { FileText, TableProperties, LayoutTemplate, FolderKanban, Network, BarChart3, Zap, StickyNote, Sigma } from 'lucide-react'
import './index.css'
import './i18n'
import { officeApi, officeInitApi } from './api'
import OfficeLogo from './OfficeLogo'
import OfficeNewActions from './OfficeNewActions'
import OfficeSidebarBody from './OfficeSidebarBody'
import OfficeFilesActions from './OfficeFilesActions'
import OfficeContextNewActions from './OfficeContextNewActions'
import OfficeRecentWidget from './OfficeRecentWidget'

export const sdkVersion = SDK_VERSION

export function register() {
  // Office apps collapse the core sidebar on open for maximum workspace width.
  CollapseSidebarRegistry.add('/office')

  // Favicon de l'onglet quand on est dans Office (sinon favicon Kubuno).
  FaviconRegistry.register('office', '/office-logo.svg')

  const ENSURE_KEY = 'kubuno:office:folders-ensured'
  if (!sessionStorage.getItem(ENSURE_KEY)) {
    officeInitApi.ensureFolders()
      .then(() => sessionStorage.setItem(ENSURE_KEY, '1'))
      .catch(() => { /* best-effort */ })
  }

  WaffleAppRegistry.register('office', 'Office', [
    { id: 'office',               label: 'Office',                               Icon: OfficeLogo,      path: '/office' },
    { id: 'office-documents',     label: 'Documents',       Icon: FileText,        path: '/office/documents' },
    { id: 'office-spreadsheets',  label: 'Spreadsheets',    Icon: TableProperties, path: '/office/spreadsheets' },
    { id: 'office-presentations', label: 'Presentations',   Icon: LayoutTemplate,  path: '/office/presentations' },
    { id: 'office-projects',      label: 'Projects',        Icon: FolderKanban,    path: '/office/projects' },
    { id: 'office-diagrams',      label: 'Diagrams',        Icon: Network,         path: '/office/diagrams' },
    { id: 'office-data',          label: 'Data',      Icon: BarChart3,       path: '/office/data' },
    { id: 'office-script',        label: 'Script',    Icon: Zap,             path: '/office/script' },
    { id: 'office-maths',         label: 'Maths', Icon: Sigma, path: '/office/maths' },
    { id: 'office-whiteboard',    label: 'Whiteboard',Icon: StickyNote,      path: '/office/whiteboard' },
  ])

  // The header gear button opens the per-user Office settings while in /office.
  ModuleSettingsRegistry.register('office')

  WidgetRegistry.register({ id: 'office-recent', moduleId: 'office', Component: OfficeRecentWidget, size: 'small', order: 50 })

  // Types de fichiers pris en charge par chaque sous-module Office (déclarés auprès
  // de `files` — base du filtrage StartPage / « ouvrir avec »).
  FileTypeRegistry.register({
    moduleId: 'office-documents', label: 'Documents', icon: 'FileText',
    mimeTypes: ['application/vnd.kubuno.document+json',
      'text/plain', 'text/markdown', 'text/html', 'application/rtf', 'application/msword',
      'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      'application/vnd.oasis.opendocument.text', 'application/vnd.oasis.opendocument.text-template'],
    extensions: ['kbdoc', 'doc', 'docx', 'odt', 'ott', 'dotx', 'rtf', 'txt', 'md', 'html'],
    open: (f, nav) => { import('./api').then(({ officeApi }) => officeApi.openByFile(f.id).then(doc => nav(`/office/${doc.id}`)).catch(() => {})) },
  })
  FileTypeRegistry.register({
    moduleId: 'office-spreadsheets', label: 'Tableur', icon: 'TableProperties',
    mimeTypes: ['application/vnd.kubuno.spreadsheet+json',
      'text/csv', 'application/vnd.ms-excel',
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'application/vnd.oasis.opendocument.spreadsheet'],
    extensions: ['kbcal', 'csv', 'tsv', 'xls', 'xlsx', 'ods', 'ots'],
    open: (f, nav) => { import('./api').then(({ spreadsheetsApi }) => spreadsheetsApi.openByFile(f.id).then(s => nav(`/office/spreadsheets/${s.id}`)).catch(() => {})) },
  })
  FileTypeRegistry.register({
    moduleId: 'office-presentations', label: 'Présentations', icon: 'LayoutTemplate',
    mimeTypes: ['application/vnd.kubuno.presentation+json',
      'application/vnd.ms-powerpoint',
      'application/vnd.openxmlformats-officedocument.presentationml.presentation',
      'application/vnd.oasis.opendocument.presentation'],
    extensions: ['kbsld', 'ppt', 'pptx', 'odp', 'otp'],
    open: (f, nav) => { import('./api').then(({ presentationsApi }) => presentationsApi.openByFile(f.id).then(p => nav(`/office/presentations/${p.id}`)).catch(() => {})) },
  })
  FileTypeRegistry.register({
    moduleId: 'office-diagrams', label: 'Diagrammes', icon: 'Network',
    mimeTypes: ['application/vnd.kubuno.diagram+json', 'image/svg+xml', 'application/xml'],
    extensions: ['kbdia', 'svg', 'drawio', 'vsdx'],
    open: (f, nav) => { import('./api').then(({ diagramsApi }) => diagramsApi.openByFile(f.id).then(d => nav(`/office/diagrams/${d.id}`)).catch(() => {})) },
  })
  FileTypeRegistry.register({
    moduleId: 'office-projects', label: 'Projets', icon: 'FolderKanban',
    mimeTypes: ['application/vnd.kubuno.project+json', 'application/json'],
    extensions: ['kbprj', 'json'],
    open: (f, nav) => { import('./api').then(({ projectsApi }) => projectsApi.openByFile(f.id).then(p => nav(`/office/projects/${p.id}`)).catch(() => {})) },
  })
  FileTypeRegistry.register({
    moduleId: 'office-data', label: 'Data', icon: 'BarChart3',
    mimeTypes: ['application/vnd.kubuno.report+json', 'application/vnd.kubuno.dataset+json',
      'text/csv', 'application/json',
      'application/vnd.oasis.opendocument.spreadsheet',
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'],
    extensions: ['kbdrp', 'kbdst', 'csv', 'tsv', 'json', 'ods', 'xlsx', 'parquet'],
    // .kbdrp → open the report by file (gets a real id in the URL). Datasets
    // (.kbdst) live inside reports, so route to the Data workspace.
    open: (f, nav) => {
      const name = (f.name ?? '').toLowerCase()
      if (name.endsWith('.kbdst')) { nav('/office/data'); return }
      import('./data-api').then(({ reportsApi }) =>
        reportsApi.openByFile(f.id).then(d => nav(`/office/data/${d.report.id}`)).catch(() => nav('/office/data')))
    },
  })
  FileTypeRegistry.register({
    moduleId: 'office-script', label: 'Script', icon: 'Zap',
    mimeTypes: ['application/vnd.kubuno.script+json', 'text/javascript', 'application/json'],
    extensions: ['kbscr', 'js', 'mjs', 'json'],
    open: (f, nav) => { import('./script-api').then(({ scriptsApi }) => scriptsApi.openByFile(f.id).then(({ script }) => nav(`/office/script/${script.id}`)).catch(() => {})) },
  })
  FileTypeRegistry.register({
    moduleId: 'office-maths', label: 'Maths', icon: 'Sigma',
    mimeTypes: ['application/vnd.kubuno.maths+json', 'text/x-tex', 'application/x-tex'],
    extensions: ['kbmath', 'tex', 'latex'],
    open: (f, nav) => { import('./maths-api').then(({ formulasApi }) => formulasApi.openByFile(f.id).then(({ formula }) => nav(`/office/maths/${formula.id}`)).catch(() => {})) },
  })
  FileTypeRegistry.register({
    moduleId: 'office-whiteboard', label: 'Whiteboard', icon: 'StickyNote',
    mimeTypes: ['application/vnd.kubuno.whiteboard'],
    extensions: ['kbwbd'],
    open: (f, nav) => { import('./whiteboard-api').then(({ boardsApi }) => boardsApi.openByFile(f.id).then(({ board }) => nav(`/office/whiteboard/${board.id}`)).catch(() => {})) },
  })

  // Contributions vers le module files
  SlotRegistry.register('files-new-actions',         'office', OfficeFilesActions)
  SlotRegistry.register('files-context-new-actions', 'office', OfficeContextNewActions)

  useSidebarStore.getState().register({
    moduleId:          'office',
    routePrefix:       '/office',
    newButtonLabelKey: 'office:common_create',
    NewActions:        OfficeNewActions,
    SidebarBody:       OfficeSidebarBody,
    collapsedBody:     true,
  })

  useToolbarStore.getState().register({
    moduleId:    'office',
    routePrefix: '/office',
    noPadding:   true,
  })

  useToolbarStore.getState().register({
    moduleId:    'office-settings',
    routePrefix: '/office/settings',
  })

  // Routes
  const OfficeSuiteHome        = lazy(() => import('./OfficeSuiteHome'))
  const OfficeApp              = lazy(() => import('./OfficeApp'))
  const OfficeSettingsPage     = lazy(() => import('./OfficeSettingsPage'))
  const SpreadsheetApp         = lazy(() => import('./SpreadsheetApp'))
  const PresentationApp        = lazy(() => import('./PresentationApp'))
  const PresentationEditorPage = lazy(() => import('./PresentationEditorPage'))
  const ProjectsApp            = lazy(() => import('./ProjectsApp'))
  const ProjectEditorPage      = lazy(() => import('./ProjectEditorPage'))
  const DiagramsApp            = lazy(() => import('./DiagramsApp'))
  const DiagramEditorPage      = lazy(() => import('./DiagramEditorPage'))
  const DataApp                = lazy(() => import('./DataApp'))
  const ScriptApp              = lazy(() => import('./ScriptApp'))
  const MathsApp               = lazy(() => import('./MathsApp'))
  const WhiteboardApp          = lazy(() => import('./WhiteboardApp'))

  RouteRegistry.register('office',                          OfficeSuiteHome)
  RouteRegistry.register('office/documents',                OfficeApp)
  RouteRegistry.register('office/recent',                   OfficeApp, { recent:    true })
  RouteRegistry.register('office/starred',                  OfficeApp, { starred:   true })
  RouteRegistry.register('office/trash',                    OfficeApp, { trashed:   true })
  RouteRegistry.register('office/templates',                OfficeApp, { templates: true })
  RouteRegistry.register('office/settings',                 OfficeSettingsPage)
  RouteRegistry.register('office/:id',                      OfficeApp)
  RouteRegistry.register('office/spreadsheets',             SpreadsheetApp)
  RouteRegistry.register('office/spreadsheets/recent',      SpreadsheetApp, { recent:  true })
  RouteRegistry.register('office/spreadsheets/starred',     SpreadsheetApp, { starred: true })
  RouteRegistry.register('office/spreadsheets/trash',       SpreadsheetApp, { trashed: true })
  RouteRegistry.register('office/spreadsheets/:id',         SpreadsheetApp)
  RouteRegistry.register('office/presentations',            PresentationApp)
  RouteRegistry.register('office/presentations/recent',     PresentationApp, { recent:  true })
  RouteRegistry.register('office/presentations/starred',    PresentationApp, { starred: true })
  RouteRegistry.register('office/presentations/trash',      PresentationApp, { trashed: true })
  RouteRegistry.register('office/presentations/:id',        PresentationEditorPage)
  RouteRegistry.register('office/projects',                 ProjectsApp)
  RouteRegistry.register('office/projects/recent',          ProjectsApp, { recent:  true })
  RouteRegistry.register('office/projects/starred',         ProjectsApp, { starred: true })
  RouteRegistry.register('office/projects/trash',           ProjectsApp, { trashed: true })
  RouteRegistry.register('office/projects/:id',             ProjectEditorPage)
  RouteRegistry.register('office/diagrams',                 DiagramsApp)
  RouteRegistry.register('office/diagrams/recent',          DiagramsApp, { recent:  true })
  RouteRegistry.register('office/diagrams/starred',         DiagramsApp, { starred: true })
  RouteRegistry.register('office/diagrams/trash',           DiagramsApp, { trashed: true })
  RouteRegistry.register('office/diagrams/:id',             DiagramEditorPage)
  RouteRegistry.register('office/data',                     DataApp)
  RouteRegistry.register('office/data/:id',                 DataApp)
  RouteRegistry.register('office/script',                   ScriptApp)
  RouteRegistry.register('office/script/:id',               ScriptApp)
  RouteRegistry.register('office/maths',                    MathsApp)
  RouteRegistry.register('office/maths/:id',                MathsApp)
  RouteRegistry.register('office/whiteboard',               WhiteboardApp)
  RouteRegistry.register('office/whiteboard/:id',           WhiteboardApp)

  // API publique consommable par d'autres modules via ModuleServiceRegistry
  ModuleServiceRegistry.publish('office', {
    list:    (opts?: object) => officeApi.list(opts ?? {}),
    update:  (id: string, data: object) => officeApi.update(id, data),
    trash:   (id: string) => officeApi.trash(id),
    restore: (id: string) => officeApi.restore(id),
    delete:  (id: string) => officeApi.delete(id),
  })
}
