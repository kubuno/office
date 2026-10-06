// Word's content control around a table of contents: click inside the table and
// a frame appears, with a small toolbar above it — a grip, the built-in gallery,
// and « Mettre à jour la table… ».
//
// The frame is a screen marker, never printed: it is drawn as an overlay above
// the page canvas, exactly like the header/footer boundaries.
import { useRef, useState } from 'react'
import { AnchoredPopover } from '@ui'
import { FileText, GripVertical } from 'lucide-react'

import { TocGallery, type TocPreset } from './TocGallery'

export interface TocControlRect {
  left: number
  top: number
  width: number
  height: number
}

export interface TocControlProps {
  rect: TocControlRect
  onPreset: (preset: TocPreset) => void
  onRemove: () => void
  onUpdate: () => void
  t: (k: string, o?: Record<string, unknown>) => string
}

const BAR_H = 24

export function TocControl({ rect, onPreset, onRemove, onUpdate, t }: TocControlProps) {
  const [open, setOpen] = useState(false)
  const anchor = useRef<HTMLButtonElement>(null)
  const label = (k: string, d: string) => t(k, { defaultValue: d })

  return (
    <>
      {/* Cadre du contrôle de contenu — repère d'écran, jamais imprimé. */}
      <div style={{
        position: 'absolute', left: rect.left - 4, top: rect.top - 2,
        width: rect.width + 8, height: rect.height + 4,
        border: '1px solid #c4c7c5', borderRadius: 2,
        pointerEvents: 'none', zIndex: 22,
      }} />

      {/* Barre du contrôle, posée AU-DESSUS du cadre (comme Word). */}
      <div
        onMouseDown={e => e.preventDefault()}
        style={{
          position: 'absolute', left: rect.left - 4, top: rect.top - 2 - BAR_H,
          height: BAR_H, display: 'flex', alignItems: 'stretch',
          background: '#f1f3f4', border: '1px solid #c4c7c5', borderBottom: 'none',
          borderRadius: '2px 2px 0 0', zIndex: 23, overflow: 'hidden',
        }}>
        <span title={label('doc_toc_control', 'Table des matières')}
          style={{ display: 'flex', alignItems: 'center', padding: '0 1px', color: '#5f6368', cursor: 'grab' }}>
          <GripVertical size={13} />
        </span>
        <button ref={anchor} type="button" onClick={() => setOpen(v => !v)}
          title={label('doc_toc', 'Table des matières')}
          style={{
            display: 'flex', alignItems: 'center', gap: 2, padding: '0 4px',
            borderLeft: '1px solid #c4c7c5', borderRight: '1px solid #c4c7c5',
            background: open ? '#e3e6e8' : 'transparent', color: '#3c4043', cursor: 'pointer',
          }}>
          <FileText size={13} />
          <span style={{ fontSize: 9, lineHeight: 1 }}>▾</span>
        </button>
        <button type="button" onClick={onUpdate}
          style={{
            display: 'flex', alignItems: 'center', gap: 4, padding: '0 8px',
            background: 'transparent', color: '#3c4043', cursor: 'pointer', whiteSpace: 'nowrap',
          }}>
          <span style={{ position: 'relative', display: 'inline-flex' }}>
            <FileText size={13} />
            <span style={{ position: 'absolute', right: -3, bottom: -2, color: '#d93025', fontSize: 10, fontWeight: 700, lineHeight: 1 }}>!</span>
          </span>
          {label('doc_toc_update_dots', 'Mettre à jour la table…')}
        </button>
      </div>

      <AnchoredPopover anchorRef={anchor} open={open} onClose={() => setOpen(false)}>
        {/* Rendu dans un PORTAIL (hors de l'arbre du module) : fond opaque explicite,
            comme les autres popovers de cette page. Un `bg-surface` n'y résout rien
            et le menu laisse voir le document au travers. */}
        {/* Chrome du menu (gris très clair), pas blanc : le blanc est réservé à la
            surface des vignettes, comme dans Word. */}
        <div className="border border-border rounded-lg shadow-lg py-1" style={{ background: '#f8f9fa' }} data-module="office">
          <TocGallery t={t} close={() => setOpen(false)} hasToc
            onPreset={onPreset} onRemove={onRemove} />
        </div>
      </AnchoredPopover>
    </>
  )
}

export default TocControl
