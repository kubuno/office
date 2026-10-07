//! « Export PDF », a port of `office/web/src/diagramPdf.ts`: a minimal, dependency-free PDF of one page
//! holding one JPEG image (the rasterised diagram), byte for byte what the web writes for the same JPEG.

use kubuno_office_shapes_core::path::js_num;

/// `+(v).toFixed(2)` printed like JavaScript.
fn fixed2(v: f64) -> String {
    js_num(format!("{v:.2}").parse::<f64>().unwrap_or(v))
}

/// `buildJpegPdf(jpeg, pxW, pxH)`: a PDF 1.4 page of `px × 0.75` points (96 dpi pixels to 72 dpi
/// points) showing the JPEG (`DCTDecode`) full page.
pub fn build_jpeg_pdf(jpeg: &[u8], px_w: u32, px_h: u32) -> Vec<u8> {
    let w_pt = fixed2(px_w as f64 * 0.75);
    let h_pt = fixed2(px_h as f64 * 0.75);
    let mut out: Vec<u8> = Vec::with_capacity(jpeg.len() + 1024);
    let mut offsets = [0usize; 6];
    let obj = |out: &mut Vec<u8>, offsets: &mut [usize; 6], n: usize, body: &str| {
        offsets[n] = out.len();
        out.extend_from_slice(format!("{n} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    // TextEncoder writes the four high characters as UTF-8 (two bytes each).
    out.extend_from_slice("%PDF-1.4\n%âãÏÓ\n".as_bytes());
    obj(&mut out, &mut offsets, 1, "<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut out, &mut offsets, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut out,
        &mut offsets,
        3,
        &format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w_pt} {h_pt}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"),
    );
    // The image XObject (JPEG through DCTDecode).
    offsets[4] = out.len();
    out.extend_from_slice(
        format!("4 0 obj\n<< /Type /XObject /Subtype /Image /Width {px_w} /Height {px_h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", jpeg.len())
            .as_bytes(),
    );
    out.extend_from_slice(jpeg);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    let content = format!("q {w_pt} 0 0 {h_pt} 0 0 cm /Im0 Do Q");
    obj(&mut out, &mut offsets, 5, &format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));

    let xref_start = out.len();
    let mut xref = String::from("xref\n0 6\n0000000000 65535 f \n");
    for off in &offsets[1..] {
        xref.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.extend_from_slice(xref.as_bytes());
    out.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF").as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    #[test]
    fn the_xref_points_at_each_object() {
        let jpeg = [0xFFu8, 0xD8, 0x01, 0x02, 0xFF, 0xD9];
        let pdf = build_jpeg_pdf(&jpeg, 201, 100);
        assert!(pdf.starts_with("%PDF-1.4\n%âãÏÓ\n".as_bytes()));
        assert_eq!(&pdf[9..17], &[b'%', 0xC3, 0xA2, 0xC3, 0xA3, 0xC3, 0x8F, 0xC3]);
        let text = String::from_utf8_lossy(&pdf).into_owned();
        assert!(text.contains("/MediaBox [0 0 150.75 75]"), "{text}");
        assert!(text.contains("q 150.75 0 0 75 0 0 cm /Im0 Do Q"));
        assert!(find(&pdf, &jpeg).is_some());
        let xref = find(&pdf, b"xref\n").expect("xref");
        let start: usize = text.rsplit("startxref\n").next().and_then(|t| t.split('\n').next()).and_then(|n| n.parse().ok()).expect("startxref");
        assert_eq!(start, xref);
        let table = &text[text.find("xref\n").expect("xref")..];
        for (n, line) in table.lines().skip(3).take(5).enumerate() {
            let off: usize = line[..10].parse().expect("offset");
            assert!(pdf[off..].starts_with(format!("{} 0 obj\n", n + 1).as_bytes()), "object {}", n + 1);
        }
        assert!(text.ends_with("%%EOF"));
        let content = "q 150.75 0 0 75 0 0 cm /Im0 Do Q";
        assert!(text.contains(&format!("<< /Length {} >>", content.len())));
    }
}
