// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Projeto: ambiente + módulos + catálogo. Salvo em `.planejei` (JSON versionado).

use std::path::{Path, PathBuf};

use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, HardwareKind};
use crate::cutting::{self, Piece, Plan};
use crate::module::{Board, Module};

pub const EXTENSION: &str = "planejei";
const FORMAT: &str = "planejei";
/// Identificador usado antes da troca de nome (GeoMóvel).
const LEGACY_FORMAT: &str = "geomovel";
/// ExtensÃµes aceitas ao abrir (a atual e a antiga).
pub const OPEN_EXTENSIONS: &[&str] = &["planejei", "geomovel"];
const VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Room {
    /// Medidas internas (mm): largura (x), profundidade (y) e pé-direito.
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    /// Espessura das paredes (mm).
    pub wall: f64,
    /// Desenha as paredes simples (desligue ao usar um ambiente do GeoArq).
    pub show_walls: bool,
    /// Parede da frente (normalmente oculta para ver o interior).
    pub front_wall: bool,
}

impl Default for Room {
    fn default() -> Self {
        Self { width: 3600.0, depth: 3000.0, height: 2700.0, wall: 150.0, show_walls: true, front_wall: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Project {
    format: String,
    version: u32,
    pub name: String,
    pub client: String,
    pub room: Room,
    pub modules: Vec<Module>,
    pub catalog: Catalog,
    /// Ambiente do GeoArq usado como referência (carregado de novo ao abrir).
    #[serde(default)]
    pub geoarq: Option<PathBuf>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            name: "Projeto sem título".into(),
            client: String::new(),
            room: Room::default(),
            modules: Vec::new(),
            catalog: Catalog::default(),
            geoarq: None,
        }
    }
}

/// Parede do ambiente a que um módulo está encostado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wall {
    Back,
    Left,
    Front,
    Right,
}

impl Wall {
    /// Rotação que deixa a frente do módulo virada para o ambiente.
    pub fn rot(self) -> u8 {
        match self {
            Wall::Back => 0,
            Wall::Left => 1,
            Wall::Front => 2,
            Wall::Right => 3,
        }
    }
}

/// Peças e ferragens de todo o projeto.
pub struct Takeoff {
    /// (módulo, peça).
    pub boards: Vec<(usize, Board)>,
    pub hardware: Vec<(HardwareKind, f64)>,
}

impl Project {
    pub fn takeoff(&self) -> Takeoff {
        let mut boards = Vec::new();
        let mut hardware: Vec<(HardwareKind, f64)> = Vec::new();
        for (i, m) in self.modules.iter().enumerate() {
            let (b, hw) = m.build(&self.catalog);
            boards.extend(b.into_iter().map(|b| (i, b)));
            for (k, q) in hw {
                match hardware.iter_mut().find(|(kk, _)| *kk == k) {
                    Some((_, n)) => *n += q,
                    None => hardware.push((k, q)),
                }
            }
        }
        hardware.sort_by_key(|(k, _)| *k);
        Takeoff { boards, hardware }
    }

    pub fn cut_plan(&self, t: &Takeoff) -> Plan {
        let pieces = t
            .boards
            .iter()
            .map(|(mi, b)| Piece {
                module: *mi,
                label: b.name.clone(),
                length: b.length,
                width: b.width,
                material: b.material,
                grain: self.catalog.material(b.material).grain,
            })
            .collect();
        cutting::optimize(pieces, &self.catalog)
    }

    /// Posição inicial de um módulo novo: na parede do fundo, ao lado do último da mesma faixa
    /// (inferiores com inferiores, aéreos com aéreos).
    pub fn auto_place(&self, m: &mut Module) {
        m.rot = 0;
        let hung = m.kind.hung() || m.pos[2] > 0.0;
        let next_x = self
            .modules
            .iter()
            .filter(|o| o.rot == 0 && (o.kind.hung() || o.pos[2] > 0.0) == hung)
            .map(|o| o.bounds().1.x)
            .fold(0.0, f64::max);
        m.pos[0] = next_x;
        m.pos[1] = self.room.depth - m.d;
    }

