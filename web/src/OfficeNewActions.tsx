/**
 * Items of the sidebar "New" button for Office — DATA for the project's menu
 * component (`MenuDropdown` from @ui), contributed through the generic
 * 'shell.new-actions' extension point (see entry.ts). The functions are
 * evaluated when the menu opens, so labels are always fresh, without hooks.
 */
import type { MenuItem } from '@ui'
import { FileText, Layout, TableProperties, LayoutTemplate, FolderKanban, Network, Sigma } from 'lucide-react'
// `navigate` is the core's SPA navigation helper for code running outside
// React: the shell hands it the router's real `navigate`.
import { i18n, navigate } from '@kubuno/sdk'
import { useOfficeStore } from './store'
import { spreadsheetsApi, presentationsApi, projectsApi, diagramsApi } from './api'
import { formulasApi } from './maths-api'
import { invalidateQueries } from './queryClientBridge'

// Creation failures were already silent in the previous useMutation-based menu
// (no onError handler); keep that behaviour without unhandled rejections.
const run = (fn: () => Promise<void>) => () => { fn().catch(() => {}) }

const newDocument = async () => {
  const doc = await useOfficeStore.getState().createDoc()
  navigate(`/office/documents/${doc.id}`)
}

const newSpreadsheet = async () => {
  const ss = await spreadsheetsApi.create({ title: i18n.t('office:common_untitled') })
  invalidateQueries(['spreadsheets'])
  navigate(`/office/spreadsheets/${ss.id}`)
}

const newPresentation = async () => {
  const pres = await presentationsApi.create({ title: i18n.t('office:shell_default_presentation_title') })
  invalidateQueries(['presentations'])
  navigate(`/office/presentations/${pres.id}`)
}

const newProject = async () => {
  const p = await projectsApi.create({ title: i18n.t('office:shell_default_project_title') })
  invalidateQueries(['projects'])
  navigate(`/office/projects/${p.id}`)
}

const newDiagram = async () => {
  const d = await diagramsApi.create({ title: i18n.t('office:shell_default_diagram_title') })
  invalidateQueries(['diagrams'])
  navigate(`/office/diagrams/${d.id}`)
}

const newFormula = async () => {
  const { formula } = await formulasApi.create({ name: i18n.t('office:math_new', { defaultValue: 'Nouvelle formule' }) })
  navigate(`/office/maths/${formula.id}`)
}

export function officeNewActionItems(): MenuItem[] {
  if (!window.location.pathname.startsWith('/office')) return []

  return [
    {
      type: 'action',
      label: i18n.t('office:shell_new_document'),
      icon: <FileText size={16} className="text-text-secondary" />,
      onClick: run(newDocument),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_new_spreadsheet'),
      icon: <TableProperties size={16} className="text-green-600" />,
      onClick: run(newSpreadsheet),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_new_presentation'),
      icon: <LayoutTemplate size={16} className="text-orange-500" />,
      onClick: run(newPresentation),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_new_project'),
      icon: <FolderKanban size={16} className="text-blue-600" />,
      onClick: run(newProject),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_new_diagram'),
      icon: <Network size={16} className="text-indigo-600" />,
      onClick: run(newDiagram),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_new_formula', { defaultValue: 'Nouvelle formule' }),
      icon: <Sigma size={16} className="text-[#0b66c3]" />,
      onClick: run(newFormula),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_from_template'),
      icon: <Layout size={16} className="text-text-secondary" />,
      onClick: () => navigate('/office/templates'),
    },
  ]
}

/** Office's contribution to DRIVE's "New" menu ('drive.new-actions' extension
 *  point — data counterpart of the former 'files-new-actions' component slot).
 *  Drive uses it both for its sidebar "New" button and for the "New" submenu of
 *  its background context menu, which is why the three document kinds the former
 *  'files-context-new-actions' slot offered all live here.
 *  No cross-module import: the point name is a literal; drive reads it only
 *  from ACTIVE modules, so registering while drive is absent is harmless. */
export function officeDriveNewActionItems(): MenuItem[] {
  return [
    {
      type: 'action',
      label: i18n.t('office:shell_new_document'),
      icon: <FileText size={16} className="text-blue-500" />,
      onClick: run(newDocument),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_new_spreadsheet'),
      icon: <TableProperties size={16} className="text-green-600" />,
      onClick: run(newSpreadsheet),
    },
    {
      type: 'action',
      label: i18n.t('office:shell_new_presentation'),
      icon: <LayoutTemplate size={16} className="text-orange-500" />,
      onClick: run(newPresentation),
    },
  ]
}
