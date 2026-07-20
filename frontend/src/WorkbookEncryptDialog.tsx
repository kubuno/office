import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button } from '@ui'
import { ShieldCheck, ShieldOff, KeyRound, Loader2 } from 'lucide-react'

interface Props {
  mode: 'encrypt' | 'decrypt' | 'unlock'
  /** encrypt: new password. decrypt/unlock: existing password → false if wrong. */
  onSubmit: (password: string) => Promise<boolean>
  onClose: () => void
  /** unlock gate can't be dismissed to a usable sheet, but Cancel still returns to the list. */
  onCancel?: () => void
}

export default function WorkbookEncryptDialog({ mode, onSubmit, onClose, onCancel }: Props) {
  const { t } = useTranslation('office')
  const [pwd, setPwd] = useState('')
  const [confirm, setConfirm] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const encrypt = mode === 'encrypt'
  const inputCls = 'h-8 px-2 border border-border rounded bg-surface-0 text-sm outline-none focus:border-primary w-full'

  const title = encrypt
    ? t('enc_title', { defaultValue: 'Chiffrer le classeur avec un mot de passe' })
    : mode === 'decrypt'
      ? t('enc_dec_title', { defaultValue: 'Déchiffrer le classeur' })
      : t('enc_unlock_title', { defaultValue: 'Classeur chiffré — mot de passe requis' })
  const icon = encrypt ? <ShieldCheck size={16} /> : mode === 'decrypt' ? <ShieldOff size={16} /> : <KeyRound size={16} />

  const submit = async () => {
    setError('')
    if (!pwd) { setError(t('enc_empty', { defaultValue: 'Saisissez un mot de passe.' })); return }
    if (encrypt && pwd !== confirm) { setError(t('enc_mismatch', { defaultValue: 'Les mots de passe ne correspondent pas.' })); return }
    setBusy(true)
    try {
      const ok = await onSubmit(pwd)
      if (!ok) { setError(t('enc_wrong', { defaultValue: 'Mot de passe incorrect.' })); setBusy(false); return }
      onClose()
    } catch {
      setError(t('enc_error', { defaultValue: 'Une erreur est survenue.' }))
      setBusy(false)
    }
  }

  return (
    <FloatingWindow title={title} icon={icon} onClose={onCancel ?? onClose} backdrop
      defaultWidth={440} defaultHeight={encrypt ? 340 : 250}>
      <div className="p-4 space-y-3 text-sm" data-module="office">
        {mode === 'unlock' && (
          <div className="text-[12px] text-text-secondary leading-snug">
            {t('enc_unlock_desc', { defaultValue: 'Ce classeur est chiffré (AES-256). Entrez le mot de passe pour en déchiffrer le contenu dans cet onglet.' })}
          </div>
        )}
        <div>
          <div className="text-xs font-medium text-text-secondary mb-1">{t('enc_pwd', { defaultValue: 'Mot de passe' })}</div>
          <input type="password" autoFocus value={pwd} onChange={e => setPwd(e.target.value)}
            onKeyDown={e => { e.stopPropagation(); if (e.key === 'Enter' && !encrypt) submit() }} className={inputCls} />
        </div>
        {encrypt && (
          <div>
            <div className="text-xs font-medium text-text-secondary mb-1">{t('enc_confirm', { defaultValue: 'Confirmer le mot de passe' })}</div>
            <input type="password" value={confirm} onChange={e => setConfirm(e.target.value)}
              onKeyDown={e => { e.stopPropagation(); if (e.key === 'Enter') submit() }} className={inputCls} />
          </div>
        )}
        {encrypt && (
          <div className="text-[11px] text-warning bg-warning-light rounded px-2 py-1.5 leading-snug">
            {t('enc_warn', { defaultValue: 'Le contenu sera réellement chiffré : un mot de passe perdu rend le classeur irrécupérable. La collaboration en temps réel est désactivée sur un classeur chiffré.' })}
          </div>
        )}
        {error && <div className="text-[12px] text-danger">{error}</div>}
        <div className="flex justify-end gap-2 pt-1">
          <Button variant="ghost" onClick={onCancel ?? onClose} disabled={busy}>{t('enc_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button variant="primary" onClick={submit} disabled={busy}>
            {busy && <Loader2 size={14} className="mr-1 animate-spin" />}
            {encrypt ? t('enc_do', { defaultValue: 'Chiffrer' }) : mode === 'decrypt' ? t('enc_dec_do', { defaultValue: 'Déchiffrer' }) : t('enc_unlock_do', { defaultValue: 'Déverrouiller' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
