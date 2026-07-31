import { useState } from 'react'
import { Checkbox } from '@ui'

/**
 * Office's own part of the core share dialog's settings screen.
 *
 * Registered for `moduleId: 'office', kind: 'document'`, so it only shows when
 * a document is being shared — a spreadsheet or a presentation would register
 * its own.
 */
export default function DocumentShareSettings() {
  const [editorsMayShare, setEditorsMayShare] = useState(true)
  const [editorsMayCopy,  setEditorsMayCopy]  = useState(true)
  const [othersMayCopy,   setOthersMayCopy]   = useState(true)

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
      <div>
        <div className="text-xs text-text-secondary mb-2">Accès</div>
        <Checkbox
          checked={editorsMayShare}
          onChange={setEditorsMayShare}
          label="Autoriser les éditeurs à modifier les autorisations et à partager"
        />
      </div>

      <div>
        <div className="text-xs text-text-secondary mb-2">
          Personnes autorisées à télécharger, copier et imprimer
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <Checkbox checked={editorsMayCopy} onChange={setEditorsMayCopy} label="Éditeurs" />
          <Checkbox checked={othersMayCopy} onChange={setOthersMayCopy} label="Commentateurs et lecteurs" />
        </div>
      </div>
    </div>
  )
}
