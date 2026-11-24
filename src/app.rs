// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Interface: menus e painéis no estilo do Promob (Galeria à esquerda, Propriedades à direita,
//! abas 3D / Plano de corte / Orçamento no centro), com a navegação do GeoArq.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, CursorIcon, Key, PointerButton, Rect, Sense, Stroke};
use glam::{DAffine3, DVec3, Mat4, Vec2, Vec3, Vec4};

use crate::budget::{self, Budget, brl, num};
use crate::camera::{Camera, Projection, StandardView};
use crate::catalog::{HardwareKind, Material};
use crate::cutting::Plan;
use crate::geom::Mesh;
use crate::module::{Module, Preset, Role, gallery};
use crate::project::{self, Project, Takeoff};
use crate::renderer::{self, FaceStyle, Globals, ViewportRenderer, srgb_to_linear4};
use crate::reports;
use crate::textures::Texture;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    View3d,
    Cut,
    Budget,
}

#[derive(Clone, Copy)]
enum FileCmd {
    New,
    Open,
    Example,
    Save,
    SaveAs,
}

struct Drag {
    idx: usize,
    /// Centro do módulo − ponto agarrado (mm, no piso).
    offset: [f64; 2],
    /// Altura do plano horizontal do arraste (m).
    plane_z: f32,
}

/// Uma caixa desenhada no 3D.
struct Solid {
    mesh: Mesh,
    color: [f32; 3],
    texture: Option<u32>,
    module: Option<usize>,
}

pub struct App {
    project: Project,
    path: Option<PathBuf>,
    saved: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,
    /// Edição contínua (arraste, digitação) vira um passo só de desfazer.
    merging: bool,

    // Derivados do projeto (recalculados quando ele muda).
    derived_for: Option<Project>,
    takeoff: Takeoff,
    plan: Plan,
    budget: Budget,

    selected: Option<usize>,
    tab: Tab,
    hide_fronts: bool,
    show_gallery: bool,
    show_props: bool,
    filter: String,
    gallery: Vec<Preset>,

    camera: Camera,
    renderer: ViewportRenderer,
    textures: Vec<Texture>,
    geoarq: Vec<Mesh>,
    scene_key: Option<(Project, Option<usize>, bool, usize, [bool; 4])>,
    aspect: f32,
    viewport_h: f32,
    needs_fit: bool,
    drag: Option<Drag>,

    show_catalog: bool,
    show_parts: bool,
    show_shortcuts: bool,
    show_about: bool,
    notice: Option<(String, bool)>,
    queued: Vec<FileCmd>,
    capture: Option<crate::capture::AutoCapture>,
}

