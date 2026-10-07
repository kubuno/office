// The web spreadsheet's equation box (SpreadsheetApp.tsx equation overlay: padding 4, border 1, font-size 18px,
// KaTeX display mode) measured in headless Chrome: node box.mjs <page dir> <cases.json> <out.json>
import { spawn } from 'node:child_process'
import { readFileSync, writeFileSync, rmSync } from 'node:fs'
const [pageDir, casesFile, outFile] = process.argv.slice(2)
const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe'
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const profile = pageDir + '/chrome-profile-box'
rmSync(profile, { recursive: true, force: true })
const port = 9592
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
  await send('Page.navigate', { url: 'file:///' + pageDir.split(String.fromCharCode(92)).join('/') + '/box.html' })
  await sleep(1500)
  const cases = JSON.parse(readFileSync(casesFile, 'utf8'))
  const out = []
  for (const src of cases) {
    out.push({ src, ...(await evaluate(`(async () => {
      const inner = document.getElementById('inner'), outer = document.getElementById('outer');
      inner.innerHTML = katex.renderToString(${JSON.stringify(src)}, { displayMode: true, throwOnError: false, strict: false, output: 'html' });
      await document.fonts.ready;
      const o = outer.getBoundingClientRect(), html = inner.querySelector('.katex-html');
      const h = html ? html.getBoundingClientRect() : null;
      const marker = document.createElement('span');
      marker.style.cssText = 'display:inline-block;width:0;height:0;vertical-align:baseline';
      if (html) html.insertBefore(marker, html.firstChild);
      const base = marker.getBoundingClientRect().bottom;
      return { outerW: o.width, outerH: o.height, innerW: inner.offsetWidth, innerH: inner.offsetHeight, htmlX: h ? h.left - o.left : 0, htmlY: h ? h.top - o.top : 0, htmlW: h ? h.width : 0, htmlH: h ? h.height : 0, baseline: base - o.top };
    })()`)) })
  }
  writeFileSync(outFile, JSON.stringify(out, null, 1))
  console.log('cases', out.length)
} finally {
  await send('Browser.close').catch(() => {})
  chrome.kill()
}
