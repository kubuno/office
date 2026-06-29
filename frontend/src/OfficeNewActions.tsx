import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { FileText, Layout, TableProperties, LayoutTemplate, FolderKanban, Network, Sigma } from 'lucide-react'
import { useNavigate, useLocation } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useOfficeStore } from './store'
import { spreadsheetsApi, presentationsApi, projectsApi, diagramsApi } from './api'
import { formulasApi } from './maths-api'

const ITEM_CLASS =
  'flex items-center gap-3 w-full px-3 py-2 text-sm text-text-primary ' +
  'hover:bg-surface-1 cursor-pointer outline-none'

export default function OfficeNewActions() {
  const navigate     = useNavigate()
  const { pathname } = useLocation()
  const qc           = useQueryClient()
  const createDoc    = useOfficeStore(s => s.createDoc)
  const { t }        = useTranslation('office')

  const createSsMut = useMutation({
    mutationFn: () => spreadsheetsApi.create({ title: t('common_untitled') }),
    onSuccess: (ss) => {
      qc.invalidateQueries({ queryKey: ['spreadsheets'] })
      navigate(`/office/spreadsheets/${ss.id}`)
    },
  })

  const createPresMut = useMutation({
    mutationFn: () => presentationsApi.create({ title: t('shell_default_presentation_title') }),
    onSuccess: (pres) => {
      qc.invalidateQueries({ queryKey: ['presentations'] })
      navigate(`/office/presentations/${pres.id}`)
    },
  })

  const createProjectMut = useMutation({
    mutationFn: () => projectsApi.create({ title: t('shell_default_project_title') }),
    onSuccess: (p) => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      navigate(`/office/projects/${p.id}`)
    },
  })

  const createDiagramMut = useMutation({
    mutationFn: () => diagramsApi.create({ title: t('shell_default_diagram_title') }),
    onSuccess: (d) => {
      qc.invalidateQueries({ queryKey: ['diagrams'] })
      navigate(`/office/diagrams/${d.id}`)
    },
  })

  const createFormulaMut = useMutation({
    mutationFn: () => formulasApi.create({ name: t('math_new', { defaultValue: 'Nouvelle formule' }) }),
    onSuccess: ({ formula }) => navigate(`/office/maths/${formula.id}`),
  })

  if (!pathname.startsWith('/office')) return null

  const handleNew = async (templateId?: string) => {
    const doc = await createDoc(templateId)
    navigate(`/office/documents/${doc.id}`)
  }

  return (
    <>
      <DropdownMenu.Item onSelect={() => handleNew()} className={ITEM_CLASS}>
        <FileText size={16} className="text-text-secondary" />
        {t('shell_new_document')}
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={() => createSsMut.mutate()} className={ITEM_CLASS}>
        <TableProperties size={16} className="text-green-600" />
        {t('shell_new_spreadsheet')}
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={() => createPresMut.mutate()} className={ITEM_CLASS}>
        <LayoutTemplate size={16} className="text-orange-500" />
        {t('shell_new_presentation')}
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={() => createProjectMut.mutate()} className={ITEM_CLASS}>
        <FolderKanban size={16} className="text-blue-600" />
        {t('shell_new_project')}
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={() => createDiagramMut.mutate()} className={ITEM_CLASS}>
        <Network size={16} className="text-indigo-600" />
        {t('shell_new_diagram')}
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={() => createFormulaMut.mutate()} className={ITEM_CLASS}>
        <Sigma size={16} className="text-[#0b66c3]" />
        {t('shell_new_formula', { defaultValue: 'Nouvelle formule' })}
      </DropdownMenu.Item>
      <DropdownMenu.Item onSelect={() => navigate('/office/templates')} className={ITEM_CLASS}>
        <Layout size={16} className="text-text-secondary" />
        {t('shell_from_template')}
      </DropdownMenu.Item>
    </>
  )
}