const FLOOR_TEXTURE: &str = "Piso de madeira";

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("o backend wgpu é obrigatório");
        cc.egui_ctx.set_visuals(egui::Visuals::light());

        let mut project = match std::env::var("PLANEJEI_DEMO").as_deref() {
            Ok("kitchen") => Project::demo_kitchen(),
            _ => Project::default(),
        };
        let mut path = None;
        let mut notice = None;
        if let Some(arg) = std::env::args().nth(1) {
            match Project::load(Path::new(&arg)) {
                Ok(p) => {
                    project = p;
                    path = Some(PathBuf::from(arg));
                }
                Err(e) => notice = Some((e, true)),
            }
        }
        let textures = [FLOOR_TEXTURE, crate::catalog::WOOD_TEXTURE].iter().filter_map(|n| crate::textures::builtin(n)).collect();
        let mut camera = Camera::default();
        if let Ok(v) = std::env::var("PLANEJEI_VIEW") {
            for part in v.split(',') {
                if let Some(view) = StandardView::from_name(part) {
                    camera.set_view(view);
                }
                if part == "parallel" {
                    camera.projection = Projection::Parallel;
                }
            }
        }
        let tab = match std::env::var("PLANEJEI_TAB").as_deref() {
            Ok("cut") => Tab::Cut,
            Ok("budget") => Tab::Budget,
            _ => Tab::View3d,
        };
        let selected = std::env::var("PLANEJEI_SELECT").ok().and_then(|s| s.parse().ok()).filter(|i| *i < project.modules.len());
        let mut app = Self {
            saved: project.clone(),
            project,
            path,
            undo: Vec::new(),
            redo: Vec::new(),
            merging: false,
            derived_for: None,
            takeoff: Takeoff { boards: Vec::new(), hardware: Vec::new() },
            plan: Plan::default(),
            budget: Budget::default(),
            selected,
            tab,
            hide_fronts: std::env::var("PLANEJEI_HIDE_FRONTS").is_ok(),
            show_gallery: true,
            show_props: true,
            filter: String::new(),
            gallery: gallery(),
            camera,
            renderer: ViewportRenderer::new(rs),
            textures,
            geoarq: Vec::new(),
            scene_key: None,
            aspect: 1.6,
            viewport_h: 800.0,
            needs_fit: true,
            drag: None,
            show_catalog: std::env::var("PLANEJEI_CATALOG").is_ok(),
            show_parts: std::env::var("PLANEJEI_PARTS").is_ok(),
            show_shortcuts: false,
            show_about: std::env::var("PLANEJEI_ABOUT").is_ok(),
            notice,
            queued: Vec::new(),
            capture: crate::capture::AutoCapture::from_env(),
        };
        app.reload_geoarq();
        // PLANEJEI_DUMP=pasta grava o projeto e as exportações (desenvolvimento).
        if let Ok(dir) = std::env::var("PLANEJEI_DUMP") {
            app.refresh_derived();
            let dir = Path::new(&dir);
            let files = [
                ("projeto.planejei", app.project.to_json()),
                ("pecas.csv", reports::parts_csv(&app.project, &app.takeoff)),
                ("plano-de-corte.svg", reports::cut_svg(&app.project, &app.plan)),
                ("proposta.html", reports::budget_html(&app.project, &app.takeoff, &app.plan, &app.budget)),
            ];
            for (name, body) in files {
                if let Err(e) = std::fs::write(dir.join(name), body) {
                    eprintln!("falha ao gravar {name}: {e}");
                }
            }
        }
        app
    }

    // ------------------------------------------------------------------ estado

    fn dirty(&self) -> bool {
        self.project != self.saved
    }

    fn refresh_derived(&mut self) {
        if self.derived_for.as_ref() == Some(&self.project) {
            return;
        }
        self.takeoff = self.project.takeoff();
        self.plan = self.project.cut_plan(&self.takeoff);
        self.budget = budget::compute(&self.project, &self.takeoff, &self.plan);
        self.derived_for = Some(self.project.clone());
        if self.selected.is_some_and(|i| i >= self.project.modules.len()) {
            self.selected = None;
        }
    }

    /// Troca o projeto inteiro (novo/abrir): zera o histórico.
    fn replace_project(&mut self, p: Project, path: Option<PathBuf>) {
        self.project = p;
        self.saved = self.project.clone();
        self.path = path;
        self.undo.clear();
        self.redo.clear();
        self.selected = None;
        self.needs_fit = true;
        self.reload_geoarq();
    }

    fn reload_geoarq(&mut self) {
        self.geoarq.clear();
        self.scene_key = None;
        if let Some(p) = self.project.geoarq.clone() {
            match project::load_geoarq(&p) {
                Ok(m) => self.geoarq = m,
                Err(e) => self.notice = Some((format!("Ambiente do GeoArq: {e}"), true)),
            }
        }
    }

    fn undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, p));
        }
    }

    fn redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, p));
        }
    }

    fn insert(&mut self, make: fn() -> Module) {
        let mut m = make();
        self.project.auto_place(&mut m);
        self.project.modules.push(m);
        self.selected = Some(self.project.modules.len() - 1);
        self.tab = Tab::View3d;
    }

    fn delete_selected(&mut self) {
        if let Some(i) = self.selected.take() {
            self.project.modules.remove(i);
        }
    }

    fn duplicate_selected(&mut self) {
        if let Some(i) = self.selected {
            let mut m = self.project.modules[i].clone();
            let f = m.footprint();
            match m.rot {
                0 => m.pos[0] += f[0],
                2 => m.pos[0] -= f[0],
                1 => m.pos[1] += f[1],
                _ => m.pos[1] -= f[1],
            }
            self.project.modules.push(m);
            self.selected = Some(self.project.modules.len() - 1);
        }
    }

    fn rotate_selected(&mut self) {
        if let Some(i) = self.selected {
            let m = &mut self.project.modules[i];
            let (a, b) = m.bounds();
            let c = (a + b) / 2.0;
            m.rot = (m.rot + 1) % 4;
            let f = m.footprint();
            m.pos[0] = c.x - f[0] / 2.0;
            m.pos[1] = c.y - f[1] / 2.0;
        }
    }

    fn nudge(&mut self, dx: f64, dy: f64) {
        if let Some(i) = self.selected {
            self.project.modules[i].pos[0] += dx;
            self.project.modules[i].pos[1] += dy;
        }
    }

    // ------------------------------------------------------------------ arquivos

    fn confirm_discard(&mut self) -> bool {
        if !self.dirty() {
            return true;
        }
        let r = rfd::MessageDialog::new()
            .set_title("Planejei")
            .set_description("Salvar as alterações do projeto antes de continuar?")
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        match r {
            rfd::MessageDialogResult::Yes => self.save(false),
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    fn save(&mut self, ask: bool) -> bool {
        let path = match (&self.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("Projeto Planejei", &[project::EXTENSION])
                .set_file_name(format!("{}.{}", self.project.name, project::EXTENSION))
                .save_file(),
        };
        let Some(path) = path else { return false };
        match self.project.save(&path) {
            Ok(()) => {
                self.saved = self.project.clone();
                self.notice = Some((format!("Salvo em {}", path.display()), false));
                self.path = Some(path);
                true
            }
            Err(e) => {
                self.notice = Some((e, true));
                false
            }
        }
    }

    fn run_file_cmds(&mut self) {
        for cmd in std::mem::take(&mut self.queued) {
            match cmd {
                FileCmd::New => {
                    if self.confirm_discard() {
                        self.replace_project(Project::default(), None);
                    }
                }
                FileCmd::Example => {
                    if self.confirm_discard() {
                        self.replace_project(Project::demo_kitchen(), None);
                    }
                }
                FileCmd::Open => {
                    if self.confirm_discard()
                        && let Some(path) = rfd::FileDialog::new().add_filter("Projeto Planejei", project::OPEN_EXTENSIONS).pick_file()
                    {
                        match Project::load(&path) {
                            Ok(p) => self.replace_project(p, Some(path)),
                            Err(e) => self.notice = Some((e, true)),
                        }
                    }
                }
                FileCmd::Save => {
                    self.save(false);
                }
                FileCmd::SaveAs => {
                    self.save(true);
                }
            }
        }
    }

    fn import_geoarq(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Modelo GeoArq", &["geoarq", "skuo"]).pick_file() else { return };
        match project::load_geoarq(&path) {
            Ok(m) => {
                self.geoarq = m;
                self.scene_key = None;
                self.project.geoarq = Some(path);
                self.project.room.show_walls = false;
                self.needs_fit = true;
                self.notice = Some(("Ambiente do GeoArq importado (as paredes simples foram ocultadas).".into(), false));
            }
            Err(e) => self.notice = Some((e, true)),
        }
    }

    fn export(&mut self, kind: &str) {
        let (filter, ext) = match kind {
            "csv" => ("Lista de peças (CSV)", "csv"),
            "svg" => ("Plano de corte (SVG)", "svg"),
            _ => ("Orçamento (HTML)", "html"),
        };
        let suffix = match kind {
            "csv" => "pecas",
            "svg" => "plano-de-corte",
            _ => "orcamento",
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter(filter, &[ext])
            .set_file_name(format!("{} - {suffix}.{ext}", self.project.name))
            .save_file()
        else {
            return;
        };
        self.refresh_derived();
        let text = match kind {
            "csv" => reports::parts_csv(&self.project, &self.takeoff),
            "svg" => reports::cut_svg(&self.project, &self.plan),
            _ => reports::budget_html(&self.project, &self.takeoff, &self.plan, &self.budget),
        };
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.notice = Some((format!("Exportado: {}", path.display()), false));
                if kind != "csv" {
                    open_path(&path);
                }
            }
            Err(e) => self.notice = Some((format!("Não foi possível exportar: {e}"), true)),
        }
    }

    // ------------------------------------------------------------------ menus

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Arquivo", |ui| {
                for (label, sc, cmd) in [
                    ("Novo", "⌘N", FileCmd::New),
                    ("Abrir…", "⌘O", FileCmd::Open),
                    ("Abrir exemplo (cozinha)", "", FileCmd::Example),
                    ("Salvar", "⌘S", FileCmd::Save),
                    ("Salvar como…", "⇧⌘S", FileCmd::SaveAs),
                ] {
                    if ui.add(egui::Button::new(label).shortcut_text(kb(sc))).clicked() {
                        self.queued.push(cmd);
                    }
                }
                ui.separator();
                if ui.button("Importar ambiente do GeoArq…").clicked() {
                    self.import_geoarq();
                }
                if ui.add_enabled(self.project.geoarq.is_some(), egui::Button::new("Remover ambiente importado")).clicked() {
                    self.project.geoarq = None;
                    self.project.room.show_walls = true;
                    self.geoarq.clear();
                    self.scene_key = None;
                }
                ui.separator();
                ui.menu_button("Exportar", |ui| self.export_items(ui));
                ui.separator();
                if ui.button("Sair").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Editar", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Desfazer").shortcut_text(kb("⌘Z"))).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Refazer").shortcut_text(kb("⇧⌘Z"))).clicked() {
                    self.redo();
                }
                ui.separator();
                self.module_actions(ui);
                ui.separator();
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("Selecionar nenhum").shortcut_text("Esc")).clicked() {
                    self.selected = None;
                }
            });
            ui.menu_button("Visualizar", |ui| {
                for (v, key) in [
                    (StandardView::Iso, "0"),
                    (StandardView::Top, "1"),
                    (StandardView::Front, "2"),
                    (StandardView::Back, "3"),
                    (StandardView::Left, "4"),
                    (StandardView::Right, "5"),
                ] {
                    if ui.add(egui::Button::new(v.label()).shortcut_text(key)).clicked() {
                        self.set_view(v);
                    }
                }
                if ui.add(egui::Button::new("Zoom extensões").shortcut_text("Shift+Z")).clicked() {
                    self.needs_fit = true;
                }
                let parallel = self.camera.projection == Projection::Parallel;
                if ui.add(egui::Button::new("Projeção paralela").selected(parallel)).clicked() {
                    self.camera.projection = if parallel { Projection::Perspective } else { Projection::Parallel };
                }
                ui.separator();
                ui.checkbox(&mut self.hide_fronts, "Ocultar portas e frentes (F)");
                ui.checkbox(&mut self.project.room.show_walls, "Paredes do ambiente");
                ui.checkbox(&mut self.project.room.front_wall, "Parede da frente");
                ui.separator();
                ui.checkbox(&mut self.show_gallery, "Painel Galeria");
                ui.checkbox(&mut self.show_props, "Painel Propriedades");
            });
            ui.menu_button("Ambiente", |ui| {
                if ui.button("Dimensões do ambiente…").clicked() {
                    self.selected = None;
                    self.show_props = true;
                }
                ui.checkbox(&mut self.project.room.show_walls, "Paredes do ambiente");
                ui.checkbox(&mut self.project.room.front_wall, "Parede da frente");
                ui.separator();
                if ui.button("Importar ambiente do GeoArq…").clicked() {
                    self.import_geoarq();
                }
            });
            ui.menu_button("Inserir", |ui| {
                let mut chosen = None;
                let mut cats: Vec<&str> = self.gallery.iter().map(|p| p.category).collect();
                cats.dedup();
                for cat in cats {
                    ui.menu_button(cat, |ui| {
                        let mut groups: Vec<&str> = self.gallery.iter().filter(|p| p.category == cat).map(|p| p.group).collect();
                        groups.dedup();
                        for g in groups {
                            ui.menu_button(g, |ui| {
                                for p in self.gallery.iter().filter(|p| p.category == cat && p.group == g) {
                                    if ui.button(p.name).clicked() {
                                        chosen = Some(p.make);
                                    }
                                }
                            });
                        }
                    });
                }
                if let Some(make) = chosen {
                    self.insert(make);
                }
            });
            ui.menu_button("Ferramentas", |ui| {
                if ui.button("Materiais, ferragens e preços…").clicked() {
                    self.show_catalog = true;
                }
                if ui.button("Parâmetros de corte…").clicked() {
                    self.show_catalog = true;
                }
            });
            ui.menu_button("Relatórios", |ui| {
                if ui.button("Lista de peças…").clicked() {
                    self.show_parts = true;
                }
                if ui.button("Plano de corte").clicked() {
                    self.tab = Tab::Cut;
                }
                if ui.button("Orçamento").clicked() {
                    self.tab = Tab::Budget;
                }
                ui.separator();
                self.export_items(ui);
            });
            ui.menu_button("Ajuda", |ui| {
                if ui.button("Atalhos de teclado").clicked() {
                    self.show_shortcuts = true;
                }
                if ui.button("Sobre o Planejei").clicked() {
                    self.show_about = true;
                }
            });
        });
    }

    fn export_items(&mut self, ui: &mut egui::Ui) {
        if ui.button("Lista de peças (CSV)…").clicked() {
            self.export("csv");
        }
        if ui.button("Plano de corte (SVG)…").clicked() {
            self.export("svg");
        }
        if ui.button("Orçamento completo (HTML/PDF)…").clicked() {
            self.export("html");
        }
    }

    fn module_actions(&mut self, ui: &mut egui::Ui) {
        let has = self.selected.is_some();
        if ui.add_enabled(has, egui::Button::new("Duplicar").shortcut_text(kb("⌘D"))).clicked() {
            self.duplicate_selected();
        }
        if ui.add_enabled(has, egui::Button::new("Girar 90°").shortcut_text("R")).clicked() {
            self.rotate_selected();
        }
        if ui.add_enabled(has, egui::Button::new("Excluir").shortcut_text("Delete")).clicked() {
            self.delete_selected();
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let b = |ui: &mut egui::Ui, label: &str, tip: &str, enabled: bool| {
                ui.add_enabled(enabled, egui::Button::new(label).min_size(egui::vec2(0.0, 26.0))).on_hover_text(tip).clicked()
            };
            if b(ui, "Novo", "Novo projeto", true) {
                self.queued.push(FileCmd::New);
            }
            if b(ui, "Abrir", "Abrir projeto", true) {
                self.queued.push(FileCmd::Open);
            }
            if b(ui, "Salvar", "Salvar projeto", true) {
                self.queued.push(FileCmd::Save);
            }
            ui.separator();
            if b(ui, "Desfazer", "Desfazer", !self.undo.is_empty()) {
                self.undo();
            }
            if b(ui, "Refazer", "Refazer", !self.redo.is_empty()) {
                self.redo();
            }
            ui.separator();
            let has = self.selected.is_some();
            if b(ui, "Duplicar", "Duplicar módulo", has) {
                self.duplicate_selected();
            }
            if b(ui, "Girar", "Girar 90° (R)", has) {
                self.rotate_selected();
            }
            if b(ui, "Excluir", "Excluir módulo (Delete)", has) {
                self.delete_selected();
            }
            ui.separator();
            for (v, label) in [
                (StandardView::Iso, "Iso"),
                (StandardView::Top, "Topo"),
                (StandardView::Front, "Frente"),
                (StandardView::Left, "Esquerda"),
                (StandardView::Right, "Direita"),
            ] {
                if b(ui, label, v.label(), true) {
                    self.set_view(v);
                    self.tab = Tab::View3d;
                }
            }
            ui.separator();
            ui.toggle_value(&mut self.hide_fronts, "Ocultar frentes").on_hover_text("Ocultar portas e frentes de gaveta (F)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (tab, label) in [(Tab::Budget, "Orçamento"), (Tab::Cut, "Plano de corte"), (Tab::View3d, "3D")] {
                    ui.selectable_value(&mut self.tab, tab, egui::RichText::new(label).strong());
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.notice {
                Some((n, true)) => ui.colored_label(Color32::from_rgb(190, 40, 40), n),
                Some((n, false)) => ui.colored_label(Color32::from_rgb(30, 110, 60), n),
                None => ui.label(match self.selected {
                    Some(_) => "Arraste o módulo para mover (encaixa nas paredes e nos vizinhos) · R gira · setas ajustam 10 mm (Shift: 100 mm)",
                    None => "Escolha um item na Galeria para inserir · clique num módulo para editar · arraste o fundo para orbitar",
                }),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.strong(brl(self.budget.total));
                ui.label(format!(
                    "{} módulos · {} peças · {} chapas ·",
                    self.project.modules.len(),
                    self.takeoff.boards.len(),
                    self.plan.sheets.len()
                ));
                if !self.plan.too_big.is_empty() {
                    ui.colored_label(Color32::from_rgb(190, 40, 40), format!("{} peça(s) maiores que a chapa ·", self.plan.too_big.len()));
                }
            });
        });
    }

    // ------------------------------------------------------------------ galeria

    fn gallery_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.heading("Galeria");
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Buscar item…"));
        ui.add_space(4.0);
        let filter = self.filter.to_lowercase();
        let mut chosen = None;
        let half = ui.available_height() * 0.62;
        egui::ScrollArea::vertical().id_salt("gallery").max_height(half).show(ui, |ui| {
            let mut cats: Vec<&str> = self.gallery.iter().map(|p| p.category).collect();
            cats.dedup();
            for cat in cats {
                let items: Vec<&Preset> = self
                    .gallery
                    .iter()
                    .filter(|p| p.category == cat && (filter.is_empty() || p.name.to_lowercase().contains(&filter) || p.group.to_lowercase().contains(&filter)))
                    .collect();
                if items.is_empty() {
                    continue;
                }
                egui::CollapsingHeader::new(egui::RichText::new(cat).strong())
                    .default_open(cat == "Cozinha" || !filter.is_empty())
                    .show(ui, |ui| {
                        let mut groups: Vec<&str> = items.iter().map(|p| p.group).collect();
                        groups.dedup();
                        for g in groups {
                            ui.label(egui::RichText::new(g).small().color(Color32::GRAY));
                            for p in items.iter().filter(|p| p.group == g) {
                                let preview = (p.make)();
                                let resp = ui
                                    .add(egui::Button::new(format!("+ {}", p.name)).frame(false))
                                    .on_hover_text(format!("{} · {} × {} × {} mm · clique para inserir", preview.kind.label(), num(preview.w), num(preview.h), num(preview.d)));
                                if resp.clicked() {
                                    chosen = Some(p.make);
                                }
                            }
                        }
                    });
            }
        });
        if let Some(make) = chosen {
            self.insert(make);
        }
        ui.separator();
        ui.strong("Itens do ambiente");
        egui::ScrollArea::vertical().id_salt("items").show(ui, |ui| {
            if self.project.modules.is_empty() {
                ui.label(egui::RichText::new("Nenhum módulo ainda.").italics().color(Color32::GRAY));
            }
            for (i, m) in self.project.modules.iter().enumerate() {
                let text = format!("{}. {}  ({} mm)", i + 1, m.name, num(m.w));
                if ui.selectable_label(self.selected == Some(i), text).clicked() {
                    self.selected = Some(i);
                }
            }
        });
    }

    // ------------------------------------------------------------------ propriedades

    fn props_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| match self.selected {
            Some(i) => self.module_props(ui, i),
            None => self.room_props(ui),
        });
    }

    fn room_props(&mut self, ui: &mut egui::Ui) {
        ui.heading("Projeto");
        egui::Grid::new("proj").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Nome");
            ui.text_edit_singleline(&mut self.project.name);
            ui.end_row();
            ui.label("Cliente");
            ui.text_edit_singleline(&mut self.project.client);
            ui.end_row();
        });
        ui.add_space(8.0);
        ui.heading("Ambiente");
        let r = &mut self.project.room;
        egui::Grid::new("room").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            mm_row(ui, "Largura", &mut r.width, 500.0..=20000.0);
            mm_row(ui, "Profundidade", &mut r.depth, 500.0..=20000.0);
            mm_row(ui, "Pé-direito", &mut r.height, 2000.0..=6000.0);
            mm_row(ui, "Espessura das paredes", &mut r.wall, 50.0..=500.0);
        });
        ui.checkbox(&mut r.show_walls, "Mostrar paredes");
        ui.checkbox(&mut r.front_wall, "Parede da frente");
        ui.add_space(8.0);
        match &self.project.geoarq {
            Some(p) => {
                ui.label(format!("Ambiente do GeoArq: {}", p.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()));
            }
            None => {
                if ui.button("Importar ambiente do GeoArq…").clicked() {
                    self.import_geoarq();
                }
            }
        }
        ui.add_space(12.0);
        ui.label(egui::RichText::new("Selecione um módulo no 3D ou em “Itens do ambiente” para editar as medidas, a construção e os materiais.").color(Color32::GRAY));
    }

    fn module_props(&mut self, ui: &mut egui::Ui, i: usize) {
        let names: Vec<String> = self.project.catalog.materials.iter().map(|m| m.name.clone()).collect();
        let cat = self.project.catalog.clone();
        let m = &mut self.project.modules[i];
        ui.heading(format!("{}. {}", i + 1, m.kind.label()));
        ui.text_edit_singleline(&mut m.name);
        ui.add_space(6.0);
        egui::CollapsingHeader::new(egui::RichText::new("Dimensões").strong()).default_open(true).show(ui, |ui| {
            egui::Grid::new("dims").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                mm_row(ui, "Largura (L)", &mut m.w, 150.0..=3000.0);
                mm_row(ui, "Altura (A)", &mut m.h, 150.0..=2750.0);
                mm_row(ui, "Profundidade (P)", &mut m.d, 150.0..=1000.0);
            });
        });
        egui::CollapsingHeader::new(egui::RichText::new("Posição").strong()).default_open(true).show(ui, |ui| {
            egui::Grid::new("pos").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                mm_row(ui, "X", &mut m.pos[0], -20000.0..=20000.0);
                mm_row(ui, "Y", &mut m.pos[1], -20000.0..=20000.0);
                mm_row(ui, "Elevação do piso", &mut m.pos[2], 0.0..=3000.0);
                ui.label("Rotação");
                egui::ComboBox::from_id_salt("rot").selected_text(format!("{}°", m.rot as u32 * 90)).show_ui(ui, |ui| {
                    for r in 0..4u8 {
                        ui.selectable_value(&mut m.rot, r, format!("{}°", r as u32 * 90));
                    }
                });
                ui.end_row();
            });
        });
        egui::CollapsingHeader::new(egui::RichText::new("Construção").strong()).default_open(true).show(ui, |ui| {
            egui::Grid::new("build").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                count_row(ui, "Portas", &mut m.doors, 0..=6);
                count_row(ui, "Gavetas", &mut m.drawers, 0..=8);
                count_row(ui, "Prateleiras", &mut m.shelves, 0..=12);
                count_row(ui, "Divisórias", &mut m.dividers, 0..=5);
                mm_row(ui, "Rodapé", &mut m.kick, 0.0..=250.0);
                ui.label("Cabideiro");
                ui.checkbox(&mut m.hanger, "");
                ui.end_row();
            });
        });
        egui::CollapsingHeader::new(egui::RichText::new("Materiais").strong()).default_open(true).show(ui, |ui| {
            egui::Grid::new("mats").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                for (label, slot) in [("Caixa", &mut m.box_mat), ("Frentes", &mut m.front_mat), ("Fundo", &mut m.back_mat)] {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(label).width(170.0).selected_text(names.get(*slot).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                        for (k, n) in names.iter().enumerate() {
                            ui.selectable_value(slot, k, n);
                        }
                    });
                    ui.end_row();
                }
            });
        });
        m.sanitize();
        let (boards, hw) = m.build(&cat);
        egui::CollapsingHeader::new(egui::RichText::new(format!("Peças ({})", boards.len())).strong()).default_open(false).show(ui, |ui| {
            egui::Grid::new("parts").striped(true).num_columns(3).show(ui, |ui| {
                for b in &boards {
                    ui.label(&b.name);
                    ui.label(format!("{} × {} × {}", num(b.length), num(b.width), num(b.thickness)));
                    ui.label(if b.edge_m() > 0.0 { format!("fita {} m", num(b.edge_m())) } else { String::new() });
                    ui.end_row();
                }
            });
        });
        egui::CollapsingHeader::new(egui::RichText::new("Ferragens").strong()).default_open(false).show(ui, |ui| {
            for (k, q) in &hw {
                ui.label(format!("{} {} — {}", num(*q), k.unit(), k.label()));
            }
        });
        // Custo do módulo (material por área, sem perda de chapa).
        let area_cost: f64 = boards
            .iter()
            .map(|b| {
                let mt = cat.material(b.material);
                b.area_m2() / (mt.sheet[0] * mt.sheet[1] / 1e6) * mt.sheet_price + b.edge_m() * mt.edge_price
            })
            .sum::<f64>()
            + hw.iter().map(|(k, q)| q * cat.hardware_price(*k)).sum::<f64>();
        ui.add_space(6.0);
        ui.label(format!("Material estimado deste módulo: {}", brl(area_cost)));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Duplicar").clicked() {
                self.duplicate_selected();
            }
            if ui.button("Girar").clicked() {
                self.rotate_selected();
            }
            if ui.button("Excluir").clicked() {
                self.delete_selected();
            }
        });
    }

    // ------------------------------------------------------------------ janelas

    fn catalog_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_catalog;
        egui::Window::new("Materiais, ferragens e preços").open(&mut open).default_width(820.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Chapas");
                let used: Vec<usize> = self.project.modules.iter().flat_map(|m| [m.box_mat, m.front_mat, m.back_mat]).collect();
                let mut remove = None;
                let cat = &mut self.project.catalog;
                let several = cat.materials.len() > 1;
                egui::Grid::new("materials").striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
                    for h in ["Material", "Esp. (mm)", "Chapa C × L (mm)", "Preço da chapa", "Fita (R$/m)", "Veio", "Cor", ""] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (k, m) in cat.materials.iter_mut().enumerate() {
                        ui.add_sized([210.0, 22.0], egui::TextEdit::singleline(&mut m.name));
                        ui.add(egui::DragValue::new(&mut m.thickness).range(3.0..=50.0).speed(0.5));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut m.sheet[0]).range(500.0..=4000.0));
                            ui.label("×");
                            ui.add(egui::DragValue::new(&mut m.sheet[1]).range(500.0..=3000.0));
                        });
                        ui.add(egui::DragValue::new(&mut m.sheet_price).range(0.0..=10000.0).speed(1.0).prefix("R$ "));
                        ui.add(egui::DragValue::new(&mut m.edge_price).range(0.0..=100.0).speed(0.1).prefix("R$ "));
                        ui.checkbox(&mut m.grain, "");
                        // Edita uma cópia: o seletor arredonda a cor ao desenhar e marcaria o projeto
                        // como alterado só por abrir a janela.
                        let mut color = m.color;
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            m.color = color;
                        }
                        let in_use = used.contains(&k);
                        if ui.add_enabled(!in_use && several, egui::Button::new("Remover")).on_disabled_hover_text("Em uso por algum módulo").clicked() {
                            remove = Some(k);
                        }
                        ui.end_row();
                    }
                });
                if let Some(k) = remove {
                    cat.materials.remove(k);
                    for m in &mut self.project.modules {
                        for slot in [&mut m.box_mat, &mut m.front_mat, &mut m.back_mat] {
                            if *slot > k {
                                *slot -= 1;
                            }
                        }
                    }
                }
                let cat = &mut self.project.catalog;
                if ui.button("+ Novo material").clicked() {
                    let mut m: Material = cat.materials[0].clone();
                    m.name = "Novo material".into();
                    cat.materials.push(m);
                }
                ui.add_space(10.0);
                ui.heading("Ferragens");
                egui::Grid::new("hw").striped(true).show(ui, |ui| {
                    for kind in HardwareKind::ALL {
                        if let Some(h) = cat.hardware.iter_mut().find(|h| h.kind == kind) {
                            ui.label(kind.label());
                            ui.add(egui::DragValue::new(&mut h.price).range(0.0..=5000.0).speed(0.1).prefix("R$ ").suffix(format!(" / {}", kind.unit())));
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(10.0);
                ui.heading("Corte e orçamento");
                egui::Grid::new("cut").spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Espessura da serra");
                    ui.add(egui::DragValue::new(&mut cat.cut.kerf).range(0.0..=10.0).speed(0.1).suffix(" mm"));
                    ui.end_row();
                    ui.label("Refilo da chapa (cada borda)");
                    ui.add(egui::DragValue::new(&mut cat.cut.trim).range(0.0..=50.0).speed(0.5).suffix(" mm"));
                    ui.end_row();
                    budget_settings(ui, &mut cat.budget);
                });
            });
        });
        self.show_catalog = open;
    }

    fn parts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_parts;
        let mut export = false;
        egui::Window::new("Lista de peças").open(&mut open).default_width(760.0).default_height(520.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} peças · {} m de fita", self.takeoff.boards.len(), num(self.takeoff.boards.iter().map(|(_, b)| b.edge_m()).sum())));
                if ui.button("Exportar CSV…").clicked() {
                    export = true;
                }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("all_parts").striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                    for h in ["Mód.", "Peça", "Compr.", "Larg.", "Esp.", "Material", "Fita (m)"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (mi, b) in &self.takeoff.boards {
                        ui.label(format!("{}", mi + 1));
                        ui.label(&b.name);
                        ui.label(num(b.length));
                        ui.label(num(b.width));
                        ui.label(num(b.thickness));
                        ui.label(&self.project.catalog.material(b.material).name);
                        ui.label(num(b.edge_m()));
                        ui.end_row();
                    }
                });
            });
        });
        self.show_parts = open;
        if export {
            self.export("csv");
        }
    }

    fn help_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.show_shortcuts;
        egui::Window::new("Atalhos de teclado").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("keys").striped(true).show(ui, |ui| {
                for (k, d) in [
                    ("Botão do meio / arrastar o fundo", "Orbitar"),
                    ("Shift + arrastar", "Pan"),
                    ("Roda / pinça", "Zoom no cursor"),
                    ("Shift+Z", "Zoom extensões"),
                    ("0 · 1 · 2 · 3 · 4 · 5", "Iso · Topo · Frente · Fundos · Esquerda · Direita"),
                    ("Arrastar um módulo", "Mover (encaixa nas paredes e nos vizinhos)"),
                    ("R", "Girar 90°"),
                    ("Setas (Shift)", "Ajustar 10 mm (100 mm)"),
                    (&kb("⌘D"), "Duplicar"),
                    ("Delete / Backspace", "Excluir"),
                    ("F", "Ocultar portas e frentes"),
                    ("Esc", "Selecionar nenhum"),
                    (&kb("⌘Z / ⇧⌘Z"), "Desfazer / refazer"),
                    (&kb("⌘N / ⌘O / ⌘S"), "Novo / abrir / salvar"),
                ] {
                    ui.strong(k);
                    ui.label(d);
                    ui.end_row();
                }
            });
        });
        self.show_shortcuts = open;
        let mut open = self.show_about;
        egui::Window::new("Sobre o Planejei").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.heading(format!("Planejei {}", env!("CARGO_PKG_VERSION")));
            ui.label("Projeto de móveis planejados com lista de peças, plano de corte e orçamento.");
            ui.label("Da família do GeoArq: mesmo motor 3D, mesma navegação.");
            ui.separator();
            // Avisos legais exigidos pela GPL (seção 0, "Appropriate Legal Notices").
            ui.label("Copyright (C) 2026 Lucas Bernardo");
            ui.label("Software livre: você pode redistribuí-lo e modificá-lo nos termos da Licença Pública Geral GNU, versão 3 ou posterior.");
            ui.label("Distribuído SEM NENHUMA GARANTIA, nem mesmo a de adequação a um uso específico.");
            ui.hyperlink_to("GNU GPL versão 3", "https://www.gnu.org/licenses/gpl-3.0.html");
        });
        self.show_about = open;
    }

    // ------------------------------------------------------------------ teclado

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = ev else { continue };
            if modifiers.command {
                match key {
                    Key::Z if modifiers.shift => self.redo(),
                    Key::Z => self.undo(),
                    Key::Y => self.redo(),
                    Key::N => self.queued.push(FileCmd::New),
                    Key::O => self.queued.push(FileCmd::Open),
                    Key::S if modifiers.shift => self.queued.push(FileCmd::SaveAs),
                    Key::S => self.queued.push(FileCmd::Save),
                    Key::D => self.duplicate_selected(),
                    _ => {}
                }
                continue;
            }
            let step = if modifiers.shift { 100.0 } else { 10.0 };
            match key {
                Key::Escape => self.selected = None,
                Key::Delete | Key::Backspace => self.delete_selected(),
                Key::R => self.rotate_selected(),
                Key::F => self.hide_fronts = !self.hide_fronts,
                Key::Z if modifiers.shift => self.needs_fit = true,
                Key::ArrowLeft => self.nudge(-step, 0.0),
                Key::ArrowRight => self.nudge(step, 0.0),
                Key::ArrowUp => self.nudge(0.0, step),
                Key::ArrowDown => self.nudge(0.0, -step),
                Key::Num0 => self.set_view(StandardView::Iso),
                Key::Num1 => self.set_view(StandardView::Top),
                Key::Num2 => self.set_view(StandardView::Front),
                Key::Num3 => self.set_view(StandardView::Back),
                Key::Num4 => self.set_view(StandardView::Left),
                Key::Num5 => self.set_view(StandardView::Right),
                _ => {}
            }
        }
    }

    fn set_view(&mut self, v: StandardView) {
        self.camera.set_view(v);
        self.needs_fit = true;
        self.tab = Tab::View3d;
    }

    // ------------------------------------------------------------------ 3D

    fn scene_bounds(&self) -> (Vec3, Vec3) {
        let r = &self.project.room;
        let mut lo = DVec3::ZERO;
        let mut hi = DVec3::new(r.width, r.depth, r.height) / 1000.0;
        for m in &self.project.modules {
            let (a, b) = m.bounds();
            lo = lo.min(a / 1000.0);
            hi = hi.max(b / 1000.0);
        }
        for mesh in &self.geoarq {
            for (_, v) in &mesh.verts {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
        (lo.as_vec3(), hi.as_vec3())
    }

    fn scene_radius(&self) -> f32 {
        let (a, b) = self.scene_bounds();
        ((b - a).length() * 0.5).max(1.0)
    }

    fn texture_index(&self, name: &str) -> Option<u32> {
        self.textures.iter().position(|t| t.name == name).map(|i| i as u32)
    }

    fn build_solids(&self) -> Vec<Solid> {
        let mut out = Vec::new();
        let r = &self.project.room;
        let mm = |v: DVec3| v / 1000.0;
        let solid = |a: DVec3, b: DVec3, color: [f32; 3], texture: Option<u32>, module: Option<usize>| Solid {
            mesh: Mesh::cuboid(mm(a), mm(b), None),
            color,
            texture,
            module,
        };
        if r.show_walls || self.geoarq.is_empty() {
            out.push(solid(DVec3::new(0.0, 0.0, -20.0), DVec3::new(r.width, r.depth, 0.0), [0.75, 0.6, 0.45], self.texture_index(FLOOR_TEXTURE), None));
        }
        if r.show_walls {
            let (w, d, h, t) = (r.width, r.depth, r.height, r.wall);
            let wall = [0.93, 0.92, 0.89];
            let [back, left, right, front] = self.walls_facing_camera();
            if back {
                out.push(solid(DVec3::new(-t, d, 0.0), DVec3::new(w + t, d + t, h), wall, None, None));
            }
            if left {
                out.push(solid(DVec3::new(-t, 0.0, 0.0), DVec3::new(0.0, d, h), wall, None, None));
            }
            if right {
                out.push(solid(DVec3::new(w, 0.0, 0.0), DVec3::new(w + t, d, h), wall, None, None));
            }
            if r.front_wall && front {
                out.push(solid(DVec3::new(-t, -t, 0.0), DVec3::new(w + t, 0.0, h), wall, None, None));
            }
        }
        let cat = &self.project.catalog;
        for (i, m) in self.project.modules.iter().enumerate() {
            let (boards, _) = m.build(cat);
            for b in &boards {
                if self.hide_fronts && b.role.is_front() {
                    continue;
                }
                let mat = cat.material(b.material);
                let tex = mat.texture.as_deref().and_then(|n| self.texture_index(n));
                let (a, c) = m.world_box(b.min, b.max);
                out.push(solid(a, c, mat.color, tex, Some(i)));
            }
            if self.hide_fronts {
                if let Some(z) = m.hanger_z(cat) {
                    let t = cat.material(m.box_mat).thickness;
                    let y = m.d * 0.5;
                    let (a, c) = m.world_box(DVec3::new(t, y - 8.0, z - 15.0), DVec3::new(m.w - t, y + 8.0, z + 15.0));
                    out.push(solid(a, c, [0.75, 0.76, 0.78], None, Some(i)));
                }
                continue;
            }
            // Puxadores (só para a apresentação).
            for b in boards.iter().filter(|b| b.role.is_front()) {
                let metal = [0.55, 0.56, 0.58];
                let (lo, hi) = if b.role == Role::DrawerFront {
                    let cx = (b.min.x + b.max.x) / 2.0;
                    let z = b.max.z - 45.0;
                    (DVec3::new(cx - 64.0, -22.0, z - 6.0), DVec3::new(cx + 64.0, 0.0, z + 6.0))
                } else {
                    let left_side = (b.min.x + b.max.x) / 2.0 > m.w / 2.0;
                    let x = if left_side { b.min.x + 40.0 } else { b.max.x - 40.0 };
                    let zc = if m.kind.hung() {
                        b.min.z + 100.0
                    } else if b.max.z - b.min.z > 1200.0 {
                        (1050.0 - m.pos[2]).clamp(b.min.z + 100.0, b.max.z - 100.0)
                    } else {
                        b.max.z - 100.0
                    };
                    (DVec3::new(x - 6.0, -22.0, zc - 64.0), DVec3::new(x + 6.0, 0.0, zc + 64.0))
                };
                let (a, c) = m.world_box(lo, hi);
                out.push(solid(a, c, metal, None, Some(i)));
            }
        }
        out
    }

    /// Paredes visíveis (fundo, esquerda, direita, frente): some a que fica entre a câmera e o
    /// ambiente, como no Promob.
    fn walls_facing_camera(&self) -> [bool; 4] {
        let e = self.camera.eye();
        let r = &self.project.room;
        let (w, d) = ((r.width / 1000.0) as f32, (r.depth / 1000.0) as f32);
        // Em vista de topo todas aparecem.
        if self.camera.forward().z < -0.95 {
            return [true; 4];
        }
        [e.y < d, e.x > 0.0, e.x < w, e.y > 0.0]
    }

    fn sync_scene(&mut self) {
        let key = (self.project.clone(), self.selected, self.hide_fronts, self.geoarq.len(), self.walls_facing_camera());
        if self.scene_key.as_ref() == Some(&key) {
            return;
        }
        let solids = self.build_solids();
        let mut parts: Vec<renderer::Part> = self
            .geoarq
            .iter()
            .map(|mesh| renderer::Part { mesh, sel: None, all_selected: false, faded: false, material: None, texture: None, uv_xf: DAffine3::IDENTITY })
            .collect();
        parts.extend(solids.iter().map(|s| renderer::Part {
            mesh: &s.mesh,
            sel: None,
            all_selected: s.module.is_some() && s.module == self.selected,
            faded: false,
            material: Some(s.color),
            texture: s.texture,
            uv_xf: DAffine3::IDENTITY,
        }));
        self.renderer.upload(&parts, &self.textures);
        self.scene_key = Some(key);
    }

    fn view_proj(&self) -> Mat4 {
        self.camera.proj(self.aspect, self.scene_radius()) * self.camera.view()
    }

    fn globals(&self, size_px: [u32; 2], ppp: f32) -> Globals {
        let cam = &self.camera;
        let light = (cam.to_eye() * 0.55 + Vec3::Z * 0.75 - cam.right() * 0.35).normalize();
        let bias = cam.depth_bias(self.scene_radius());
        let v4 = |v: Vec3, w: f32| [v.x, v.y, v.z, w];
        Globals {
            view_proj: self.view_proj().to_cols_array_2d(),
            eye: v4(cam.eye(), cam.forward().z),
            target: v4(cam.target, 0.0),
            viewport: [size_px[0] as f32, size_px[1] as f32, ppp, if cam.projection == Projection::Parallel { 1.0 } else { 0.0 }],
            light_dir: v4(light, 0.0),
            bias: [bias[0], bias[1], 0.0, 0.0],
            back_color: srgb_to_linear4([0.62, 0.69, 0.77, 1.0]),
            bg_top: srgb_to_linear4([0.86, 0.90, 0.95, 1.0]),
            bg_bottom: srgb_to_linear4([0.985, 0.985, 0.975, 1.0]),
            shading: [1.0, 1.0, 0.0, 0.0],
            clip: [0.0; 4],
        }
    }

    fn zoom_extents(&mut self) {
        let (a, b) = self.scene_bounds();
        self.camera.zoom_extents((a + b) * 0.5, (b - a).length() * 0.5, self.aspect);
    }

    /// Módulo sob o cursor (o mais próximo da câmera) e o ponto atingido (m).
    fn pick(&self, ndc: Vec2) -> Option<(usize, Vec3)> {
        let ray = self.camera.ray(ndc, self.aspect);
        let mut best: Option<(f32, usize)> = None;
        for (i, m) in self.project.modules.iter().enumerate() {
            let (a, b) = m.bounds();
            if let Some(t) = ray_box(ray.origin, ray.dir, (a / 1000.0).as_vec3(), (b / 1000.0).as_vec3())
                && best.is_none_or(|(bt, _)| t < bt)
            {
                best = Some((t, i));
            }
        }
        best.map(|(t, i)| (i, ray.origin + ray.dir * t))
    }

    fn viewport(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let rect = ui.max_rect();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        self.aspect = (rect.width() / rect.height().max(1.0)).max(0.01);
        self.viewport_h = rect.height();
        if self.needs_fit && rect.width() > 10.0 {
            self.zoom_extents();
            self.needs_fit = false;
        }
        let to_ndc = |p: egui::Pos2| Vec2::new((p.x - rect.center().x) / (rect.width() * 0.5), -(p.y - rect.center().y) / (rect.height() * 0.5));
        let ndc = resp.hover_pos().map(to_ndc).unwrap_or(Vec2::ZERO);
        let shift = ui.input(|i| i.modifiers.shift);

        // Clique: seleciona. Arraste num módulo: move. Arraste no fundo: orbita (Shift: pan).
        if resp.clicked() {
            self.selected = resp.interact_pointer_pos().and_then(|p| self.pick(to_ndc(p))).map(|(i, _)| i);
        }
        if resp.drag_started_by(PointerButton::Primary) && !shift
            && let Some(p) = resp.interact_pointer_pos()
            && let Some((i, hit)) = self.pick(to_ndc(p))
        {
            let (a, b) = self.project.modules[i].bounds();
            let c = (a + b) / 2.0;
            self.selected = Some(i);
            self.drag = Some(Drag { idx: i, offset: [c.x - hit.x as f64 * 1000.0, c.y - hit.y as f64 * 1000.0], plane_z: hit.z });
        }
        if !resp.dragged() {
            self.drag = None;
        }
        let d = resp.drag_delta();
        let d = Vec2::new(d.x, d.y);
        if let Some(drag) = &self.drag {
            if let Some(p) = resp.interact_pointer_pos()
                && let Some(hit) = self.camera.ray(to_ndc(p), self.aspect).hit_plane(Vec3::Z, drag.plane_z)
                && drag.idx < self.project.modules.len()
            {
                let center = [hit.x as f64 * 1000.0 + drag.offset[0], hit.y as f64 * 1000.0 + drag.offset[1]];
                let snapped = self.project.snap(drag.idx, center);
                self.project.modules[drag.idx] = snapped;
            }
        } else if resp.dragged_by(PointerButton::Middle) || resp.dragged_by(PointerButton::Primary) {
            if shift {
                self.camera.pan(d, self.viewport_h);
            } else {
                self.camera.orbit(d);
            }
        } else if resp.dragged_by(PointerButton::Secondary) {
            self.camera.pan(d, self.viewport_h);
        }
        if resp.hovered() {
            for ev in ui.input(|i| i.events.clone()) {
                if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = ev {
                    if modifiers.command || modifiers.ctrl {
                        continue;
                    }
                    let k = match unit {
                        egui::MouseWheelUnit::Point => 0.004,
                        egui::MouseWheelUnit::Line => 0.15,
                        egui::MouseWheelUnit::Page => 1.0,
                    };
                    let amount = if delta.y != 0.0 { delta.y } else { delta.x };
                    self.camera.zoom_at((-amount * k).exp(), ndc, self.aspect);
                }
            }
            let zoom = ui.input(|i| i.zoom_delta());
            if (zoom - 1.0).abs() > 1e-4 {
                self.camera.zoom_at(1.0 / zoom, ndc, self.aspect);
            }
            let over = resp.hover_pos().and_then(|p| self.pick(to_ndc(p))).is_some();
            ui.ctx().set_cursor_icon(if self.drag.is_some() {
                CursorIcon::Grabbing
            } else if resp.dragged() {
                CursorIcon::AllScroll
            } else if over {
                CursorIcon::Grab
            } else {
                CursorIcon::Default
            });
        }

        self.sync_scene();
        let Some(rs) = frame.wgpu_render_state() else { return };
        self.renderer.set_overlay(&[]);
        let size_px = [(rect.width() * ppp).round() as u32, (rect.height() * ppp).round() as u32];
        let globals = self.globals(size_px, ppp);
        let tex = {
            let mut egui_renderer = rs.renderer.write();
            self.renderer.render(&mut egui_renderer, size_px, &globals, FaceStyle::ShadedTextured, false, false)
        };
        let painter = ui.painter_at(rect);
        painter.image(tex, rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);

        // Cota do módulo selecionado.
        if let Some(i) = self.selected {
            let m = &self.project.modules[i];
            let (a, b) = m.bounds();
            let top = DVec3::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0, b.z + 60.0) / 1000.0;
            if let Some(p) = self.to_screen(top.as_vec3(), rect) {
                let text = format!("{}  ·  {} × {} × {} mm", m.name, num(m.w), num(m.h), num(m.d));
                let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::WHITE);
                let r = Rect::from_center_size(p, galley.size() + egui::vec2(14.0, 8.0));
                painter.rect_filled(r, 4.0, Color32::from_rgba_unmultiplied(25, 70, 200, 225));
                painter.galley(r.min + egui::vec2(7.0, 4.0), galley, Color32::WHITE);
            }
        }
        if self.project.modules.is_empty() {
            painter.text(
                rect.center_top() + egui::vec2(0.0, 30.0),
                egui::Align2::CENTER_TOP,
                "Ambiente vazio — escolha um módulo na Galeria (ou Arquivo › Abrir exemplo)",
                egui::FontId::proportional(16.0),
                Color32::from_gray(90),
            );
        }
    }

    fn to_screen(&self, p: Vec3, rect: Rect) -> Option<egui::Pos2> {
        let c = self.view_proj() * Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 1e-6 {
            return None;
        }
        let (x, y) = (c.x / c.w, c.y / c.w);
        Some(egui::pos2(rect.center().x + x * rect.width() * 0.5, rect.center().y - y * rect.height() * 0.5))
    }

    // ------------------------------------------------------------------ plano de corte

    fn cut_view(&mut self, ui: &mut egui::Ui) {
        let mut export = None;
        egui::Frame::new().inner_margin(12.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Plano de corte");
                ui.label(format!("· {} chapas · serra {} mm · refilo {} mm", self.plan.sheets.len(), num(self.project.catalog.cut.kerf), num(self.project.catalog.cut.trim)));
                if ui.button("Exportar SVG…").clicked() {
                    export = Some("svg");
                }
                if ui.button("Lista de peças (CSV)…").clicked() {
                    export = Some("csv");
                }
                if ui.button("Parâmetros…").clicked() {
                    self.show_catalog = true;
                }
            });
            for mi in self.plan.materials() {
                let sheets: Vec<&crate::cutting::Sheet> = self.plan.sheets.iter().filter(|s| s.material == mi).collect();
                let usage = if sheets.is_empty() { 0.0 } else { sheets.iter().map(|s| s.usage).sum::<f64>() / sheets.len() as f64 };
                let n = self.plan.pieces.iter().filter(|p| p.material == mi).count();
                ui.label(format!(
                    "{}: {} chapa(s), {} peças, {} m² de peças, aproveitamento médio {:.0}%",
                    self.project.catalog.material(mi).name,
                    sheets.len(),
                    n,
                    num(self.plan.piece_area_m2(mi)),
                    usage * 100.0
                ));
            }
            for &i in &self.plan.too_big {
                let p = &self.plan.pieces[i];
                ui.colored_label(Color32::from_rgb(190, 40, 40), format!("{} (módulo {}) — {} × {} mm não cabe na chapa", p.label, p.module + 1, num(p.length), num(p.width)));
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let trim = self.project.catalog.cut.trim;
                for (si, s) in self.plan.sheets.iter().enumerate() {
                    let mat = self.project.catalog.material(s.material);
                    ui.strong(format!("Chapa {} — {} ({} × {} mm) — aproveitamento {:.0}%", si + 1, mat.name, num(mat.sheet[0]), num(mat.sheet[1]), s.usage * 100.0));
                    let w = ui.available_width().min(1100.0);
                    let scale = w / mat.sheet[0] as f32;
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, mat.sheet[1] as f32 * scale), Sense::hover());
                    let painter = ui.painter_at(rect.expand(1.0));
                    painter.rect_filled(rect, 0.0, Color32::from_gray(232));
                    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(80)), egui::StrokeKind::Inside);
                    let hover = ui.input(|i| i.pointer.hover_pos());
                    for pl in &s.placed {
                        let piece = &self.plan.pieces[pl.piece];
                        let c = reports::module_color(piece.module);
                        let r = Rect::from_min_size(
                            rect.min + egui::vec2((trim + pl.x) as f32 * scale, (trim + pl.y) as f32 * scale),
                            egui::vec2(pl.w as f32 * scale, pl.h as f32 * scale),
                        );
                        painter.rect_filled(r, 0.0, Color32::from_rgb(c[0], c[1], c[2]));
                        painter.rect_stroke(r, 0.0, Stroke::new(0.8, Color32::from_gray(60)), egui::StrokeKind::Inside);
                        if r.width() > 46.0 && r.height() > 22.0 {
                            let fs = (r.height() * 0.22).clamp(8.0, 12.0);
                            painter.text(r.center() - egui::vec2(0.0, fs * 0.6), egui::Align2::CENTER_CENTER, format!("{} {}", piece.module + 1, piece.label), egui::FontId::proportional(fs), Color32::from_gray(30));
                            painter.text(r.center() + egui::vec2(0.0, fs * 0.6), egui::Align2::CENTER_CENTER, format!("{} × {}", num(piece.length), num(piece.width)), egui::FontId::proportional(fs), Color32::from_gray(60));
                        }
                        if hover.is_some_and(|h| r.contains(h)) {
                            painter.rect_stroke(r, 0.0, Stroke::new(2.0, Color32::from_rgb(25, 70, 200)), egui::StrokeKind::Inside);
                            egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), ui.scope_id().with("piece_tip"), egui::PopupAnchor::Pointer).show(|ui| {
                                ui.strong(&piece.label);
                                ui.label(format!("Módulo {}: {}", piece.module + 1, self.project.modules.get(piece.module).map(|m| m.name.as_str()).unwrap_or("")));
                                ui.label(format!("{} × {} mm{}", num(piece.length), num(piece.width), if pl.rotated { " (girada)" } else { "" }));
                            });
                        }
                    }
                    ui.add_space(14.0);
                }
            });
        });
        if let Some(k) = export {
            self.export(k);
        }
    }

    // ------------------------------------------------------------------ orçamento

    fn budget_view(&mut self, ui: &mut egui::Ui) {
        let mut export = false;
        egui::Frame::new().inner_margin(12.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Orçamento");
                    if ui.button("Exportar proposta (HTML/PDF)…").clicked() {
                        export = true;
                    }
                    if ui.button("Preços…").clicked() {
                        self.show_catalog = true;
                    }
                });
                ui.label(format!("{} · cliente: {}", self.project.name, if self.project.client.is_empty() { "—" } else { &self.project.client }));
                ui.add_space(8.0);
                let b = &self.budget;
                egui::Grid::new("budget").striped(true).spacing([18.0, 4.0]).min_col_width(60.0).show(ui, |ui| {
                    for h in ["Item", "Qtd.", "Un.", "Preço un.", "Total"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    let mut group = "";
                    for l in &b.lines {
                        if l.group != group {
                            group = l.group;
                            ui.label(egui::RichText::new(group).strong().color(Color32::from_rgb(25, 70, 200)));
                            ui.end_row();
                        }
                        ui.label(&l.desc);
                        ui.label(num(l.qty));
                        ui.label(l.unit);
                        ui.label(brl(l.unit_price));
                        ui.label(brl(l.total));
                        ui.end_row();
                    }
                    ui.separator();
                    ui.end_row();
                    let cat = &self.project.catalog;
                    for (label, v) in [
                        ("Materiais".to_string(), b.material),
                        (format!("Mão de obra ({}%)", num(cat.budget.labor_pct)), b.labor),
                        ("Custo".to_string(), b.cost),
                        (format!("Margem ({}%)", num(cat.budget.margin_pct)), b.margin),
                    ] {
                        ui.label(label);
                        ui.label("");
                        ui.label("");
                        ui.label("");
                        ui.label(brl(v));
                        ui.end_row();
                    }
                    ui.label(egui::RichText::new("Total").size(18.0).strong());
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.label(egui::RichText::new(brl(b.total)).size(18.0).strong());
                    ui.end_row();
                });
                ui.add_space(12.0);
                ui.strong("Parâmetros");
                egui::Grid::new("bset").spacing([8.0, 6.0]).show(ui, |ui| budget_settings(ui, &mut self.project.catalog.budget));
            });
        });
        if export {
            self.export("html");
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Fechar com alterações não salvas: pergunta antes.
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            // Captura automática (desenvolvimento) fecha sem perguntar.
            if std::env::var("PLANEJEI_SCREENSHOT").is_ok() || self.confirm_discard() {
                self.saved = self.project.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let before = self.project.clone();
        let undo_len = (self.undo.len(), self.redo.len());

        self.handle_keyboard(&ctx);
        self.refresh_derived();
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar")
            .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(6, 4)))
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.show_gallery {
            egui::Panel::left("gallery").default_size(250.0).show(ui, |ui| self.gallery_panel(ui));
        }
        if self.show_props {
            egui::Panel::right("props").default_size(300.0).show(ui, |ui| self.props_panel(ui));
        }
        match self.tab {
            Tab::View3d => {
                egui::CentralPanel::no_frame().show(ui, |ui| self.viewport(ui, frame));
            }
            Tab::Cut => {
                egui::CentralPanel::default().show(ui, |ui| self.cut_view(ui));
            }
            Tab::Budget => {
                egui::CentralPanel::default().show(ui, |ui| self.budget_view(ui));
            }
        }
        self.catalog_window(&ctx);
        self.parts_window(&ctx);
        self.help_windows(&ctx);
        self.run_file_cmds();

        // Desfazer: cada mudança vira um passo; arrastes e digitação contínuos se juntam num só.
        let history_moved = (self.undo.len(), self.redo.len()) != undo_len;
        let changed = self.project != before;
        if changed && !history_moved && self.derived_for.is_some() {
            if !self.merging {
                self.undo.push(before);
                self.redo.clear();
                if self.undo.len() > 200 {
                    self.undo.remove(0);
                }
            }
            self.notice = None;
        }
        self.merging = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
        if changed {
            self.refresh_derived();
        }

        let title = format!(
            "{}{} — Planejei",
            if self.dirty() { "• " } else { "" },
            self.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| self.project.name.clone())
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        if let Some(c) = &mut self.capture {
            c.tick(&ctx);
        }
    }
}

