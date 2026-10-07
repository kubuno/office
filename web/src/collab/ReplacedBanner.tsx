import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { ReplacedReason } from './collabEpoch'

/**
 * Host side of a replaced collaboration room (see `collabEpoch.ts`): reloads the
 * editor once — after a short random delay, since every open tab of the file
 * learns it at the same moment — and keeps a notice for the user. `reload` must
 * remount the editor on freshly fetched content.
 */
export function useCollabReload(reload: () => void | Promise<void>): {
  onReplaced: (reason: ReplacedReason) => void
  notice: { lostEdits: boolean } | null
  dismiss: () => void
} {
  const [notice, setNotice] = useState<{ lostEdits: boolean } | null>(null)
  const pendingRef = useRef(false)
  const reloadRef = useRef(reload)
  reloadRef.current = reload

  const onReplaced = useCallback((reason: ReplacedReason) => {
    setNotice(prev => ({ lostEdits: (prev?.lostEdits ?? false) || reason === 'stale-save' }))
    if (pendingRef.current) return
    pendingRef.current = true
    window.setTimeout(() => {
      Promise.resolve(reloadRef.current()).finally(() => { pendingRef.current = false })
    }, 150 + Math.random() * 600)
  }, [])

  // A plain reload notice fades out; one about lost edits stays until dismissed.
  useEffect(() => {
    if (!notice || notice.lostEdits) return
    const timer = window.setTimeout(() => setNotice(null), 10000)
    return () => window.clearTimeout(timer)
  }, [notice])

  return { onReplaced, notice, dismiss: () => setNotice(null) }
}

/** Notice shown after the file was reloaded because it was replaced elsewhere. */
export function CollabReplacedBanner({ lostEdits, onClose }: { lostEdits: boolean; onClose: () => void }) {
  const { t } = useTranslation('office')
  return (
    <div
      role="status"
      className="fixed left-1/2 top-16 z-[1000] flex max-w-[min(640px,calc(100vw-32px))] -translate-x-1/2 items-center gap-3 rounded-md bg-[#323232] px-4 py-2 text-sm text-white shadow-lg"
    >
      <span>
        {t('collab_replaced_reloaded', { defaultValue: 'Ce fichier a été modifié depuis une autre application : la version enregistrée a été rechargée.' })}
        {lostEdits && (
          <> {t('collab_replaced_lost_edits', { defaultValue: "Vos dernières modifications n'ont pas pu y être enregistrées." })}</>
        )}
      </span>
      <button type="button" className="shrink-0 font-medium text-[#8ab4f8] hover:underline" onClick={onClose}>
        {t('common_ok', { defaultValue: 'OK' })}
      </button>
    </div>
  )
}
