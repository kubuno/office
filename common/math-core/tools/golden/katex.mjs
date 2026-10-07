// Golden vectors: KaTeX rendered by headless Chrome, every glyph's position measured, so the Rust layout is
// compared with the real thing. Run on Windows with a local Chrome:
//   node katex.mjs <page dir holding page.html + KaTeX dist> <cases.json> <out.json>
import { spawn } from 'node:child_process'
import { readFileSync, writeFileSync, rmSync } from 'node:fs'
const [pageDir, casesFile, outFile] = process.argv.slice(2)
const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe'
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const profile = pageDir + '/chrome-profile'
rmSync(profile, { recursive: true, force: true })
const port = 9591
const chrome = spawn(CHROME, ['--headless=new', '--disable-gpu', '--renderer-process-limit=1', `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, '--no-first-run', '--allow-file-access-from-files', '--force-device-scale-factor=1', 'about:blank'], { stdio: 'ignore' })
let page
for (let i = 0; i < 150 && !page; i++) { try { page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === 'page') } catch { await sleep(200) } }
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((r) => ws.addEventListener('open', r, { once: true }))
let seq = 0
const pending = new Map()
ws.addEventListener('message', (ev) => { const m = JSON.parse(ev.data); if (m.id && pending.has(m.id)) { const { resolve, reject } = pending.get(m.id); pending.delete(m.id); m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result) } })
const send = (method, params = {}) => new Promise((resolve, reject) => { const id = ++seq; pending.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params })) })
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text); return r.result.value }
try {
  await send('Page.enable')
  await send('Page.navigate', { url: 'file:///' + pageDir.split(String.fromCharCode(92)).join('/') + '/page.html' })
  await sleep(1500)
  const cases = JSON.parse(readFileSync(casesFile, 'utf8'))
  const out = []
  for (const src of cases) {
    const res = await evaluate(`(async () => {
      const host = document.getElementById('host');
      host.innerHTML = katex.renderToString(${JSON.stringify(src)}, { displayMode: true, throwOnError: false, strict: false, output: 'html' });
      await document.fonts.ready;
      const html = host.querySelector('.katex-html');
      if (!html) return { error: true };
      const marker = document.createElement('span');
      marker.style.cssText = 'display:inline-block;width:0;height:0;vertical-align:baseline';
      html.insertBefore(marker, html.firstChild);
      const katexEl = host.querySelector('.katex');
      const em = parseFloat(getComputedStyle(katexEl).fontSize);
      const hr = html.getBoundingClientRect();
      const base = marker.getBoundingClientRect().bottom;
      const glyphs = [];
      const walker = document.createTreeWalker(html, NodeFilter.SHOW_TEXT);
      let n;
      while ((n = walker.nextNode())) {
        const st = getComputedStyle(n.parentElement);
        const text = n.textContent;
        let off = 0;
        for (const ch of text) {
          const r = document.createRange();
          r.setStart(n, off); r.setEnd(n, off + ch.length);
          const rect = r.getBoundingClientRect();
          off += ch.length;
          if (ch === '\u200b') continue;
          glyphs.push({ ch, x: (rect.left - hr.left) / em, bottom: (rect.bottom - base) / em, w: rect.width / em, family: st.fontFamily.split(',')[0].replace(/"/g, '').trim(), weight: st.fontWeight, style: st.fontStyle, size: parseFloat(st.fontSize) / em });
        }
      }
      const rules = [];
      for (const el of html.querySelectorAll('.frac-line, .overline-line, .underline-line, .hline, .rule')) {
        const r = el.getBoundingClientRect();
        rules.push({ x: (r.left - hr.left) / em, w: r.width / em, bottom: (r.bottom - base) / em, h: r.height / em });
      }
      return { em, width: hr.width / em, glyphs, rules };
    })()`)
    out.push({ src, ...res })
  }
  writeFileSync(outFile, JSON.stringify(out, null, 1))
  console.log('cases', out.length)
} finally {
  await send('Browser.close').catch(() => {})
  chrome.kill()
}
