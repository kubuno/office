import { useTranslation } from 'react-i18next'
import CollaboratorsDialog from './CollaboratorsDialog'
import { officeApi } from './api'

// Partage d'un document : fine surcouche du dialog générique de collaborateurs.
export default function DocumentShareDialog({ docId, onClose }: { docId: string; onClose: () => void }) {
  const { t } = useTranslation('office')
  return (
    <CollaboratorsDialog
      entityId={docId}
      cacheKey="doc-collab"
      title={t('share_title', 'Partager le document')}
      onClose={onClose}
      api={officeApi}
    />
  )
}
