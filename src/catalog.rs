// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Catálogo editável: chapas (com fita de borda), ferragens e parâmetros de produção/orçamento.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Material {
    pub name: String,
    /// Espessura (mm).
    pub thickness: f64,
    /// Tamanho da chapa (mm): comprimento (sentido do veio) × largura.
    pub sheet: [f64; 2],
    /// Preço da chapa inteira (R$).
    pub sheet_price: f64,
    /// Fita de borda da mesma cor (R$/m).
    pub edge_price: f64,
    /// Cor (sRGB 0..1) para o 3D.
    pub color: [f32; 3],
    /// Textura embutida para o 3D (ex.: "Madeira clara").
    #[serde(default)]
    pub texture: Option<String>,
    /// Padrão amadeirado: a peça não pode girar na chapa.
    pub grain: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HardwareKind {
    Hinge,
    Slide,
    Handle,
    ShelfSupport,
    HangerRail,
    HangerSupport,
    Foot,
}

impl HardwareKind {
    pub const ALL: [HardwareKind; 7] = [
        HardwareKind::Hinge,
        HardwareKind::Slide,
        HardwareKind::Handle,
        HardwareKind::ShelfSupport,
        HardwareKind::HangerRail,
        HardwareKind::HangerSupport,
        HardwareKind::Foot,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HardwareKind::Hinge => "Dobradiça 35 mm com amortecedor",
            HardwareKind::Slide => "Corrediça telescópica (par)",
            HardwareKind::Handle => "Puxador",
            HardwareKind::ShelfSupport => "Suporte de prateleira",
            HardwareKind::HangerRail => "Tubo cabideiro oblongo",
            HardwareKind::HangerSupport => "Suporte de cabideiro",
            HardwareKind::Foot => "Pé regulável",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            HardwareKind::HangerRail => "m",
            HardwareKind::Slide => "par",
            _ => "un.",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HardwarePrice {
    pub kind: HardwareKind,
    pub price: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CutSettings {
    /// Espessura do disco da serra (mm).
    pub kerf: f64,
    /// Refilo em cada borda da chapa (mm).
    pub trim: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetSettings {
    /// Mão de obra sobre o custo de material (%).
    pub labor_pct: f64,
    /// Margem sobre o custo total (%).
    pub margin_pct: f64,
    /// Cobrar chapa inteira (true) ou só a área usada + perda (false).
    pub whole_sheets: bool,
    /// Perda considerada ao cobrar por área (%).
    pub loss_pct: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Catalog {
    pub materials: Vec<Material>,
    pub hardware: Vec<HardwarePrice>,
    pub cut: CutSettings,
    pub budget: BudgetSettings,
}

impl Catalog {
    pub fn hardware_price(&self, kind: HardwareKind) -> f64 {
        self.hardware.iter().find(|h| h.kind == kind).map(|h| h.price).unwrap_or(0.0)
    }

    pub fn material(&self, i: usize) -> &Material {
        &self.materials[i.min(self.materials.len() - 1)]
    }
}

pub const WOOD_TEXTURE: &str = "Madeira clara";

impl Default for Catalog {
    fn default() -> Self {
        let mat = |name: &str, t: f64, price: f64, edge: f64, color: [f32; 3], texture: Option<&str>, grain: bool| Material {
            name: name.into(),
            thickness: t,
            sheet: [2750.0, 1850.0],
            sheet_price: price,
            edge_price: edge,
            color,
            texture: texture.map(Into::into),
            grain,
        };
        Self {
            materials: vec![
                mat("MDF Branco TX 15 mm", 15.0, 265.0, 1.6, [0.95, 0.95, 0.93], None, false),
                mat("MDF Branco TX 18 mm", 18.0, 315.0, 1.8, [0.95, 0.95, 0.93], None, false),
                mat("MDF Carvalho 18 mm", 18.0, 430.0, 2.6, [0.80, 0.66, 0.46], Some(WOOD_TEXTURE), true),
                mat("MDF Grafite 18 mm", 18.0, 395.0, 2.2, [0.30, 0.31, 0.33], None, false),
                mat("MDF Verde Sálvia 18 mm", 18.0, 410.0, 2.4, [0.60, 0.66, 0.57], None, false),
                mat("HDF Branco 6 mm", 6.0, 120.0, 0.0, [0.93, 0.93, 0.91], None, false),
            ],
            hardware: vec![
                HardwarePrice { kind: HardwareKind::Hinge, price: 12.0 },
                HardwarePrice { kind: HardwareKind::Slide, price: 48.0 },
                HardwarePrice { kind: HardwareKind::Handle, price: 22.0 },
                HardwarePrice { kind: HardwareKind::ShelfSupport, price: 0.45 },
                HardwarePrice { kind: HardwareKind::HangerRail, price: 32.0 },
                HardwarePrice { kind: HardwareKind::HangerSupport, price: 4.5 },
                HardwarePrice { kind: HardwareKind::Foot, price: 3.8 },
            ],
            cut: CutSettings { kerf: 4.0, trim: 10.0 },
            budget: BudgetSettings { labor_pct: 45.0, margin_pct: 30.0, whole_sheets: true, loss_pct: 15.0 },
        }
    }
}

/// Índices dos materiais padrão.
pub const MAT_BOX: usize = 0;
pub const MAT_FRONT: usize = 1;
pub const MAT_BACK: usize = 5;
