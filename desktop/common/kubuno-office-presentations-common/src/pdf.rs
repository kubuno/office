//! The PDF export (`pdfExport.ts` `pagesToPdf`): a PDF 1.4 whose every page is the JPEG of a rendered slide, CSS
//! pixels (96 dpi) converted to points (72 dpi). No dependency: the bytes are assembled here, so every OS writes
//! the same file once its window has rendered the slides.

/// One page: its JPEG, the image's pixel size, and the page's logical size in CSS pixels.
pub struct Page {
    pub jpeg: Vec<u8>,
    pub image_w: u32,
    pub image_h: u32,
    pub w_px: f64,
    pub h_px: f64,
}

const PX_TO_PT: f64 = 72.0 / 96.0;

/// `pagesToPdf`.
pub fn pages_to_pdf(pages: &[Page], title: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let n = pages.len();
    let total = 3 + n * 3;
    let mut offsets = vec![0usize; total + 1];
    let page_id = |i: usize| 4 + i * 3;
    let content_id = |i: usize| 5 + i * 3;
    let image_id = |i: usize| 6 + i * 3;
    out.extend_from_slice("%PDF-1.4\n%âãÏÓ\n".as_bytes());
    let begin = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, id: usize| {
        offsets[id] = out.len();
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    };
    begin(&mut out, &mut offsets, 1);
    out.extend_from_slice(b"<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    begin(&mut out, &mut offsets, 2);
    let kids = (0..n).map(|i| format!("{} 0 R", page_id(i))).collect::<Vec<_>>().join(" ");
    out.extend_from_slice(format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>\nendobj\n").as_bytes());
    begin(&mut out, &mut offsets, 3);
    let safe: String = title.chars().map(|c| if matches!(c, '(' | ')' | '\\') { ' ' } else { c }).take(200).collect();
    out.extend_from_slice(format!("<< /Title ({safe}) /Producer (Kubuno Office) >>\nendobj\n").as_bytes());
    for (i, pg) in pages.iter().enumerate() {
        let (w, h) = (pg.w_px * PX_TO_PT, pg.h_px * PX_TO_PT);
        begin(&mut out, &mut offsets, page_id(i));
        out.extend_from_slice(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.2} {h:.2}] /Resources << /XObject << /Im{i} {} 0 R >> /ProcSet [/PDF /ImageC] >> /Contents {} 0 R >>\nendobj\n",
                image_id(i),
                content_id(i)
            )
            .as_bytes(),
        );
        let stream = format!("q\n{w:.2} 0 0 {h:.2} 0 0 cm\n/Im{i} Do\nQ\n");
        begin(&mut out, &mut offsets, content_id(i));
        out.extend_from_slice(format!("<< /Length {} >>\nstream\n{stream}endstream\nendobj\n", stream.len()).as_bytes());
        begin(&mut out, &mut offsets, image_id(i));
        out.extend_from_slice(
            format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", pg.image_w, pg.image_h, pg.jpeg.len()).as_bytes(),
        );
        out.extend_from_slice(&pg.jpeg);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    }
    let xref_start = out.len();
    let mut xref = format!("xref\n0 {}\n0000000000 65535 f \n", total + 1);
    for off in offsets.iter().skip(1) {
        xref.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.extend_from_slice(xref.as_bytes());
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref_start}\n%%EOF\n", total + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_xref_points_at_every_object() {
        let pages = [Page { jpeg: vec![0xff, 0xd8, 0xff, 0xd9], image_w: 2, image_h: 1, w_px: 1920.0, h_px: 1080.0 }];
        let pdf = pages_to_pdf(&pages, "Deck (1)");
        let text = String::from_utf8_lossy(&pdf).into_owned();
        assert!(text.starts_with("%PDF-1.4\n"));
        assert!(text.contains("/Title (Deck  1 )"));
        assert!(text.contains("/MediaBox [0 0 1440.00 810.00]"));
        let start: usize = text.rsplit("startxref\n").next().and_then(|s| s.lines().next()).and_then(|s| s.parse().ok()).unwrap_or(0);
        // Offsets count bytes: read the table from the bytes (the JPEG is not UTF-8).
        let tail = String::from_utf8_lossy(&pdf[start..]).into_owned();
        assert!(tail.starts_with("xref\n0 7\n"), "{tail}");
        for (id, line) in tail.lines().skip(3).take(6).enumerate() {
            let off: usize = line[..10].parse().unwrap_or(0);
            assert!(pdf[off..].starts_with(format!("{} 0 obj", id + 1).as_bytes()), "object {}", id + 1);
        }
    }
}
