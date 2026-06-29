import { FileText, TableProperties, LayoutTemplate } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useOfficeStore } from './store'
import { spreadsheetsApi, presentationsApi } from './api'
import { ContextMenuItem, useContextMenu } from '@kubuno/sdk'
export default function OfficeContextNewActions() {
  const navigate  = useNavigate()
  const createDoc = useOfficeStore(s => s.createDoc)
  const { close } = useContextMenu()
  const { t }     = useTranslation('office')

  const handleNewDocument = async () => {
    close()
    // Create a new document; the title can be customised by the user once open
    const doc = await createDoc(undefined, t('shell_default_document_filename'))
    navigate(`/office/documents/${doc.id}`)
  }

  const handleNewSpreadsheet = async () => {
    close()
    const ss = await spreadsheetsApi.create({ title: t('shell_default_spreadsheet_title') })
    navigate(`/office/spreadsheets/${ss.id}`)
  }

  const handleNewPresentation = async () => {
    close()
    const pres = await presentationsApi.create({ title: t('shell_default_presentation_title') })
    navigate(`/office/presentations/${pres.id}`)
  }

  return (
    <>
      <ContextMenuItem
        icon={<FileText size={16} className="text-blue-500" />}
        label={t('shell_new_document')}
        onClick={handleNewDocument}
      />
      <ContextMenuItem
        icon={<TableProperties size={16} className="text-green-600" />}
        label={t('shell_new_spreadsheet')}
        onClick={handleNewSpreadsheet}
      />
      <ContextMenuItem
        icon={<LayoutTemplate size={16} className="text-orange-500" />}
        label={t('shell_new_presentation')}
        onClick={handleNewPresentation}
      />
    </>
  )
}
