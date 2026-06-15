/**
 * Export PDF côté client, sans dépendance : assemble un PDF 1.4 dont chaque page
 * est l'image JPEG du canvas rendu par le moteur de mise en page (WYSIWYG exact :
 * texte, tableaux, images, en-têtes/pieds, numéros). Les dimensions sont
 * converties px CSS (96 dpi) → points PDF (72 dpi) ; l'image est encodée à
 * l'échelle ×2 pour une netteté correcte à l'impression.
 */

interface PdfPageInput {
  canvas: HTMLCanvasElement
  wPx:    number   // largeur logique de la page (px CSS, 96 dpi)
  hPx:    number
}

const PX_TO_PT = 72 / 96

function dataUrlToBytes(dataUrl: string): Uint8Array {
  const b64 = dataUrl.slice(dataUrl.indexOf(',') + 1)
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

const enc = new TextEncoder()

export function pagesToPdf(pages: PdfPageInput[], title: string): Blob {
  const parts: Uint8Array[] = []
  const offsets: number[] = []          // offset de chaque objet (1-based)
  let length = 0

  const push = (chunk: Uint8Array | string) => {
    const bytes = typeof chunk === 'string' ? enc.encode(chunk) : chunk
    parts.push(bytes)
    length += bytes.length
  }
  const beginObj = (id: number) => { offsets[id] = length; push(`${id} 0 obj\n`) }

  // Objets : 1=Catalog, 2=Pages, 3=Info, puis par page i : Page, Contents, Image.
  const nPages = pages.length
  const pageObjId    = (i: number) => 4 + i * 3
  const contentObjId = (i: number) => 5 + i * 3
  const imageObjId   = (i: number) => 6 + i * 3
  const totalObjs = 3 + nPages * 3

  push('%PDF-1.4\n%âãÏÓ\n')

  beginObj(1)
  push('<< /Type /Catalog /Pages 2 0 R >>\nendobj\n')

  beginObj(2)
  const kids = pages.map((_, i) => `${pageObjId(i)} 0 R`).join(' ')
  push(`<< /Type /Pages /Kids [${kids}] /Count ${nPages} >>\nendobj\n`)

  beginObj(3)
  const safeTitle = title.replace(/[()\\]/g, ' ').slice(0, 200)
  push(`<< /Title (${safeTitle}) /Producer (Kubuno Office) >>\nendobj\n`)

  pages.forEach((pg, i) => {
    const wPt = pg.wPx * PX_TO_PT
    const hPt = pg.hPx * PX_TO_PT
    const jpeg = dataUrlToBytes(pg.canvas.toDataURL('image/jpeg', 0.92))

    beginObj(pageObjId(i))
    push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${wPt.toFixed(2)} ${hPt.toFixed(2)}] ` +
         `/Resources << /XObject << /Im${i} ${imageObjId(i)} 0 R >> /ProcSet [/PDF /ImageC] >> ` +
         `/Contents ${contentObjId(i)} 0 R >>\nendobj\n`)

    const stream = `q\n${wPt.toFixed(2)} 0 0 ${hPt.toFixed(2)} 0 0 cm\n/Im${i} Do\nQ\n`
    beginObj(contentObjId(i))
    push(`<< /Length ${stream.length} >>\nstream\n${stream}endstream\nendobj\n`)

    beginObj(imageObjId(i))
    push(`<< /Type /XObject /Subtype /Image /Width ${pg.canvas.width} /Height ${pg.canvas.height} ` +
         `/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>\nstream\n`)
    push(jpeg)
    push('\nendstream\nendobj\n')
  })

  // Table xref + trailer.
  const xrefStart = length
  let xref = `xref\n0 ${totalObjs + 1}\n0000000000 65535 f \n`
  for (let id = 1; id <= totalObjs; id++) {
    xref += `${String(offsets[id]).padStart(10, '0')} 00000 n \n`
  }
  push(xref)
  push(`trailer\n<< /Size ${totalObjs + 1} /Root 1 0 R /Info 3 0 R >>\nstartxref\n${xrefStart}\n%%EOF\n`)

  return new Blob(parts as BlobPart[], { type: 'application/pdf' })
}

/** Déclenche le téléchargement d'un blob sous `filename`. */
export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 5000)
}
