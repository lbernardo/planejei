// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
use glam::{DVec2, DVec3};

use super::TOL;

/// Plano `dot(n, p) = d`, com `n` unitário.
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub n: DVec3,
    pub d: f64,
}

impl Plane {
    pub fn from_point_normal(p: DVec3, n: DVec3) -> Self {
        let n = n.normalize();
        Self { n, d: n.dot(p) }
    }

    /// Plano por três pontos, se não forem colineares.
    pub fn from_points(a: DVec3, b: DVec3, c: DVec3) -> Option<Self> {
        let n = (b - a).cross(c - a);
        let scale = (b - a).length() * (c - a).length();
        if scale <= 0.0 || n.length() < scale * 1e-9 {
            return None;
        }
        Some(Self::from_point_normal(a, n))
    }

    /// Mesma superfície com normal em sentido canônico (maior componente positiva).
    pub fn canonical(self) -> Self {
        let n = self.n;
        let a = n.abs();
        let major = if a.x >= a.y && a.x >= a.z {
            n.x
        } else if a.y >= a.z {
            n.y
        } else {
            n.z
        };
        if major < 0.0 {
            Self { n: -n, d: -self.d }
        } else {
            self
        }
    }

    pub fn dist(&self, p: DVec3) -> f64 {
        self.n.dot(p) - self.d
    }

    pub fn contains(&self, p: DVec3) -> bool {
        self.dist(p).abs() < TOL
    }

    /// Mesma superfície, independentemente do sentido da normal.
    pub fn same_surface(&self, o: &Plane) -> bool {
        let dot = self.n.dot(o.n);
        (dot > 1.0 - 1e-9 && (self.d - o.d).abs() < TOL)
            || (dot < -(1.0 - 1e-9) && (self.d + o.d).abs() < TOL)
    }

    /// Base ortonormal (u, v) com u × v = n. Em paredes, v aponta para cima.
    pub fn basis(&self) -> (DVec3, DVec3) {
        basis_for(self.n)
    }

    pub fn to_2d(self, p: DVec3, basis: (DVec3, DVec3)) -> DVec2 {
        DVec2::new(p.dot(basis.0), p.dot(basis.1))
    }

    pub fn project(&self, p: DVec3) -> DVec3 {
        p - self.n * self.dist(p)
    }
}

pub fn basis_for(n: DVec3) -> (DVec3, DVec3) {
    if n.z.abs() < 0.999 {
        let v = (DVec3::Z - n * n.z).normalize();
        (v.cross(n), v)
    } else {
        let u = (DVec3::X - n * n.x).normalize();
        (u, n.cross(u))
    }
}

/// Área com sinal (positiva = anti-horário).
pub fn signed_area(poly: &[DVec2]) -> f64 {
    let mut a = 0.0;
    for i in 0..poly.len() {
        let p = poly[i];
        let q = poly[(i + 1) % poly.len()];
        a += p.x * q.y - q.x * p.y;
    }
    a * 0.5
}

/// Ponto estritamente dentro do polígono (regra par-ímpar).
pub fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Triangula um polígono com furos. Índices referem-se à lista `outer ++ holes...`.
pub fn triangulate(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Vec<[usize; 3]> {
    let mut coords = Vec::with_capacity((outer.len() + holes.iter().map(Vec::len).sum::<usize>()) * 2);
    let mut hole_idx = Vec::with_capacity(holes.len());
    for p in outer {
        coords.extend([p.x, p.y]);
    }
    for h in holes {
        hole_idx.push(coords.len() / 2);
        for p in h {
            coords.extend([p.x, p.y]);
        }
    }
    match earcutr::earcut(&coords, &hole_idx, 2) {
        Ok(idx) => idx.as_chunks::<3>().0.iter().map(|t| [t[0], t[1], t[2]]).collect(),
        Err(_) => Vec::new(),
    }
}
