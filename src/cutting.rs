// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Plano de corte: encaixa as peças nas chapas com cortes de guilhotina (como a seccionadora),
//! descontando o refilo e a espessura da serra.

use crate::catalog::Catalog;

#[derive(Clone, Debug)]
pub struct Piece {
    /// Módulo de origem (índice) e nome da peça.
    pub module: usize,
    pub label: String,
    pub length: f64,
    pub width: f64,
    pub material: usize,
    /// O material tem veio: o comprimento da peça segue o comprimento da chapa.
    pub grain: bool,
}

#[derive(Clone, Debug)]
pub struct Placed {
    pub piece: usize,
    /// Posição e tamanho na chapa (mm), a partir do canto da área útil (já sem refilo).
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotated: bool,
}

#[derive(Clone, Debug)]
pub struct Sheet {
    pub material: usize,
    pub placed: Vec<Placed>,
    /// Área ocupada pelas peças / área útil.
    pub usage: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub pieces: Vec<Piece>,
    pub sheets: Vec<Sheet>,
    /// Peças maiores que a chapa.
    pub too_big: Vec<usize>,
}

impl Plan {
    pub fn sheets_of(&self, material: usize) -> usize {
        self.sheets.iter().filter(|s| s.material == material).count()
    }

    /// Área das peças de um material (m²).
    pub fn piece_area_m2(&self, material: usize) -> f64 {
        self.pieces.iter().filter(|p| p.material == material).map(|p| p.length * p.width / 1e6).sum()
    }

    pub fn materials(&self) -> Vec<usize> {
        let mut m: Vec<usize> = self.pieces.iter().map(|p| p.material).collect();
        m.sort();
        m.dedup();
        m
    }
}

