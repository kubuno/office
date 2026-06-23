import { useParams } from 'react-router-dom'
import OfficeFontsSettings from './OfficeFontsSettings'
import { DocumentEditorArea } from './DocumentEditorPage'
import { DocumentsHome } from './DocumentsBackstage'

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

  // Plus de StartPage séparée : l'accueil sans document ouvre la chrome éditeur avec
  // l'onglet « Fichier » (backstage) ouvert et verrouillé.
  return <DocumentsHome />
}
