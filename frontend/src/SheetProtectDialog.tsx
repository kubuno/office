import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button } from '@ui'
import { Lock, Unlock, Loader2 } from 'lucide-react'

interface Props {
  mode: 'protect' | 'unlock'
  /** protect: called with the new password. unlock: return true if the password matched. */
  onSubmit: (password: string) => Promise<boolean>
  onClose: () => void
}

export default function SheetProtectDialog({ mode, onSubmit, onClose }: Props) {
  const { t } = useTranslation('office')
  const [pwd, setPwd] = useState('')
  const [confirm, setConfirm] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const protect = mode === 'protect'
  const inputCls = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary w-full'

  const submit = async () => {
    setError('')
    if (!pwd) { setError(t('prot_empty', { defaultValue: 'Saisissez un mot de passe.' })); return }
    if (protect && pwd !== confirm) { setError(t('prot_mismatch', { defaultValue: 'Les mots de passe ne correspondent pas.' })); return }
    setBusy(true)
    try {
      const ok = await onSubmit(pwd)
      if (!ok && !protect) { setError(t('prot_wrong', { defaultValue: 'Mot de passe incorrect.' })); setBusy(false); return }
      onClose()
    } catch {
      setError(t('prot_error', { defaultValue: 'Une erreur est survenue.' }))
      setBusy(false)
    }
  }

  return (
    <FloatingWindow
      title={protect ? t('prot_title', { defaultValue: 'Protéger la feuille' }) : t('prot_unlock_title', { defaultValue: 'Ôter la protection de la feuille' })}
      icon={protect ? <Lock size={16} /> : <Unlock size={16} />}
      onClose={onClose} backdrop
      defaultWidth={400} defaultHeight={protect ? 300 : 230}
    >
      <div className="p-4 space-y-3 text-sm" data-module="office">
        <div>
          <div className="text-xs font-medium text-text-secondary mb-1">{t('prot_pwd', { defaultValue: 'Mot de passe' })}</div>
          <input type="password" autoFocus value={pwd} onChange={e => setPwd(e.target.value)}
            onKeyDown={e => { e.stopPropagation(); if (e.key === 'Enter' && !protect) submit() }}
            className={inputCls} />
        </div>
        {protect && (
          <div>
            <div className="text-xs font-medium text-text-secondary mb-1">{t('prot_confirm', { defaultValue: 'Confirmer le mot de passe' })}</div>
            <input type="password" value={confirm} onChange={e => setConfirm(e.target.value)}
              onKeyDown={e => { e.stopPropagation(); if (e.key === 'Enter') submit() }}
              className={inputCls} />
          </div>
        )}
        {protect && (
          <div className="text-[11px] text-warning bg-warning-light rounded px-2 py-1.5 leading-snug">
            {t('prot_warn', { defaultValue: 'Attention : un mot de passe perdu ne peut pas être récupéré. Notez-le en lieu sûr.' })}
          </div>
        )}
        {error && <div className="text-[12px] text-danger">{error}</div>}
        <div className="flex justify-end gap-2 pt-1">
          <Button variant="ghost" onClick={onClose} disabled={busy}>{t('prot_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" onClick={submit} disabled={busy}>
            {busy && <Loader2 size={14} className="mr-1 animate-spin" />}
            {protect ? t('prot_apply', { defaultValue: 'Protéger' }) : t('prot_remove', { defaultValue: 'Ôter la protection' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
