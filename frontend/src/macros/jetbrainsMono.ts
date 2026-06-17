// Police JetBrains Mono EMBARQUÉE (self-hosted, OFL) pour l'éditeur de macros.
// Les .woff2 sont importés en assets Vite (URL hashée, base relative) puis injectés
// via @font-face une seule fois. Monaco l'utilise via fontFamily 'JetBrains Mono'.
import regularUrl from '../assets/JetBrainsMono-Regular.woff2'
import boldUrl from '../assets/JetBrainsMono-Bold.woff2'

export const JETBRAINS_MONO = "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Menlo, Consolas, monospace"

let injected = false
export function ensureJetBrainsMono(): void {
  if (injected || typeof document === 'undefined') return
  injected = true
  const style = document.createElement('style')
  style.id = 'kubuno-jetbrains-mono'
  style.textContent =
    `@font-face{font-family:'JetBrains Mono';font-weight:400;font-style:normal;font-display:swap;src:url('${regularUrl}') format('woff2')}` +
    `@font-face{font-family:'JetBrains Mono';font-weight:700;font-style:normal;font-display:swap;src:url('${boldUrl}') format('woff2')}`
  document.head.appendChild(style)
}
