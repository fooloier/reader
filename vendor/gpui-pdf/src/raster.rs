use std::sync::{Arc, LazyLock, Weak};

use async_channel::{Receiver, Sender, bounded, unbounded};
use gpui::RenderImage;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::vello_cpu::peniko::ImageAlphaType::AlphaPremultiplied;
use image::{Frame, RgbaImage};

use crate::Document;

/// Rasterize a single page (0-based) of an already-parsed [`Document`] at `scale`
/// (PDF point-size × this) to a BGRA `RenderImage` composited onto white. Higher
/// scale = sharper but more memory; [`crate::PdfView`] picks `scale` from the
/// display's pixel ratio, zoom, and quality so pages are crisp without wasting memory.
pub fn render_page(doc: &Document, idx: usize, scale: f32) -> Result<Arc<RenderImage>, String> {
    let pixmaps = hayro::render_pdf(doc, scale, InterpreterSettings::default(), Some(idx..=idx))
        .ok_or_else(|| format!("render page {idx}"))?;
    let pixmap = pixmaps
        .into_iter()
        .next()
        .ok_or_else(|| format!("no page {idx}"))?;

    let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    let mut bgra = pixmap.take_rgba8(AlphaPremultiplied);
    composite_white_bgra(&mut bgra);
    let buf = RgbaImage::from_raw(w, h, bgra).ok_or_else(|| "bad pixel buffer".to_string())?;
    Ok(Arc::new(RenderImage::new([Frame::new(buf)])))
}

fn composite_white_bgra(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let [red, green, blue, alpha] = *pixel;
        let white = 255 - alpha;
        *pixel = [
            blue.saturating_add(white),
            green.saturating_add(white),
            red.saturating_add(white),
            255,
        ];
    }
}

pub(crate) type RasterReceiver = Receiver<Option<Arc<RenderImage>>>;

pub(crate) struct RasterRequest(RasterReceiver);

impl RasterRequest {
    pub(crate) fn queue(doc: &Arc<Document>, page: usize, scale: f32) -> Self {
        let (request, job) = RasterJob::new(doc, page, scale);
        let _ = RASTER_JOBS.try_send(job);
        request
    }

    pub(crate) fn receiver(&self) -> RasterReceiver {
        self.0.clone()
    }

    pub(crate) fn matches(&self, result: &RasterReceiver) -> bool {
        self.0.same_channel(result)
    }
}

impl Drop for RasterRequest {
    fn drop(&mut self) {
        self.0.close();
        let _ = self.0.try_recv();
    }
}

struct RasterJob {
    doc: Weak<Document>,
    page: usize,
    scale: f32,
    result: Sender<Option<Arc<RenderImage>>>,
}

impl RasterJob {
    fn new(doc: &Arc<Document>, page: usize, scale: f32) -> (RasterRequest, Self) {
        let (result, receiver) = bounded(1);
        let job = Self {
            doc: Arc::downgrade(doc),
            page,
            scale,
            result,
        };
        (RasterRequest(receiver), job)
    }

    fn run(self, render: impl FnOnce(&Document, usize, f32) -> Result<Arc<RenderImage>, String>) {
        if self.result.is_closed() {
            return;
        }
        let image = self
            .doc
            .upgrade()
            .and_then(|doc| render(&doc, self.page, self.scale).ok());
        let _ = self.result.try_send(image);
    }
}

static RASTER_JOBS: LazyLock<Sender<RasterJob>> = LazyLock::new(|| {
    let (sender, jobs) = unbounded::<RasterJob>();
    for worker in 0..2 {
        let jobs = jobs.clone();
        std::thread::Builder::new()
            .name(format!("pdf-raster-{worker}"))
            .spawn(move || {
                while let Ok(job) = jobs.recv_blocking() {
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        job.run(render_page)
                    }))
                    .is_err()
                    {
                        log::error!("PDF page rasterization panicked");
                    }
                }
            })
            .expect("start PDF raster worker");
    }
    sender
});

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn two_pixel_pdf() -> Arc<Document> {
        let content = "1 0 0 rg 0 0 1 1 re f\n";
        let stream = format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        );
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 2 1] /Contents 4 0 R >>",
            &stream,
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let offsets: Vec<_> = objects
            .iter()
            .enumerate()
            .map(|(i, object)| {
                let offset = pdf.len();
                writeln!(pdf, "{} 0 obj\n{object}\nendobj", i + 1).unwrap();
                offset
            })
            .collect();
        let xref = pdf.len();
        writeln!(pdf, "xref\n0 5\n0000000000 65535 f ").unwrap();
        for offset in offsets {
            writeln!(pdf, "{offset:010} 00000 n ").unwrap();
        }
        writeln!(
            pdf,
            "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
        )
        .unwrap();
        crate::parse(Arc::new(pdf)).unwrap()
    }

    #[test]
    fn cancelled_request_skips_queued_rasterization() {
        let doc = two_pixel_pdf();
        let (request, job) = RasterJob::new(&doc, 0, 1.0);
        let result = request.receiver();
        drop(request);

        job.run(|_, _, _| panic!("cancelled job reached the rasterizer"));
        assert!(result.recv_blocking().is_err());
    }

    #[test]
    fn queued_job_does_not_retain_document() {
        let doc = two_pixel_pdf();
        let (request, job) = RasterJob::new(&doc, 0, 1.0);
        drop(doc);

        job.run(|_, _, _| panic!("job reached the rasterizer after the document was dropped"));
        assert!(request.receiver().recv_blocking().unwrap().is_none());
    }

    #[test]
    fn completed_request_cannot_clear_reused_page_slot() {
        let doc = two_pixel_pdf();
        let (first, first_job) = RasterJob::new(&doc, 0, 1.0);
        let first_result = first.receiver();
        let mut slot = crate::Slot {
            raster: Some(first),
            ..Default::default()
        };
        first_job.run(render_page);
        assert!(first_result.recv_blocking().unwrap().is_some());

        let (next, next_job) = RasterJob::new(&doc, 0, 2.0);
        let next_result = next.receiver();
        slot.raster = Some(next);
        assert!(!slot.take_raster(&first_result));

        next_job.run(render_page);
        assert!(next_result.recv_blocking().unwrap().is_some());
        assert!(slot.take_raster(&next_result));
    }

    #[test]
    fn shared_worker_renders_opaque_bgra_pixels() {
        let doc = two_pixel_pdf();
        let request = RasterRequest::queue(&doc, 0, 1.0);
        let image = request.receiver().recv_blocking().unwrap().unwrap();

        assert_eq!(
            image.as_bytes(0).unwrap(),
            [0, 0, 255, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn white_composite_preserves_premultiplied_colors() {
        let mut pixels = [0, 0, 0, 0, 90, 40, 10, 128, 200, 100, 50, 255];
        composite_white_bgra(&mut pixels);
        assert_eq!(
            pixels,
            [255, 255, 255, 255, 137, 167, 217, 255, 50, 100, 200, 255]
        );
    }
}