    /// Encaixe ao arrastar: encosta na parede mais próxima (girando o módulo para o ambiente)
    /// e alinha as laterais às dos outros módulos. `center` é o centro desejado no piso (mm).
    pub fn snap(&self, idx: usize, center: [f64; 2]) -> Module {
        let mut m = self.modules[idx].clone();
        const WALL_SNAP: f64 = 250.0;
        const SIDE_SNAP: f64 = 60.0;
        let r = &self.room;
        // Distância de cada parede até a borda do módulo (com a rotação dessa parede).
        let half = |rot: u8| if rot % 2 == 0 { [m.w / 2.0, m.d / 2.0] } else { [m.d / 2.0, m.w / 2.0] };
        let walls = [
            (Wall::Back, r.depth - (center[1] + half(0)[1])),
            (Wall::Left, center[0] - half(1)[0]),
            (Wall::Front, center[1] - half(2)[1]),
            (Wall::Right, r.width - (center[0] + half(3)[0])),
        ];
        let near = walls.iter().filter(|(_, dist)| dist.abs() < WALL_SNAP).min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
        if let Some((wall, _)) = near {
            m.rot = wall.rot();
        }
        let f = m.footprint();
        let mut x = center[0] - f[0] / 2.0;
        let mut y = center[1] - f[1] / 2.0;
        match near.map(|n| n.0) {
            Some(Wall::Back) => y = r.depth - f[1],
            Some(Wall::Front) => y = 0.0,
            Some(Wall::Left) => x = 0.0,
            Some(Wall::Right) => x = r.width - f[0],
            None => {}
        }
        // Lateral com lateral (e com os cantos do ambiente).
        let along_x = matches!(near.map(|n| n.0), Some(Wall::Back | Wall::Front) | None);
        let mut targets: Vec<f64> = vec![0.0, if along_x { r.width } else { r.depth }];
        for (j, o) in self.modules.iter().enumerate() {
            if j == idx {
                continue;
            }
            let (a, b) = o.bounds();
            if along_x {
                targets.extend([a.x, b.x]);
            } else {
                targets.extend([a.y, b.y]);
            }
        }
        let (lo, len) = if along_x { (x, f[0]) } else { (y, f[1]) };
        let mut best: Option<(f64, f64)> = None;
        for t in targets {
            for edge in [lo, lo + len] {
                let d = t - edge;
                if d.abs() < SIDE_SNAP && best.is_none_or(|b| d.abs() < b.0) {
                    best = Some((d.abs(), d));
                }
            }
        }
        if let Some((_, d)) = best {
            if along_x {
                x += d;
            } else {
                y += d;
            }
        }
        m.pos[0] = x;
        m.pos[1] = y;
        m
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("projeto serializável")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let p: Project = serde_json::from_str(text).map_err(|e| format!("Arquivo inválido: {e}"))?;
        if p.format != FORMAT && p.format != LEGACY_FORMAT {
            return Err("Este arquivo não é um projeto do Planejei.".into());
        }
        if p.version > VERSION {
            return Err(format!("O arquivo foi salvo por uma versão mais nova (formato {}).", p.version));
        }
        Ok(p)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json()).map_err(|e| format!("Não foi possível salvar: {e}"))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("Não foi possível abrir: {e}"))?;
        Self::from_json(&text)
    }

    /// Cozinha de exemplo (para conhecer o programa).
    pub fn demo_kitchen() -> Self {
        use crate::module::gallery;
        let mut p = Project { name: "Cozinha exemplo".into(), client: "Cliente exemplo".into(), ..Default::default() };
        let g = gallery();
        let make = |name: &str| (g.iter().find(|x| x.name == name).expect("item da galeria").make)();
        for name in ["Paneleiro 60 cm", "Gaveteiro 3 gavetas 60 cm", "Balcão pia 120 cm", "Balcão 2 portas 80 cm"] {
            let mut m = make(name);
            p.auto_place(&mut m);
            p.modules.push(m);
        }
        let mut x = 600.0;
        for name in ["Aéreo 2 portas 80 cm", "Aéreo 2 portas 80 cm", "Aéreo 2 portas 80 cm"] {
            let mut m = make(name);
            p.auto_place(&mut m);
            m.pos[0] = x;
            x += m.w;
            p.modules.push(m);
        }
        // Fachadas em carvalho nos aéreos, grafite nos inferiores.
        for m in &mut p.modules {
            m.front_mat = if m.kind.hung() { 2 } else { 3 };
        }
        let mut side = make("Balcão 2 portas 80 cm");
        side.rot = 3;
        side.pos = [p.room.width - side.d, 600.0, 0.0];
        side.front_mat = 3;
        p.modules.push(side);
        p
    }
}

// ------------------------------------------------------------------ ambiente do GeoArq