// ------------------------------------------------------------------ auxiliares

fn mm_row(ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.label(label);
    ui.add(egui::DragValue::new(v).range(range).speed(1.0).suffix(" mm").max_decimals(1));
    ui.end_row();
}

fn count_row(ui: &mut egui::Ui, label: &str, v: &mut u32, range: std::ops::RangeInclusive<u32>) {
    ui.label(label);
    ui.add(egui::DragValue::new(v).range(range).speed(0.05));
    ui.end_row();
}

fn budget_settings(ui: &mut egui::Ui, b: &mut crate::catalog::BudgetSettings) {
    ui.label("Mão de obra");
    ui.add(egui::DragValue::new(&mut b.labor_pct).range(0.0..=300.0).speed(0.5).suffix(" %"));
    ui.end_row();
    ui.label("Margem");
    ui.add(egui::DragValue::new(&mut b.margin_pct).range(0.0..=300.0).speed(0.5).suffix(" %"));
    ui.end_row();
    ui.label("Cobrar chapas");
    ui.horizontal(|ui| {
        ui.radio_value(&mut b.whole_sheets, true, "inteiras");
        ui.radio_value(&mut b.whole_sheets, false, "por m² usado + perda");
    });
    ui.end_row();
    if !b.whole_sheets {
        ui.label("Perda");
        ui.add(egui::DragValue::new(&mut b.loss_pct).range(0.0..=100.0).speed(0.5).suffix(" %"));
        ui.end_row();
    }
}

/// Distância ao longo do raio até a caixa (slab test), se atingir.
fn ray_box(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let inv = Vec3::ONE / d;
    let t0 = (lo - o) * inv;
    let t1 = (hi - o) * inv;
    let tmin = t0.min(t1).max_element();
    let tmax = t0.max(t1).min_element();
    (tmax >= tmin.max(0.0)).then_some(tmin.max(0.0))
}

fn kb(s: &str) -> String {
    let cmd = if cfg!(target_os = "macos") { "Cmd+" } else { "Ctrl+" };
    s.replace('⇧', "Shift+").replace('⌘', cmd)
}

fn open_path(p: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}
