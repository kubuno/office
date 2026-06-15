import { useParams } from 'react-router-dom'
import OfficeFontsSettings from './OfficeFontsSettings'
import { DocumentEditorArea } from './DocumentEditorPage'
import DocumentsApp from './DocumentsApp'

// recent/starred/trashed/templates props kept for route compat — view handled by ModuleFileBrowser tabs
export default function OfficeApp({ settings }: {
  recent?: boolean; starred?: boolean; trashed?: boolean; templates?: boolean; settings?: boolean
}) {
  const { id } = useParams<{ id: string }>()

  if (settings) return <OfficeFontsSettings />

  if (id) {
    return (
      <div className="h-full overflow-hidden flex flex-col">
        <DocumentEditorArea key={id} docId={id} />
      </div>
    )
  }

  return <DocumentsApp />
}
