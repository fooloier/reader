use anyhow::{Context, Result, bail, ensure};
use reader_core::{Format, Metadata};
use std::{path::Path, sync::Arc};

pub fn inspect(path: &Path, format: Format, fallback: &str) -> Result<(Metadata, Option<Vec<u8>>)> {
    let fallback = fallback.to_owned();
    match format {
        Format::Pdf => {
            let bytes = Arc::new(std::fs::read(path)?);
            let pdf = hayro::hayro_syntax::Pdf::new(bytes)
                .map_err(|e| anyhow::anyhow!("无法读取 PDF：{e:?}"))?;
            ensure!(!pdf.pages().is_empty(), "PDF 没有页面");
            let m = pdf.metadata();
            let title = m
                .title
                .as_deref()
                .map(decode_pdf_string)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(fallback);
            let authors = m
                .author
                .as_deref()
                .map(decode_pdf_string)
                .filter(|s| !s.trim().is_empty())
                .into_iter()
                .collect();
            let first = &pdf.pages()[0];
            let scale = (240. / first.render_dimensions().0.max(1.)).min(1.);
            let cover = hayro::render_pdf(&pdf, scale, Default::default(), Some(0..=0))
                .and_then(|mut v| v.pop())
                .and_then(|p| {
                    let mut bytes = p.data_as_u8_slice().to_vec();
                    for pixel in bytes.as_chunks_mut::<4>().0 {
                        let alpha = 255 - pixel[3];
                        for c in &mut pixel[..3] {
                            *c = c.saturating_add(alpha);
                        }
                        pixel[3] = 255;
                    }
                    let img =
                        image::RgbaImage::from_raw(p.width().into(), p.height().into(), bytes)?;
                    let mut out = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
                    Some(out.into_inner())
                });
            Ok((
                Metadata {
                    title,
                    authors,
                    year: None,
                },
                cover,
            ))
        }
        Format::Epub => {
            let epub = rbook::Epub::open(path).context("无法读取 EPUB")?;
            ensure!(!epub.spine().is_empty(), "EPUB 没有正文");
            let m = epub.metadata();
            if m.by_property("rendition:layout")
                .any(|v| v.value() == "pre-paginated")
            {
                bail!("暂不支持固定版式 EPUB");
            }
            let title = m
                .title()
                .map(|v| v.value().to_owned())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(fallback);
            let authors = m.creators().map(|v| v.value().to_owned()).collect();
            let year = m
                .published_entry()
                .and_then(|v| v.value().get(..4)?.parse::<i32>().ok())
                .filter(|v| (1..=9999).contains(v));
            let cover = epub
                .manifest()
                .cover_image()
                .and_then(|entry| entry.read_bytes().ok())
                .and_then(|bytes| {
                    let img = image::load_from_memory(&bytes).ok()?.thumbnail(240, 360);
                    let mut out = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
                    Some(out.into_inner())
                });
            Ok((
                Metadata {
                    title,
                    authors,
                    year,
                },
                cover,
            ))
        }
    }
}

pub fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xfe, 0xff]) {
        return String::from_utf16_lossy(
            &bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_be_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        );
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    const SPECIAL: [char; 32] = [
        '•', '†', '‡', '…', '—', '–', 'ƒ', '⁄', '‹', '›', '−', '‰', '„', '“', '”', '‘', '’', '‚',
        '™', 'ﬁ', 'ﬂ', 'Ł', 'Œ', 'Š', 'Ÿ', 'Ž', 'ı', 'ł', 'œ', 'š', 'ž', '�',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9f => SPECIAL[(b - 0x80) as usize],
            0xa0 => '€',
            0x18..=0x1f => ['˘', 'ˇ', 'ˆ', '˙', '˝', '˛', '˚', '˜'][(b - 0x18) as usize],
            _ => char::from(b),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn pdf_text_strings_follow_their_encoding() {
        assert_eq!(
            super::decode_pdf_string(&[0xfe, 0xff, 0x4e, 0x2d, 0x65, 0x87]),
            "中文"
        );
        assert_eq!(super::decode_pdf_string(&[0x80, 0x93, 0xa0]), "•ﬁ€");
        assert_eq!(super::decode_pdf_string(b"\xef\xbb\xbfUTF-8"), "UTF-8");
    }
}
