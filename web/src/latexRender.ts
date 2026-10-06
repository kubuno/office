import katex from 'katex'

// Mini-moteur de rendu LaTeX pour l'aperçu du module Maths.
//
// Deux modes :
//  • Expression mathématique simple (aucun délimiteur ni préambule) → rendue
//    entièrement en mode math display via KaTeX (ex. « x = \frac{-b}{2a} »).
//  • Document mixte (contient \documentclass / \begin{document} / $ / \[ / \() →
//    le préambule est ignoré, le texte est rendu normalement (avec \textbf, etc.)
//    et seules les portions mathématiques délimitées passent par KaTeX.

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function renderMath(tex: string, display: boolean): string {
  try {
    return katex.renderToString(tex, { displayMode: display, throwOnError: false, output: 'html', strict: false })
  } catch {
    return `<span style="color:#d93025">${escapeHtml(tex)}</span>`
  }
}

// Lit un groupe { … } équilibré ; s[start] doit être '{'. Renvoie le contenu et
// l'index juste après l'accolade fermante.
function readBraces(s: string, start: number): { inner: string; next: number } {
  let depth = 0, i = start
  for (; i < s.length; i++) {
    if (s[i] === '{') depth++
    else if (s[i] === '}') { depth--; if (depth === 0) return { inner: s.slice(start + 1, i), next: i + 1 } }
  }
  return { inner: s.slice(start + 1), next: s.length }
}

const TEXT_TAGS: Record<string, string> = {
  textbf: 'strong', textit: 'em', emph: 'em', underline: 'u', texttt: 'code', textsf: 'span',
}

// Rendu du contenu inline d'un paragraphe de texte (commandes + échappement HTML,
// les sauts de ligne simples deviennent des espaces).
function renderInline(s: string): string {
  let out = ''
  let i = 0
  while (i < s.length) {
    const ch = s[i]
    if (ch === '\\') {
      if (s[i + 1] === '\\') { out += '<br/>'; i += 2; continue }        // \\ saut de ligne
      const m = /^\\([a-zA-Z]+)\s*/.exec(s.slice(i))
      if (m) {
        const cmd = m[1]; i += m[0].length
        if (TEXT_TAGS[cmd] && s[i] === '{') {
          const { inner, next } = readBraces(s, i); i = next
          out += `<${TEXT_TAGS[cmd]}>${renderInline(inner)}</${TEXT_TAGS[cmd]}>`
        } else if (s[i] === '{') {                                       // commande inconnue → on garde l'argument brut
          const { inner, next } = readBraces(s, i); i = next
          out += renderInline(inner)
        }
        // sinon commande sans argument inconnue → ignorée
      } else {
        out += escapeHtml(s[i + 1] ?? ''); i += 2                        // caractère échappé (\%, \&, \_, \{ …)
      }
    } else if (ch === '~') { out += '&nbsp;'; i++ }
    else if (ch === '\n') { out += ' '; i++ }
    else { out += escapeHtml(ch); i++ }
  }
  return out
}

// Découpe un segment de texte en paragraphes (ligne vide) ; chaque paragraphe est
// rendu inline.
function renderTextBlock(s: string): string {
  const paras = s.split(/\n[ \t]*\n/).map(p => p.trim()).filter(Boolean)
  return paras.map(p => `<p style="margin:0 0 .7em">${renderInline(p)}</p>`).join('')
}

function stripPreamble(src: string): string {
  const m = /\\begin\{document\}([\s\S]*?)\\end\{document\}/.exec(src)
  let body = m ? m[1] : src
  body = body
    .replace(/\\documentclass\s*(\[[^\]]*\])?\s*\{[^}]*\}/g, '')
    .replace(/\\usepackage\s*(\[[^\]]*\])?\s*\{[^}]*\}/g, '')
  return body
}

// Découpe le corps en segments texte / math (délimiteurs $$ \[ \( $).
function renderBody(body: string): string {
  let out = '', buf = '', i = 0
  const flush = () => { if (buf) { out += renderTextBlock(buf); buf = '' } }
  while (i < body.length) {
    if (body.startsWith('$$', i)) {
      const end = body.indexOf('$$', i + 2)
      if (end >= 0) { flush(); out += renderMath(body.slice(i + 2, end).trim(), true); i = end + 2; continue }
    }
    if (body.startsWith('\\[', i)) {
      const end = body.indexOf('\\]', i + 2)
      if (end >= 0) { flush(); out += renderMath(body.slice(i + 2, end).trim(), true); i = end + 2; continue }
    }
    if (body.startsWith('\\(', i)) {
      const end = body.indexOf('\\)', i + 2)
      if (end >= 0) { flush(); out += renderMath(body.slice(i + 2, end).trim(), false); i = end + 2; continue }
    }
    if (body[i] === '$') {
      const end = body.indexOf('$', i + 1)
      if (end >= 0) { flush(); out += renderMath(body.slice(i + 1, end).trim(), false); i = end + 1; continue }
    }
    buf += body[i]; i++
  }
  flush()
  return out
}

export function isLatexDocument(src: string): boolean {
  return /\\documentclass|\\begin\{document\}|\$|\\\[|\\\(/.test(src)
}

/** Rend une source LaTeX en HTML. `doc` indique le mode (document mixte vs math pur). */
export function renderLatex(src: string): { html: string; doc: boolean } {
  if (!src.trim()) return { html: '', doc: false }
  if (isLatexDocument(src)) {
    return { html: renderBody(stripPreamble(src)), doc: true }
  }
  return { html: renderMath(src, true), doc: false }
}
