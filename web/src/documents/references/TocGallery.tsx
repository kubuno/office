// Word's built-in table-of-contents gallery. Shown in two places — the ribbon
// button and the content control that appears when the caret enters a table —
// so it lives here rather than in either of them.
import { useState, type ReactNode } from 'react'
import { FileText, Quote } from 'lucide-react'

export type TocPreset = 'auto1' | 'auto2' | 'manual'

export interface TocGalleryProps {
  onPreset: (preset: TocPreset) => void
  onRemove: () => void
  /** Absent in the in-page control: Word only offers it from the ribbon. */
  onCustom?: () => void
  hasToc: boolean
  close: () => void
  t: (k: string, o?: Record<string, unknown>) => string
}

interface Row { label: string; indent: number; page: string; bold?: boolean }

const AUTO_ROWS: Row[] = [
  { label: 'Titre 1', indent: 0, page: '1' },
  { label: 'Titre 2', indent: 1, page: '1' },
  { label: 'Titre 3', indent: 2, page: '1' },
]
const MANUAL_ROWS: Row[] = [
  { label: 'Tapez le titre du chapitre (niveau 1)', indent: 0, page: '1', bold: true },
  { label: 'Tapez le titre du chapitre (niveau 2)', indent: 1, page: '2' },
  { label: 'Tapez le titre du chapitre (niveau 3)', indent: 2, page: '3' },
  { label: 'Tapez le titre du chapitre (niveau 1)', indent: 0, page: '4', bold: true },
]

function GalleryEntry({ title, rows, onPick, t }: {
  title: string; rows: Row[]; onPick: () => void
  t: (k: string, o?: Record<string, unknown>) => string
}): ReactNode {
  // Colours and the hover state are INLINE, not utility classes: the gallery is
  // rendered inside a menu (a portal, outside the module subtree) where the
  // module's tokens resolve to nothing and its `group-hover:` variants — emitted
  // in the `kubuno-module` cascade layer — do not apply. Inline styles are the
  // only thing guaranteed to hold in both hosts (ribbon menu and page control).
  const [hover, setHover] = useState(false)
  return (
    <button type="button" onMouseDown={e => e.preventDefault()} onClick={onPick}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{ display: 'block', width: '100%', textAlign: 'left', padding: '6px 8px', cursor: 'pointer', background: '#fff' }}>
      <div style={{ marginBottom: 4, paddingLeft: 4, fontWeight: 600, color: '#202124' }}>{title}</div>
      <div style={{
        padding: '6px 8px', borderRadius: 2,
        border: `1px solid ${hover ? '#1a73e8' : 'transparent'}`,
        background: hover ? '#e8f0fe' : '#fff',
      }}>
        <div style={{ color: '#1a73e8', marginBottom: 2 }}>{t('doc_toc', { defaultValue: 'Table des matières' })}</div>
        {rows.map((r, i) => (
          <div key={i} className="flex items-baseline whitespace-nowrap leading-5"
            style={{ paddingLeft: r.indent * 14, fontWeight: r.bold ? 600 : 400, color: '#202124' }}>
            <span className="shrink-0 truncate max-w-[62%]">{r.label}</span>
            <span className="flex-1 min-w-0 overflow-hidden px-1" style={{ color: '#9aa0a6' }}>{'.'.repeat(60)}</span>
            <span className="shrink-0">{r.page}</span>
          </div>
        ))}
      </div>
    </button>
  )
}

/** Command line at the bottom of the gallery (same inline-style reason). */
function CommandRow({ icon, label, onClick, disabled }: {
  icon: ReactNode; label: string; onClick: () => void; disabled?: boolean
}) {
  const [hover, setHover] = useState(false)
  return (
    <button type="button" disabled={disabled}
      onMouseDown={e => e.preventDefault()} onClick={onClick}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        display: 'flex', alignItems: 'center', gap: 8, padding: '6px 12px', width: '100%',
        textAlign: 'left', color: '#202124', cursor: disabled ? 'default' : 'pointer',
        background: hover && !disabled ? '#f1f3f4' : 'transparent', opacity: disabled ? 0.4 : 1,
      }}>
      {icon} {label}
    </button>
  )
}

export function TocGallery({ onPreset, onRemove, onCustom, hasToc, close, t }: TocGalleryProps) {
  const label = (k: string, d: string) => t(k, { defaultValue: d })
  return (
    <div className="w-[340px] max-h-[min(60vh,520px)] overflow-auto">
      <div className="px-3 py-1 bg-[#f1f3f4] text-[#5f6368] font-medium">
        {label('doc_toc_builtin', 'Prédéfini')}
      </div>
      {/* Surface BLANCHE de la galerie : elle couvre les vignettes, et ELLES SEULES.
          Les commandes du bas restent sur le fond du menu hôte, comme dans Word. */}
      <div style={{ background: '#fff' }}>
        <GalleryEntry t={t} title={label('doc_toc_auto1', 'Table automatique 1')} rows={AUTO_ROWS}
          onPick={() => { close(); onPreset('auto1') }} />
        <GalleryEntry t={t} title={label('doc_toc_auto2', 'Table automatique 2')} rows={AUTO_ROWS}
          onPick={() => { close(); onPreset('auto2') }} />
        <GalleryEntry t={t} title={label('doc_toc_manual', 'Table des matières manuelle')} rows={MANUAL_ROWS}
          onPick={() => { close(); onPreset('manual') }} />
      </div>
      <div className="mt-1 pt-1 border-t border-[#dadce0] flex flex-col">
        {onCustom && (
          <CommandRow icon={<FileText size={14} />} label={label('doc_toc_custom', 'Table des matières personnalisée…')}
            onClick={() => { close(); onCustom() }} />
        )}
        <CommandRow icon={<Quote size={14} />} label={label('doc_toc_remove', 'Supprimer la table des matières')}
          disabled={!hasToc} onClick={() => { close(); onRemove() }} />
      </div>
    </div>
  )
}

export default TocGallery