#[derive(Clone, Copy, Debug)]
struct Free {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Open {
    material: usize,
    free: Vec<Free>,
    placed: Vec<Placed>,
}

pub fn optimize(pieces: Vec<Piece>, cat: &Catalog) -> Plan {
    let kerf = cat.cut.kerf.max(0.0);
    let trim = cat.cut.trim.max(0.0);
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    // Ordena por material e, dentro dele, da maior para a menor área.
    order.sort_by(|a, b| {
        let (pa, pb) = (&pieces[*a], &pieces[*b]);
        pa.material
            .cmp(&pb.material)
            .then((pb.length * pb.width).total_cmp(&(pa.length * pa.width)))
            .then(pb.length.total_cmp(&pa.length))
    });

    let mut open: Vec<Open> = Vec::new();
    let mut too_big = Vec::new();
    for &i in &order {
        let p = &pieces[i];
        let mat = cat.material(p.material);
        // Área útil ampliada pela serra: a última peça de cada fileira não precisa do corte extra.
        let (uw, uh) = (mat.sheet[0] - 2.0 * trim + kerf, mat.sheet[1] - 2.0 * trim + kerf);
        let mut options = vec![(p.length + kerf, p.width + kerf, false)];
        if !p.grain {
            options.push((p.width + kerf, p.length + kerf, true));
        }
        let mut best: Option<(f64, f64, usize, usize, f64, f64, bool)> = None;
        let consider = |sheet: usize, free: &[Free], best: &mut Option<(f64, f64, usize, usize, f64, f64, bool)>| {
            for (fi, f) in free.iter().enumerate() {
                for &(w, h, rot) in &options {
                    if w <= f.w + 1e-6 && h <= f.h + 1e-6 {
                        let area = f.w * f.h - w * h;
                        let short = (f.w - w).min(f.h - h);
                        if best.is_none_or(|b| (area, short) < (b.0, b.1)) {
                            *best = Some((area, short, sheet, fi, w, h, rot));
                        }
                    }
                }
            }
        };
        for (si, s) in open.iter().enumerate().filter(|(_, s)| s.material == p.material) {
            consider(si, &s.free, &mut best);
        }
        if best.is_none() {
            let fresh = [Free { x: 0.0, y: 0.0, w: uw, h: uh }];
            let mut b = None;
            consider(open.len(), &fresh, &mut b);
            if b.is_none() {
                too_big.push(i);
                continue;
            }
            open.push(Open { material: p.material, free: fresh.to_vec(), placed: Vec::new() });
            best = b;
        }
        let (_, _, si, fi, w, h, rotated) = best.expect("checado acima");
        let s = &mut open[si];
        let f = s.free.swap_remove(fi);
        s.placed.push(Placed { piece: i, x: f.x, y: f.y, w: w - kerf, h: h - kerf, rotated });
        // Guilhotina: corta pelo lado que deixa a maior sobra inteira.
        let (rw, rh) = (f.w - w, f.h - h);
        let (a, b) = if rw < rh {
            (Free { x: f.x + w, y: f.y, w: rw, h }, Free { x: f.x, y: f.y + h, w: f.w, h: rh })
        } else {
            (Free { x: f.x + w, y: f.y, w: rw, h: f.h }, Free { x: f.x, y: f.y + h, w, h: rh })
        };
        for r in [a, b] {
            if r.w > 1.0 && r.h > 1.0 {
                s.free.push(r);
            }
        }
    }

    let sheets = open
        .into_iter()
        .map(|o| {
            let mat = cat.material(o.material);
            let useful = (mat.sheet[0] - 2.0 * trim) * (mat.sheet[1] - 2.0 * trim);
            let used: f64 = o.placed.iter().map(|p| p.w * p.h).sum();
            Sheet { material: o.material, placed: o.placed, usage: used / useful }
        })
        .collect();
    Plan { pieces, sheets, too_big }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(l: f64, w: f64, grain: bool) -> Piece {
        Piece { module: 0, label: "p".into(), length: l, width: w, material: 0, grain }
    }

    fn overlaps(a: &Placed, b: &Placed) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn pieces_fit_without_overlap_and_inside_the_sheet() {
        let cat = Catalog::default();
        let mut ps = Vec::new();
        for _ in 0..10 {
            ps.push(piece(800.0, 532.0, false));
            ps.push(piece(697.0, 397.0, false));
            ps.push(piece(770.0, 100.0, false));
        }
        let plan = optimize(ps, &cat);
        assert!(plan.too_big.is_empty());
        let (uw, uh) = (2750.0 - 20.0, 1850.0 - 20.0);
        for s in &plan.sheets {
            for (i, a) in s.placed.iter().enumerate() {
                assert!(a.x + a.w <= uw + 1e-6 && a.y + a.h <= uh + 1e-6, "fora da chapa: {a:?}");
                for b in &s.placed[i + 1..] {
                    assert!(!overlaps(a, b), "{a:?} × {b:?}");
                    // serra entre peças vizinhas
                    let gap_x = (b.x - (a.x + a.w)).max(a.x - (b.x + b.w));
                    let gap_y = (b.y - (a.y + a.h)).max(a.y - (b.y + b.h));
                    assert!(gap_x >= 4.0 - 1e-6 || gap_y >= 4.0 - 1e-6);
                }
            }
        }
        let total: usize = plan.sheets.iter().map(|s| s.placed.len()).sum();
        assert_eq!(total, 30);
        // 30 peças ≈ 7,8 m² numa chapa útil de 5,0 m²: no mínimo 2, aceitável 3
        assert!((2..=3).contains(&plan.sheets.len()), "{} chapas", plan.sheets.len());
    }

    #[test]
    fn grain_forbids_rotation_and_oversize_is_reported() {
        let cat = Catalog::default();
        // 1900 de comprimento só cabe no sentido de 2750
        let plan = optimize(vec![piece(1900.0, 400.0, true), piece(3000.0, 300.0, false)], &cat);
        assert_eq!(plan.too_big, vec![1]);
        let p = &plan.sheets[0].placed[0];
        assert!(!p.rotated && p.w == 1900.0);
    }
}
