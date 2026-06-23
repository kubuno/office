// Raw DEFLATE (RFC 1951) inflate — compact, dependency-free. Used to read
// compressed draw.io files (<diagram> payload = base64 → deflateRaw → urlencoded XML).

interface Code { code: number; len: number; sym: number }

const LEN_BASE = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258]
const LEN_EXTRA = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0]
const DIST_BASE = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577]
const DIST_EXTRA = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13]
const CL_ORDER = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15]

function buildTree(lengths: number[]): Code[] {
  const blCount: number[] = new Array(16).fill(0)
  for (const l of lengths) if (l) blCount[l]++
  const nextCode: number[] = new Array(16).fill(0)
  let code = 0
  for (let b = 1; b <= 15; b++) { code = (code + blCount[b - 1]) << 1; nextCode[b] = code }
  const out: Code[] = []
  for (let s = 0; s < lengths.length; s++) {
    const len = lengths[s]
    if (len) out.push({ code: nextCode[len]++, len, sym: s })
  }
  return out
}

export function inflateRaw(input: Uint8Array): Uint8Array {
  let bp = 0
  const out: number[] = []
  const bit = () => { const b = (input[bp >> 3] >> (bp & 7)) & 1; bp++; return b }
  const bits = (n: number) => { let v = 0; for (let i = 0; i < n; i++) v |= bit() << i; return v }
  const decode = (tree: Code[]): number => {
    let code = 0, len = 0
    for (;;) {
      code = (code << 1) | bit(); len++
      for (const c of tree) if (c.len === len && c.code === code) return c.sym
      if (len > 15) throw new Error('inflate: bad code')
    }
  }

  let final = 0
  do {
    final = bit()
    const type = bits(2)
    if (type === 0) {
      bp = (bp + 7) & ~7
      const bytePos = bp >> 3
      const len = input[bytePos] | (input[bytePos + 1] << 8)
      const p = bytePos + 4
      for (let i = 0; i < len; i++) out.push(input[p + i])
      bp = (p + len) << 3
    } else {
      let litTree: Code[], distTree: Code[]
      if (type === 1) {
        const litLens: number[] = []
        for (let i = 0; i <= 143; i++) litLens.push(8)
        for (let i = 144; i <= 255; i++) litLens.push(9)
        for (let i = 256; i <= 279; i++) litLens.push(7)
        for (let i = 280; i <= 287; i++) litLens.push(8)
        litTree = buildTree(litLens)
        distTree = buildTree(new Array(30).fill(5))
      } else {
        const hlit = bits(5) + 257, hdist = bits(5) + 1, hclen = bits(4) + 4
        const clLens = new Array(19).fill(0)
        for (let i = 0; i < hclen; i++) clLens[CL_ORDER[i]] = bits(3)
        const clTree = buildTree(clLens)
        const all: number[] = []
        while (all.length < hlit + hdist) {
          const s = decode(clTree)
          if (s < 16) all.push(s)
          else if (s === 16) { const r = bits(2) + 3; const prev = all[all.length - 1]; for (let i = 0; i < r; i++) all.push(prev) }
          else if (s === 17) { const r = bits(3) + 3; for (let i = 0; i < r; i++) all.push(0) }
          else { const r = bits(7) + 11; for (let i = 0; i < r; i++) all.push(0) }
        }
        litTree = buildTree(all.slice(0, hlit))
        distTree = buildTree(all.slice(hlit))
      }
      for (;;) {
        const s = decode(litTree)
        if (s === 256) break
        if (s < 256) { out.push(s); continue }
        const li = s - 257
        const length = LEN_BASE[li] + bits(LEN_EXTRA[li])
        const ds = decode(distTree)
        const dist = DIST_BASE[ds] + bits(DIST_EXTRA[ds])
        const start = out.length - dist
        for (let i = 0; i < length; i++) out.push(out[start + i])
      }
    }
  } while (!final)
  return new Uint8Array(out)
}
