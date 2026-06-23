// Minimal, dependency-free PDF writer: a single page embedding one JPEG image
// (the rasterised diagram). Enough for "Export PDF" without pulling in pdf-lib.

export function buildJpegPdf(jpeg: Uint8Array, pxW: number, pxH: number): Uint8Array {
  const wPt = +(pxW * 0.75).toFixed(2) // 96dpi px → 72dpi pt
  const hPt = +(pxH * 0.75).toFixed(2)
  const encoder = new TextEncoder()
  const parts: Uint8Array[] = []
  let len = 0
  const offsets: number[] = []
  const push = (x: string | Uint8Array) => { const b = typeof x === 'string' ? encoder.encode(x) : x; parts.push(b); len += b.length }
  const obj = (n: number, body: string) => { offsets[n] = len; push(`${n} 0 obj\n`); push(body); push('\nendobj\n') }

  push('%PDF-1.4\n%âãÏÓ\n')
  obj(1, '<< /Type /Catalog /Pages 2 0 R >>')
  obj(2, '<< /Type /Pages /Kids [3 0 R] /Count 1 >>')
  obj(3, `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${wPt} ${hPt}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>`)
  // Image XObject (JPEG via DCTDecode)
  offsets[4] = len
  push(`4 0 obj\n<< /Type /XObject /Subtype /Image /Width ${pxW} /Height ${pxH} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>\nstream\n`)
  push(jpeg)
  push('\nendstream\nendobj\n')
  const content = `q ${wPt} 0 0 ${hPt} 0 0 cm /Im0 Do Q`
  obj(5, `<< /Length ${content.length} >>\nstream\n${content}\nendstream`)

  const xrefStart = len
  let xref = 'xref\n0 6\n0000000000 65535 f \n'
  for (let i = 1; i <= 5; i++) xref += String(offsets[i]).padStart(10, '0') + ' 00000 n \n'
  push(xref)
  push(`trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xrefStart}\n%%EOF`)

  const out = new Uint8Array(len)
  let o = 0
  for (const p of parts) { out.set(p, o); o += p.length }
  return out
}
