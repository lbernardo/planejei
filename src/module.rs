// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Módulos paramétricos de marcenaria: a partir de poucos parâmetros (L × A × P, portas,
//! gavetas, prateleiras…) geram as peças (com medidas de corte e fita de borda) e as ferragens.
//!
//! Coordenadas locais em mm: x = largura (da esquerda para a direita), y = profundidade
//! (0 = frente, `d` = fundo), z = altura (0 = base). A frente do módulo olha para −y.

use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, HardwareKind, MAT_BACK, MAT_BOX, MAT_FRONT};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Kind {
    /// Balcão inferior com portas.
    BaseDoors,
    /// Balcão inferior gaveteiro.
    BaseDrawers,
    /// Aéreo (suspenso).
    Wall,
    /// Torre / paneleiro.
    Tower,
    /// Roupeiro (guarda-roupa).
    Wardrobe,
    /// Nicho / estante aberta.
    Shelf,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::BaseDoors => "Balcão com portas",
            Kind::BaseDrawers => "Balcão gaveteiro",
            Kind::Wall => "Aéreo",
            Kind::Tower => "Torre / paneleiro",
            Kind::Wardrobe => "Roupeiro",
            Kind::Shelf => "Nicho / estante",
        }
    }

    /// Fica suspenso na parede (não no piso).
    pub fn hung(self) -> bool {
        self == Kind::Wall
    }

    /// Tem travessas no lugar do tampo (recebe bancada por cima).
    pub fn has_rails(self) -> bool {
        matches!(self, Kind::BaseDoors | Kind::BaseDrawers)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Module {
    pub name: String,
    pub kind: Kind,
    /// Largura, altura e profundidade (mm).
    pub w: f64,
    pub h: f64,
    pub d: f64,
    /// Canto mínimo da projeção no piso (x, y) e altura da base (z), em mm no mundo.
    pub pos: [f64; 3],
    /// Rotação em quartos de volta (anti-horário). 0: frente para −y (encostado na parede do fundo).
    pub rot: u8,
    pub doors: u32,
    pub drawers: u32,
    pub shelves: u32,
    pub dividers: u32,
    /// Altura do rodapé (mm); 0 = sem rodapé.
    pub kick: f64,
    pub hanger: bool,
    pub box_mat: usize,
    pub front_mat: usize,
    pub back_mat: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Side,
    Bottom,
    Top,
    Rail,
    Back,
    Shelf,
    Divider,
    Kick,
    Door,
    DrawerFront,
    DrawerSide,
    DrawerBack,
    DrawerBottom,
}

impl Role {
    /// Peça de frente (porta/frente de gaveta), que pode ser ocultada no 3D.
    pub fn is_front(self) -> bool {
        matches!(self, Role::Door | Role::DrawerFront)
    }
}

/// Uma peça a cortar.
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    pub name: String,
    pub role: Role,
    /// Comprimento (maior medida, no sentido do veio), largura e espessura (mm).
    pub length: f64,
    pub width: f64,
    pub thickness: f64,
    pub material: usize,
    /// Fita nos lados do comprimento (2) e da largura (2).
    pub edges: [bool; 4],
    /// Caixa local (mm).
    pub min: DVec3,
    pub max: DVec3,
}

impl Board {
    /// Metros de fita de borda.
    pub fn edge_m(&self) -> f64 {
        let [l1, l2, w1, w2] = self.edges.map(|b| b as u8 as f64);
        (self.length * (l1 + l2) + self.width * (w1 + w2)) / 1000.0
    }

    pub fn area_m2(&self) -> f64 {
        self.length * self.width / 1e6
    }
}

/// Onde vai a fita de borda.
#[derive(Clone, Copy)]
enum Band {
    None,
    /// Na aresta da frente, que corre ao longo do eixo dado (0 = x, 2 = z).
    Front(usize),
    /// Nos quatro lados.
    All,
}

