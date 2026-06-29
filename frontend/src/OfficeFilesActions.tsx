import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { FileText } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useOfficeStore } from './store'

const ITEM_CLASS =
  'flex items-center gap-3 w-full px-3 py-2 text-sm text-text-primary ' +
  'hover:bg-surface-1 cursor-pointer outline-none'

/**
 * Contribue un menu "Nouveau Document" dans le bouton Nouveau du module files.
 * Enregistré dans le slot 'files-new-actions' — rendu uniquement si le module
 * office est actif (filtrage par moduleId dans le composant Slot).
 */
export default function OfficeFilesActions() {
  const navigate  = useNavigate()
  const createDoc = useOfficeStore(s => s.createDoc)
  const { t }     = useTranslation('office')

  const handleNew = async () => {
    const doc = await createDoc()
    navigate(`/office/documents/${doc.id}`)
  }

  return (
    <DropdownMenu.Item onSelect={handleNew} className={ITEM_CLASS}>
      <FileText size={16} className="text-blue-500" />
      {t('shell_new_document')}
    </DropdownMenu.Item>
  )
}
