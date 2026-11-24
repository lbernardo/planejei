// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
// No Windows, a versão de release abre só a janela do app (sem o console atrás).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod budget;
#[allow(dead_code)] // motor compartilhado com o GeoArq
mod camera;
#[allow(dead_code)] // motor compartilhado com o GeoArq
mod capture;
mod catalog;
mod cutting;
#[allow(dead_code)] // motor compartilhado com o GeoArq
mod geom;
mod module;
mod project;
#[allow(dead_code)] // motor compartilhado com o GeoArq
mod renderer;
mod reports;
#[allow(dead_code)] // motor compartilhado com o GeoArq
mod textures;

fn main() -> eframe::Result {
    env_logger::init();

    // PLANEJEI_SIZE=1600x900 fixa o tamanho da janela (capturas para vídeo e documentação).
    let size = std::env::var("PLANEJEI_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .unwrap_or([1440.0, 900.0]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Planejei")
            .with_inner_size(size)
            .with_min_inner_size([900.0, 560.0])
            .with_icon(icon()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native("Planejei", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

/// Ícone da janela (Dock/barra de tarefas).
fn icon() -> eframe::egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("assets/icon.png válido")
        .resize(256, 256, image::imageops::FilterType::Triangle)
        .to_rgba8();
    eframe::egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}