fn board(name: &str, role: Role, material: usize, min: DVec3, max: DVec3, band: Band) -> Board {
    let dims = (max - min).to_array();
    let thick_axis = (0..3).min_by(|a, b| dims[*a].total_cmp(&dims[*b])).unwrap();
    let others: Vec<usize> = (0..3).filter(|a| *a != thick_axis).collect();
    let (la, wa) = if dims[others[0]] >= dims[others[1]] { (others[0], others[1]) } else { (others[1], others[0]) };
    let edges = match band {
        Band::None => [false; 4],
        Band::All => [true; 4],
        Band::Front(axis) if axis == la => [true, false, false, false],
        Band::Front(axis) if axis == wa => [false, false, true, false],
        Band::Front(_) => [false; 4],
    };
    Board {
        name: name.into(),
        role,
        length: round1(dims[la]),
        width: round1(dims[wa]),
        thickness: round1(dims[thick_axis]),
        material,
        edges,
        min,
        max,
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// Folga das frentes (mm), entre peças e nas bordas.
pub const GAP: f64 = 3.0;
/// Recuo do fundo em relação à traseira das laterais (mm).
const BACK_INSET: f64 = 10.0;
/// Altura de cada frente de gaveta quando o módulo também tem portas (mm).
const DRAWER_FRONT_H: f64 = 170.0;
/// Espaço de cada corrediça (mm).
const SLIDE_GAP: f64 = 13.0;

pub fn hinges_for(door_h: f64) -> u32 {
    match door_h {
        h if h <= 900.0 => 2,
        h if h <= 1600.0 => 3,
        h if h <= 2000.0 => 4,
        _ => 5,
    }
}

impl Module {
    pub fn new(kind: Kind) -> Self {
        let base = Module {
            name: kind.label().into(),
            kind,
            w: 800.0,
            h: 800.0,
            d: 550.0,
            pos: [0.0; 3],
            rot: 0,
            doors: 2,
            drawers: 0,
            shelves: 1,
            dividers: 0,
            kick: 100.0,
            hanger: false,
            box_mat: MAT_BOX,
            front_mat: MAT_FRONT,
            back_mat: MAT_BACK,
        };
        match kind {
            Kind::BaseDoors => base,
            Kind::BaseDrawers => Module { w: 600.0, doors: 0, drawers: 3, shelves: 0, ..base },
            Kind::Wall => Module { h: 700.0, d: 350.0, kick: 0.0, pos: [0.0, 0.0, 1500.0], ..base },
            Kind::Tower => Module { w: 600.0, h: 2200.0, d: 550.0, shelves: 4, ..base },
            Kind::Wardrobe => Module { w: 1600.0, h: 2400.0, d: 600.0, doors: 4, shelves: 1, dividers: 1, hanger: true, ..base },
            Kind::Shelf => Module { w: 900.0, h: 1800.0, d: 350.0, doors: 0, shelves: 4, kick: 0.0, ..base },
        }
    }

    /// Medida (largura, profundidade) da projeção no piso, já girada (mm).
    pub fn footprint(&self) -> [f64; 2] {
        if self.rot % 2 == 0 { [self.w, self.d] } else { [self.d, self.w] }
    }

    /// Ponto local (mm) → mundo (mm).
    pub fn to_world(&self, p: DVec3) -> DVec3 {
        let r = |x: f64, y: f64| match self.rot % 4 {
            0 => (x, y),
            1 => (-y, x),
            2 => (-x, -y),
            _ => (y, -x),
        };
        // Canto mínimo da caixa girada, para alinhar com `pos`.
        let corners = [r(0.0, 0.0), r(self.w, 0.0), r(0.0, self.d), r(self.w, self.d)];
        let minx = corners.iter().map(|c| c.0).fold(f64::MAX, f64::min);
        let miny = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min);
        let (x, y) = r(p.x, p.y);
        DVec3::new(x - minx + self.pos[0], y - miny + self.pos[1], p.z + self.pos[2])
    }

    /// Caixa (mm) no mundo de uma caixa local.
    pub fn world_box(&self, min: DVec3, max: DVec3) -> (DVec3, DVec3) {
        let (a, b) = (self.to_world(min), self.to_world(max));
        (a.min(b), a.max(b))
    }

    /// Caixa envolvente do módulo no mundo (mm).
    pub fn bounds(&self) -> (DVec3, DVec3) {
        self.world_box(DVec3::ZERO, v(self.w, self.d, self.h))
    }

    /// Mantém os parâmetros num intervalo que gera peças válidas.
    pub fn sanitize(&mut self) {
        self.w = self.w.clamp(150.0, 3000.0);
        self.h = self.h.clamp(150.0, 2750.0);
        self.d = self.d.clamp(150.0, 1000.0);
        self.doors = self.doors.min(6);
        self.drawers = self.drawers.min(8);
        self.shelves = self.shelves.min(12);
        self.dividers = self.dividers.min(5);
        self.kick = self.kick.clamp(0.0, 250.0).min(self.h * 0.4);
        self.rot %= 4;
    }

    /// Peças e ferragens do módulo.
    pub fn build(&self, cat: &Catalog) -> (Vec<Board>, Vec<(HardwareKind, f64)>) {
        let mut out = Vec::new();
        let mut hw: Vec<(HardwareKind, f64)> = Vec::new();
        let mut add_hw = |k: HardwareKind, q: f64| match hw.iter_mut().find(|(kk, _)| *kk == k) {
            Some((_, n)) => *n += q,
            None => hw.push((k, q)),
        };

        let (w, h, d) = (self.w, self.h, self.d);
        let t = cat.material(self.box_mat).thickness;
        let tf = cat.material(self.front_mat).thickness;
        let tb = cat.material(self.back_mat).thickness;
        let has_front = self.doors > 0 || self.drawers > 0;
        let y0 = if has_front { tf } else { 0.0 };
        let zb = self.kick;
        let back_y1 = d - BACK_INSET;
        let back_y0 = back_y1 - tb;
        let (bm, fm, km) = (self.box_mat, self.front_mat, self.back_mat);

        // Caixa.
        out.push(board("Lateral esquerda", Role::Side, bm, v(0.0, y0, 0.0), v(t, d, h), Band::Front(2)));
        out.push(board("Lateral direita", Role::Side, bm, v(w - t, y0, 0.0), v(w, d, h), Band::Front(2)));
        out.push(board("Base", Role::Bottom, bm, v(t, y0, zb), v(w - t, d, zb + t), Band::Front(0)));
        if self.kind.has_rails() {
            out.push(board("Travessa frontal", Role::Rail, bm, v(t, y0, h - t), v(w - t, y0 + 100.0, h), Band::Front(0)));
            out.push(board("Travessa traseira", Role::Rail, bm, v(t, d - 100.0, h - t), v(w - t, d, h), Band::None));
        } else {
            out.push(board("Tampo", Role::Top, bm, v(t, y0, h - t), v(w - t, d, h), Band::Front(0)));
        }
        // Fundo encaixado em canal de 5 mm nas laterais, base e tampo.
        out.push(board("Fundo", Role::Back, km, v(t - 5.0, back_y0, zb + t - 5.0), v(w - t + 5.0, back_y1, h - t + 5.0), Band::None));
        if zb > 0.0 {
            out.push(board("Rodapé", Role::Kick, bm, v(t, 50.0, 0.0), v(w - t, 50.0 + t, zb), Band::None));
        }

        // Vãos entre divisórias.
        let n_div = self.dividers;
        let inner_w = w - 2.0 * t;
        let bay_w = (inner_w - n_div as f64 * t) / (n_div + 1) as f64;
        let bays: Vec<(f64, f64)> = (0..=n_div)
            .map(|i| {
                let x0 = t + i as f64 * (bay_w + t);
                (x0, x0 + bay_w)
            })
            .collect();
        for (i, (_, x1)) in bays.iter().enumerate().take(n_div as usize) {
            let _ = i;
            out.push(board("Divisória", Role::Divider, bm, v(*x1, y0, zb + t), v(x1 + t, back_y0, h - t), Band::Front(2)));
        }

        // Frentes: gavetas em cima (ou ocupando tudo) e portas embaixo.
        let mut door_zone = (zb, h);
        if self.drawers > 0 {
            let n = self.drawers as f64;
            let dh = if self.doors > 0 { DRAWER_FRONT_H } else { (h - zb) / n };
            let z0 = h - dh * n;
            door_zone = (zb, z0);
            let slide = (((d - y0 - BACK_INSET - tb - 20.0) / 50.0).floor() * 50.0).clamp(250.0, 650.0);
            let bx0 = t + SLIDE_GAP;
            let bx1 = w - t - SLIDE_GAP;
            for k in 0..self.drawers {
                let zf0 = z0 + k as f64 * dh;
                out.push(board("Frente de gaveta", Role::DrawerFront, fm, v(GAP / 2.0, 0.0, zf0 + GAP / 2.0), v(w - GAP / 2.0, tf, zf0 + dh - GAP / 2.0), Band::All));
                let zd = (zf0 + 20.0).max(zb + t + 5.0);
                let sh = (dh - GAP - 50.0).max(60.0);
                let (yd0, yd1) = (y0 + 5.0, y0 + 5.0 + slide);
                out.push(board("Lateral de gaveta", Role::DrawerSide, bm, v(bx0, yd0, zd), v(bx0 + t, yd1, zd + sh), Band::Front(0)));
                out.push(board("Lateral de gaveta", Role::DrawerSide, bm, v(bx1 - t, yd0, zd), v(bx1, yd1, zd + sh), Band::Front(0)));
                out.push(board("Contrafrente de gaveta", Role::DrawerBack, bm, v(bx0 + t, yd0, zd), v(bx1 - t, yd0 + t, zd + sh), Band::Front(0)));
                out.push(board("Traseira de gaveta", Role::DrawerBack, bm, v(bx0 + t, yd1 - t, zd), v(bx1 - t, yd1, zd + sh), Band::Front(0)));
                out.push(board("Fundo de gaveta", Role::DrawerBottom, km, v(bx0 + t - 5.0, yd0 + t - 5.0, zd + 10.0), v(bx1 - t + 5.0, yd1 - t + 5.0, zd + 10.0 + tb), Band::None));
                add_hw(HardwareKind::Slide, 1.0);
                add_hw(HardwareKind::Handle, 1.0);
            }
        }
        if self.doors > 0 && door_zone.1 - door_zone.0 > 50.0 {
            let n = self.doors as f64;
            let dw = (w - GAP * n) / n;
            let (dz0, dz1) = (door_zone.0 + GAP / 2.0, door_zone.1 - GAP / 2.0);
            for i in 0..self.doors {
                let x0 = GAP / 2.0 + i as f64 * (dw + GAP);
                out.push(board("Porta", Role::Door, fm, v(x0, 0.0, dz0), v(x0 + dw, tf, dz1), Band::All));
                add_hw(HardwareKind::Hinge, hinges_for(dz1 - dz0) as f64);
                add_hw(HardwareKind::Handle, 1.0);
            }
        }

        // Prateleiras (em cada vão, na faixa das portas ou no módulo todo se for aberto).
        let (lo, hi) = if self.drawers > 0 && self.doors > 0 {
            (zb + t, door_zone.1 - t)
        } else if self.drawers > 0 {
            (0.0, 0.0) // gaveteiro: sem prateleiras
        } else {
            (zb + t, h - t)
        };
        if hi - lo > 100.0 {
            let shelf_y = (y0 + 20.0, back_y0 - 2.0);
            let mut zs: Vec<f64> = Vec::new();
            if self.hanger {
                // Maleiro: prateleiras a partir de cima, a cada 350 mm; o cabideiro fica embaixo.
                for k in 0..self.shelves {
                    let z = hi - 350.0 * (k + 1) as f64;
                    if z > lo + 900.0 {
                        zs.push(z);
                    }
                }
            } else {
                let s = self.shelves as f64;
                zs = (1..=self.shelves).map(|k| lo + (hi - lo) * k as f64 / (s + 1.0) - t / 2.0).collect();
            }
            for (x0, x1) in &bays {
                for z in &zs {
                    out.push(board("Prateleira", Role::Shelf, bm, v(x0 + 1.0, shelf_y.0, *z), v(x1 - 1.0, shelf_y.1, z + t), Band::Front(0)));
                    add_hw(HardwareKind::ShelfSupport, 4.0);
                }
                if self.hanger {
                    add_hw(HardwareKind::HangerRail, ((x1 - x0) / 1000.0 * 100.0).round() / 100.0);
                    add_hw(HardwareKind::HangerSupport, 2.0);
                }
            }
        }
        (out, hw)
    }

    /// Altura onde fica o cabideiro (mm, local), se houver.
    pub fn hanger_z(&self, cat: &Catalog) -> Option<f64> {
        if !self.hanger {
            return None;
        }
        let t = cat.material(self.box_mat).thickness;
        let hi = self.h - t;
        let lowest_shelf = (0..self.shelves).map(|k| hi - 350.0 * (k + 1) as f64).filter(|z| *z > self.kick + t + 900.0).fold(hi, f64::min);
        Some(lowest_shelf - 60.0)
    }
}

/// Item da galeria (como as bibliotecas do Promob: categoria › grupo › item).
pub struct Preset {
    pub category: &'static str,
    pub group: &'static str,
    pub name: &'static str,
    pub make: fn() -> Module,
}

pub fn gallery() -> Vec<Preset> {
    fn named(mut m: Module, name: &str) -> Module {
        m.name = name.into();
        m
    }
    vec![
        Preset { category: "Cozinha", group: "Inferiores", name: "Balcão 2 portas 80 cm", make: || named(Module::new(Kind::BaseDoors), "Balcão 2 portas") },
        Preset { category: "Cozinha", group: "Inferiores", name: "Balcão 1 porta 40 cm", make: || named(Module { w: 400.0, doors: 1, ..Module::new(Kind::BaseDoors) }, "Balcão 1 porta") },
        Preset { category: "Cozinha", group: "Inferiores", name: "Balcão pia 120 cm", make: || named(Module { w: 1200.0, shelves: 0, ..Module::new(Kind::BaseDoors) }, "Balcão pia") },
        Preset { category: "Cozinha", group: "Inferiores", name: "Balcão 1 gaveta + 2 portas", make: || named(Module { drawers: 1, ..Module::new(Kind::BaseDoors) }, "Balcão gaveta e portas") },
        Preset { category: "Cozinha", group: "Inferiores", name: "Gaveteiro 3 gavetas 60 cm", make: || named(Module::new(Kind::BaseDrawers), "Gaveteiro 3 gavetas") },
        Preset { category: "Cozinha", group: "Inferiores", name: "Gaveteiro 4 gavetas 40 cm", make: || named(Module { w: 400.0, drawers: 4, ..Module::new(Kind::BaseDrawers) }, "Gaveteiro 4 gavetas") },
        Preset { category: "Cozinha", group: "Aéreos", name: "Aéreo 2 portas 80 cm", make: || named(Module::new(Kind::Wall), "Aéreo 2 portas") },
        Preset { category: "Cozinha", group: "Aéreos", name: "Aéreo 1 porta 40 cm", make: || named(Module { w: 400.0, doors: 1, ..Module::new(Kind::Wall) }, "Aéreo 1 porta") },
        Preset { category: "Cozinha", group: "Aéreos", name: "Aéreo basculante 80 cm", make: || named(Module { h: 350.0, doors: 1, shelves: 0, pos: [0.0, 0.0, 1850.0], ..Module::new(Kind::Wall) }, "Aéreo basculante") },
        Preset { category: "Cozinha", group: "Torres", name: "Paneleiro 60 cm", make: || named(Module::new(Kind::Tower), "Paneleiro") },
        Preset { category: "Cozinha", group: "Torres", name: "Torre quente (forno) 60 cm", make: || named(Module { doors: 2, drawers: 2, shelves: 2, ..Module::new(Kind::Tower) }, "Torre quente") },
        Preset { category: "Dormitório", group: "Roupeiros", name: "Roupeiro 4 portas 160 cm", make: || named(Module::new(Kind::Wardrobe), "Roupeiro 4 portas") },
        Preset { category: "Dormitório", group: "Roupeiros", name: "Roupeiro 2 portas 90 cm", make: || named(Module { w: 900.0, doors: 2, dividers: 0, ..Module::new(Kind::Wardrobe) }, "Roupeiro 2 portas") },
        Preset { category: "Dormitório", group: "Roupeiros", name: "Roupeiro 6 portas 240 cm", make: || named(Module { w: 2400.0, doors: 6, dividers: 2, ..Module::new(Kind::Wardrobe) }, "Roupeiro 6 portas") },
        Preset { category: "Dormitório", group: "Cômodas", name: "Cômoda 4 gavetas 80 cm", make: || named(Module { w: 800.0, h: 900.0, d: 450.0, drawers: 4, ..Module::new(Kind::BaseDrawers) }, "Cômoda 4 gavetas") },
        Preset { category: "Dormitório", group: "Cômodas", name: "Criado-mudo 2 gavetas", make: || named(Module { w: 450.0, h: 500.0, d: 400.0, drawers: 2, kick: 60.0, ..Module::new(Kind::BaseDrawers) }, "Criado-mudo") },
        Preset { category: "Banheiro", group: "Gabinetes", name: "Gabinete 2 portas 80 cm", make: || named(Module { h: 600.0, d: 450.0, shelves: 0, kick: 0.0, pos: [0.0, 0.0, 250.0], ..Module::new(Kind::BaseDoors) }, "Gabinete banheiro") },
        Preset { category: "Banheiro", group: "Gabinetes", name: "Espelheira 60 cm", make: || named(Module { w: 600.0, h: 700.0, d: 150.0, doors: 1, pos: [0.0, 0.0, 1100.0], ..Module::new(Kind::Wall) }, "Espelheira") },
        Preset { category: "Escritório", group: "Estantes", name: "Estante 90 cm", make: || named(Module::new(Kind::Shelf), "Estante") },
        Preset { category: "Escritório", group: "Estantes", name: "Nicho 60 cm", make: || named(Module { w: 600.0, h: 300.0, d: 300.0, shelves: 0, pos: [0.0, 0.0, 1400.0], ..Module::new(Kind::Shelf) }, "Nicho") },
        Preset { category: "Escritório", group: "Armários", name: "Armário alto 2 portas", make: || named(Module { w: 800.0, h: 2100.0, d: 450.0, shelves: 4, ..Module::new(Kind::Tower) }, "Armário alto") },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(b: &'a [Board], name: &str) -> Vec<&'a Board> {
        b.iter().filter(|x| x.name == name).collect()
    }

    #[test]
    fn base_cabinet_parts_have_market_measures() {
        let cat = Catalog::default();
        let m = Module::new(Kind::BaseDoors); // 800 × 800 × 550, caixa 15, frente 18, fundo 6
        let (b, hw) = m.build(&cat);
        let side = find(&b, "Lateral esquerda")[0];
        assert_eq!((side.length, side.width, side.thickness), (800.0, 532.0, 15.0));
        assert_eq!(side.edges, [true, false, false, false]);
        let base = find(&b, "Base")[0];
        assert_eq!((base.length, base.width), (770.0, 532.0));
        let doors = find(&b, "Porta");
        assert_eq!(doors.len(), 2);
        // (800 − 2 × 3) / 2 = 397 de largura; 700 − 3 de altura
        assert_eq!((doors[0].length, doors[0].width, doors[0].thickness), (697.0, 397.0, 18.0));
        assert_eq!(find(&b, "Fundo")[0].thickness, 6.0);
        assert_eq!(find(&b, "Travessa frontal").len(), 1);
        assert_eq!(find(&b, "Rodapé").len(), 1);
        let hinges = hw.iter().find(|(k, _)| *k == HardwareKind::Hinge).unwrap().1;
        assert_eq!(hinges, 4.0);
    }

    #[test]
    fn drawers_get_slides_and_boxes() {
        let cat = Catalog::default();
        let (b, hw) = Module::new(Kind::BaseDrawers).build(&cat);
        assert_eq!(find(&b, "Frente de gaveta").len(), 3);
        assert_eq!(find(&b, "Lateral de gaveta").len(), 6);
        assert_eq!(find(&b, "Prateleira").len(), 0);
        assert_eq!(hw.iter().find(|(k, _)| *k == HardwareKind::Slide).unwrap().1, 3.0);
        // corrediça arredondada para baixo em múltiplos de 50
        let side = find(&b, "Lateral de gaveta")[0];
        assert_eq!(side.length % 50.0, 0.0);
    }

    #[test]
    fn wardrobe_dividers_split_shelves_and_rails() {
        let cat = Catalog::default();
        let (b, hw) = Module::new(Kind::Wardrobe).build(&cat);
        assert_eq!(find(&b, "Divisória").len(), 1);
        assert_eq!(find(&b, "Prateleira").len(), 2, "uma por vão");
        assert_eq!(hw.iter().find(|(k, _)| *k == HardwareKind::HangerSupport).unwrap().1, 4.0);
        let doors = find(&b, "Porta");
        assert_eq!(hinges_for(doors[0].length), 5);
    }

    #[test]
    fn rotation_keeps_footprint_at_pos() {
        for rot in 0..4 {
            let m = Module { rot, pos: [1000.0, 2000.0, 0.0], ..Module::new(Kind::BaseDoors) };
            let (a, b) = m.bounds();
            let f = m.footprint();
            assert!((a.x - 1000.0).abs() < 1e-9 && (a.y - 2000.0).abs() < 1e-9, "rot {rot}: {a:?}");
            assert!((b.x - a.x - f[0]).abs() < 1e-9 && (b.y - a.y - f[1]).abs() < 1e-9);
        }
        // rot 1 encosta na parede esquerda: a frente (y local = 0) fica do lado +x
        let m = Module { rot: 1, ..Module::new(Kind::BaseDoors) };
        assert!(m.to_world(DVec3::new(0.0, 0.0, 0.0)).x > m.to_world(DVec3::new(0.0, m.d, 0.0)).x);
    }
}
