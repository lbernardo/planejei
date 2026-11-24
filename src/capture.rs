// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Captura automática da janela para conferência visual (desenvolvimento).
//!
//! `PLANEJEI_SCREENSHOT=arquivo.png` salva um PNG depois de alguns quadros e fecha o app.
//! `PLANEJEI_VIEW=top|front|right|iso|...` escolhe a vista inicial.

use std::sync::Arc;

use eframe::egui;

pub struct AutoCapture {
    path: String,
    frames: u32,
    requested: bool,
    /// Espera as animações de abertura (janelas aparecem com fade).
    start: std::time::Instant,
}

impl AutoCapture {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("PLANEJEI_SCREENSHOT").ok()?;
        Some(Self {
            path,
            frames: 0,
            requested: false,
            start: std::time::Instant::now(),
        })
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        if self.requested {
            return;
        }
        if self.frames < 8 || self.start.elapsed().as_secs_f32() < 1.0 {
            ctx.request_repaint();
            return;
        }
        self.requested = true;
        let path = self.path.clone();
        let ctx2 = ctx.clone();
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |img: Arc<egui::ColorImage>| {
                if let Err(e) = save_png(&path, &img) {
                    eprintln!("falha ao salvar captura: {e}");
                } else {
                    eprintln!("captura salva em {path}");
                }
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
            }),
        ));
        ctx.request_repaint();
    }
}

/// Recorta um retângulo (em pixels) de uma captura de tela.
pub fn crop(img: &egui::ColorImage, x0: usize, y0: usize, w: usize, h: usize) -> egui::ColorImage {
    let (iw, ih) = (img.size[0], img.size[1]);
    let (x0, y0) = (x0.min(iw), y0.min(ih));
    let (w, h) = (w.min(iw - x0), h.min(ih - y0));
    let mut pixels = Vec::with_capacity(w * h);
    for y in y0..y0 + h {
        pixels.extend_from_slice(&img.pixels[y * iw + x0..y * iw + x0 + w]);
    }
    egui::ColorImage { size: [w, h], pixels, source_size: egui::vec2(w as f32, h as f32) }
}

pub fn save_png(path: &str, img: &egui::ColorImage) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header()?;
    let data: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    w.write_image_data(&data)?;
    Ok(())
}
