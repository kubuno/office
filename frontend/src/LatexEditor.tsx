import { useEffect, useRef } from 'react'

// Éditeur de code LaTeX avec coloration syntaxique. Technique « overlay » : un
// <textarea> transparent (curseur + saisie) au-dessus d'un <pre> coloré, parfaitement
// alignés (même police/taille/marge) et défilement synchronisé.

const FONT = "13px 'Fira Code', 'DM Mono', ui-monospace, SFMono-Regular, Menlo, monospace"

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

// Coloration LaTeX : commandes \cmd, accolades/crochets, indices/exposants & opérateurs,
// nombres, commentaires %…, environnements \begin{…}/\end{…}.
function highlight(src: string): string {
  // On découpe en jetons via une regex globale, en échappant le HTML par jeton.
  const re = /(%[^\n]*)|(\\(?:begin|end)\b)|(\\[a-zA-Z]+)|(\\[^a-zA-Z])|([{}\[\]()])|([_^&])|([+\-*/=<>|])|(\d+(?:\.\d+)?)/g
  let out = ''
  let last = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(src)) !== null) {
    out += escapeHtml(src.slice(last, m.index))
    const tok = m[0]
    let cls = ''
    if (m[1]) cls = 'tk-com'        // commentaire
    else if (m[2]) cls = 'tk-env'   // \begin \end
    else if (m[3]) cls = 'tk-cmd'   // \commande
    else if (m[4]) cls = 'tk-esc'   // \{  \\  etc.
    else if (m[5]) cls = 'tk-brace' // { } [ ] ( )
    else if (m[6]) cls = 'tk-sub'   // _ ^ &
    else if (m[7]) cls = 'tk-op'    // opérateurs
    else if (m[8]) cls = 'tk-num'   // nombres
    out += cls ? `<span class="${cls}">${escapeHtml(tok)}</span>` : escapeHtml(tok)
    last = re.lastIndex
  }
  out += escapeHtml(src.slice(last))
  // garde une ligne vide finale visible
  return out + '\n'
}

export default function LatexEditor({
  value, onChange, taRef,
}: {
  value: string
  onChange: (v: string) => void
  taRef?: React.RefObject<HTMLTextAreaElement | null>
}) {
  const innerRef = useRef<HTMLTextAreaElement>(null)
  const ta = taRef ?? innerRef
  const preRef = useRef<HTMLPreElement>(null)

  // Synchronise le défilement de l'overlay coloré avec le textarea.
  const sync = () => {
    if (preRef.current && ta.current) {
      preRef.current.scrollTop  = ta.current.scrollTop
      preRef.current.scrollLeft = ta.current.scrollLeft
    }
  }
  useEffect(sync, [value]) // eslint-disable-line react-hooks/exhaustive-deps

  const shared: React.CSSProperties = {
    margin: 0,
    border: 0,
    padding: '12px 14px',
    font: FONT,
    lineHeight: '20px',
    whiteSpace: 'pre',
    overflowWrap: 'normal',
    tabSize: 2,
  }

  return (
    <div className="relative flex-1 min-h-0 overflow-hidden bg-white" style={{ contain: 'strict' }}>
      <style>{`
        .latex-hl .tk-cmd   { color:#0b66c3; }
        .latex-hl .tk-env   { color:#8250df; font-weight:600; }
        .latex-hl .tk-esc   { color:#0a7ea4; }
        .latex-hl .tk-brace { color:#9a6700; }
        .latex-hl .tk-sub   { color:#d6336c; font-weight:600; }
        .latex-hl .tk-op    { color:#c2410c; }
        .latex-hl .tk-num   { color:#1e7e34; }
        .latex-hl .tk-com   { color:#6a737d; font-style:italic; }
      `}</style>
      <pre
        ref={preRef}
        aria-hidden
        className="latex-hl absolute inset-0 overflow-auto pointer-events-none"
        style={{ ...shared, color: '#202124' }}
        dangerouslySetInnerHTML={{ __html: highlight(value) }}
      />
      <textarea
        ref={ta}
        value={value}
        onChange={e => onChange(e.target.value)}
        onScroll={sync}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        className="absolute inset-0 w-full h-full resize-none overflow-auto bg-transparent outline-none"
        style={{ ...shared, color: 'transparent', caretColor: '#202124' }}
        placeholder="LaTeX…"
      />
    </div>
  )
}