/// Lê um `.geoarq` (só a geometria, em metros) para servir de ambiente de referência.
pub fn load_geoarq(path: &Path) -> Result<Vec<crate::geom::Mesh>, String> {
    use crate::geom::{Mesh, RawMesh};
    #[derive(Deserialize)]
    struct FaceRec {
        outer: Vec<usize>,
        #[serde(default)]
        holes: Vec<Vec<usize>>,
        normal: [f64; 3],
        #[serde(default)]
        material: Option<[f32; 3]>,
    }
    #[derive(Deserialize, Default)]
    struct MeshRec {
        #[serde(default)]
        vertices: Vec<[f64; 3]>,
        #[serde(default)]
        edges: Vec<[usize; 2]>,
        #[serde(default)]
        soft: Vec<usize>,
        #[serde(default)]
        faces: Vec<FaceRec>,
    }
    #[derive(Deserialize)]
    struct DefRec {
        mesh: MeshRec,
    }
    #[derive(Deserialize)]
    struct InstRec {
        def: usize,
        xf: Vec<f64>,
        #[serde(default)]
        material: Option<[f32; 3]>,
    }
    #[derive(Deserialize)]
    struct FileRec {
        format: String,
        #[serde(flatten)]
        root: MeshRec,
        #[serde(default)]
        definitions: Vec<DefRec>,
        #[serde(default)]
        instances: Vec<InstRec>,
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Não foi possível abrir: {e}"))?;
    let rec: FileRec = serde_json::from_str(&text).map_err(|e| format!("Arquivo do GeoArq inválido: {e}"))?;
    if rec.format != "geoarq" && rec.format != "sketchuo" {
        return Err("Este arquivo não é um modelo do GeoArq.".into());
    }
    let to_mesh = |m: &MeshRec, fallback: Option<[f32; 3]>| {
        Mesh::from_raw(&RawMesh {
            verts: m.vertices.iter().map(|p| DVec3::from_array(*p)).collect(),
            edges: m.edges.clone(),
            soft: m.soft.clone(),
            faces: m.faces.iter().map(|f| (f.outer.clone(), f.holes.clone(), DVec3::from_array(f.normal), f.material.or(fallback))).collect(),
            face_textures: Vec::new(),
            guides: Vec::new(),
        })
    };
    let mut out = vec![to_mesh(&rec.root, None)?];
    for i in &rec.instances {
        let def = rec.definitions.get(i.def).ok_or("Instância aponta para definição inexistente.")?;
        let arr: [f64; 12] = i.xf.clone().try_into().map_err(|_| "Transformação inválida.".to_string())?;
        out.push(to_mesh(&def.mesh, i.material)?.transformed(glam::DAffine3::from_cols_array(&arr)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::Kind;

    #[test]
    fn json_round_trip() {
        let p = Project::demo_kitchen();
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn opens_files_saved_as_geomovel() {
        let json = Project::default().to_json().replacen("\"planejei\"", "\"geomovel\"", 1);
        assert!(Project::from_json(&json).is_ok());
        let other = Project::default().to_json().replacen("\"planejei\"", "\"outro\"", 1);
        assert!(Project::from_json(&other).is_err());
    }

    #[test]
    fn auto_place_lines_modules_up_on_the_back_wall() {
        let mut p = Project::default();
        for _ in 0..3 {
            let mut m = Module::new(Kind::BaseDoors);
            p.auto_place(&mut m);
            p.modules.push(m);
        }
        let xs: Vec<f64> = p.modules.iter().map(|m| m.pos[0]).collect();
        assert_eq!(xs, vec![0.0, 800.0, 1600.0]);
        assert!(p.modules.iter().all(|m| m.pos[1] + m.d == p.room.depth));
    }

    #[test]
    fn snap_turns_to_the_nearest_wall_and_aligns_sides() {
        let mut p = Project::default();
        p.modules.push(Module { pos: [0.0, p.room.depth - 550.0, 0.0], ..Module::new(Kind::BaseDoors) });
        p.modules.push(Module::new(Kind::BaseDoors));
        // perto da parede esquerda, a meia altura do ambiente
        let m = p.snap(1, [300.0, 1500.0]);
        assert_eq!(m.rot, 1);
        assert_eq!(m.pos[0], 0.0);
        // perto da parede do fundo, 30 mm depois do primeiro módulo: encosta nele
        let m = p.snap(1, [800.0 + 400.0 + 30.0, p.room.depth - 300.0]);
        assert_eq!(m.rot, 0);
        assert_eq!(m.pos, [800.0, p.room.depth - 550.0, 0.0]);
    }
}
