// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Operações de modelagem sobre a malha: Empurrar/Puxar, Mover, Copiar, Excluir.

use std::collections::HashSet;

use glam::{DAffine3, DVec3};

use super::mesh::{EdgeId, Face, FaceId, Mesh, VertId};
use super::plane::Plane;
use super::TOL;

/// Normal de Newell de um contorno.
fn newell(pts: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n
}

/// Desloca um contorno plano (anti-horário em torno de `n`) para dentro (`dist > 0`)
/// ou para fora, mantendo os ângulos (cantos em quina).
pub fn offset_loop(pts: &[DVec3], n: DVec3, dist: f64) -> Result<Vec<DVec3>, &'static str> {
    let k = pts.len();
    if k < 3 {
        return Err("Contorno inválido.");
    }
    // Para dentro = à esquerda de cada aresta (contorno anti-horário em torno de n).
    let inward = |i: usize| {
        let d = (pts[(i + 1) % k] - pts[i]).normalize_or_zero();
        n.cross(d)
    };
    let mut out = Vec::with_capacity(k);
    for i in 0..k {
        let prev = (i + k - 1) % k;
        let (d0, d1) = (
            (pts[i] - pts[prev]).normalize_or_zero(),
            (pts[(i + 1) % k] - pts[i]).normalize_or_zero(),
        );
        let (n0, n1) = (inward(prev), inward(i));
        // Interseção das duas retas deslocadas: pts[i] + n0*dist + d0*s = pts[i] + n1*dist + d1*t
        let cross = d0.cross(d1).dot(n);
        let p = if cross.abs() < 1e-9 {
            pts[i] + n1 * dist // arestas colineares
        } else {
            let w = (n1 - n0) * dist;
            let s = w.cross(d1).dot(n) / cross;
            pts[i] + n0 * dist + d0 * s
        };
        out.push(p);
    }
    // Recusa contornos invertidos (distância grande demais).
    for i in 0..k {
        let a = pts[(i + 1) % k] - pts[i];
        let b = out[(i + 1) % k] - out[i];
        if a.dot(b) <= 0.0 || b.length() < TOL {
            return Err("Distância grande demais para esta face.");
        }
    }
    Ok(out)
}

impl Mesh {
    pub fn face_edges(&self, f: FaceId) -> Vec<EdgeId> {
        self.faces[f]
            .directed_edges()
            .filter_map(|(a, b)| self.edge_between(a, b))
            .collect()
    }

    pub fn face_vertices(&self, f: FaceId) -> HashSet<VertId> {
        self.faces[f].loops().flatten().copied().collect()
    }

    pub fn face_area(&self, f: FaceId) -> f64 {
        self.face_triangles(f)
            .iter()
            .map(|[a, b, c]| (*b - *a).cross(*c - *a).length() * 0.5)
            .sum()
    }

    pub fn edge_length(&self, e: EdgeId) -> f64 {
        let (a, b) = self.edge_points(e);
        a.distance(b)
    }

    /// Remove só as faces (as arestas ficam).
    pub fn delete_faces(&mut self, faces: &[FaceId]) {
        for f in faces {
            self.faces.remove(*f);
        }
    }

    /// Tudo o que está conectado aos vértices dados (clique triplo).
    pub fn connected(&self, seeds: impl IntoIterator<Item = VertId>) -> (HashSet<EdgeId>, HashSet<FaceId>) {
        let mut verts: HashSet<VertId> = HashSet::new();
        let mut stack: Vec<VertId> = seeds.into_iter().collect();
        let mut edges = HashSet::new();
        while let Some(v) = stack.pop() {
            if !verts.insert(v) {
                continue;
            }
            for &e in self.edges_of(v) {
                edges.insert(e);
                for w in self.edges[e].v {
                    if !verts.contains(&w) {
                        stack.push(w);
                    }
                }
            }
        }
        let faces = self
            .faces
            .iter()
            .filter(|(_, f)| f.outer.iter().all(|v| verts.contains(v)))
            .map(|(id, _)| id)
            .collect();
        (edges, faces)
    }

    /// Garante que a normal da face aponte para o lado de `dir`.
    pub fn orient_toward(&mut self, f: FaceId, dir: DVec3) {
        if let Some(face) = self.faces.get_mut(f)
            && face.normal.dot(dir) < 0.0 {
                face.reverse();
            }
    }

    /// Desloca vértices. Devolve `false` se alguma face deixar de ser plana.
    pub fn translate(&mut self, verts: &HashSet<VertId>, delta: DVec3) -> bool {
        self.transform(verts, DAffine3::from_translation(delta))
    }

    /// Aplica uma transformação (mover, girar, escalar) aos vértices.
    /// Devolve `false` se alguma face deixar de ser plana ou degenerar.
    pub fn transform(&mut self, verts: &HashSet<VertId>, xf: DAffine3) -> bool {
        for v in verts {
            if let Some(p) = self.verts.get_mut(*v) {
                *p = xf.transform_point3(*p);
            }
        }
        let touched: Vec<FaceId> = self
            .faces
            .iter()
            .filter(|(_, f)| f.loops().flatten().any(|v| verts.contains(v)))
            .map(|(id, _)| id)
            .collect();
        let mut valid = true;
        for f in touched {
            let all_moved = self.faces[f].loops().flatten().all(|v| verts.contains(v));
            // Referência de sentido: a normal antiga transformada (faces movidas por inteiro)
            // ou a própria normal antiga (faces esticadas).
            let reference = if all_moved {
                xf.matrix3.inverse().transpose() * self.faces[f].normal
            } else {
                self.faces[f].normal
            };
            let pts: Vec<DVec3> = self.faces[f].outer.iter().map(|v| self.verts[*v]).collect();
            let n = newell(&pts);
            if n.length() < 1e-12 {
                valid = false;
                continue;
            }
            let mut n = n.normalize();
            if n.dot(reference) < 0.0 {
                n = -n;
            }
            self.faces[f].normal = n;
            let plane = Plane::from_point_normal(pts[0], n);
            if self.faces[f].loops().flatten().any(|v| !plane.contains(self.verts[*v])) {
                valid = false;
            }
        }
        valid
    }

    /// Copia arestas deslocadas por `delta` (as faces fechadas se formam de novo).
    #[cfg(test)]
    pub fn copy_translated(&mut self, edges: &HashSet<EdgeId>, delta: DVec3, toward_viewer: DVec3) {
        self.copy_transformed(edges, DAffine3::from_translation(delta), toward_viewer);
    }

    /// Cópia da malha com todos os vértices transformados (para desenhar instâncias).
    pub fn transformed(&self, xf: DAffine3) -> Mesh {
        let mut m = self.clone();
        for p in m.verts.values_mut() {
            *p = xf.transform_point3(*p);
        }
        let nm = xf.matrix3.inverse().transpose();
        // Transformação espelhada inverte o sentido dos contornos.
        let mirrored = xf.matrix3.determinant() < 0.0;
        for f in m.faces.values_mut() {
            f.normal = (nm * f.normal).normalize_or(f.normal);
            if mirrored {
                f.outer.reverse();
                for h in &mut f.holes {
                    h.reverse();
                }
            }
        }
        for g in &mut m.guides {
            g.point = xf.transform_point3(g.point);
            g.dir = xf.transform_vector3(g.dir).normalize_or(g.dir);
        }
        m
    }

    /// Separa as entidades indicadas numa malha nova (para Criar grupo/componente).
    /// Arestas que ainda sustentam faces que ficaram continuam na malha original.
    pub fn extract(&mut self, edges: &HashSet<EdgeId>, faces: &HashSet<FaceId>) -> Mesh {
        let mut all_edges: HashSet<EdgeId> = edges.iter().copied().filter(|e| self.edges.contains_key(*e)).collect();
        for f in faces {
            if self.faces.contains_key(*f) {
                all_edges.extend(self.face_edges(*f));
            }
        }
        // Monta a malha nova por listas (preserva faces, orientação e material).
        let mut index: std::collections::HashMap<VertId, usize> = std::collections::HashMap::new();
        let mut verts: Vec<DVec3> = Vec::new();
        let mut idx = |v: VertId| -> usize {
            *index.entry(v).or_insert_with(|| {
                verts.push(self.verts[v]);
                verts.len() - 1
            })
        };
        let mut raw_edges = Vec::new();
        let mut soft = Vec::new();
        for e in &all_edges {
            let [a, b] = self.edges[*e].v;
            if self.edges[*e].soft {
                soft.push(raw_edges.len());
            }
            raw_edges.push([idx(a), idx(b)]);
        }
        let mut raw_faces = Vec::new();
        let mut face_textures = Vec::new();
        for f in faces.iter().filter(|f| self.faces.contains_key(**f)) {
            let face = &self.faces[*f];
            let outer: Vec<usize> = face.outer.iter().map(|v| idx(*v)).collect();
            let mut holes = Vec::new();
            for h in &face.holes {
                holes.push(h.iter().map(|v| idx(*v)).collect::<Vec<usize>>());
            }
            raw_faces.push((outer, holes, face.normal, face.material));
            face_textures.push(face.texture);
        }
        let out = Mesh::from_raw(&super::mesh::RawMesh {
            verts,
            edges: raw_edges,
            soft,
            faces: raw_faces,
            face_textures,
            guides: Vec::new(),
        })
        .unwrap_or_default();

        // Remove da original: as faces e as arestas que não sustentam mais nenhuma face.
        let fs: Vec<FaceId> = faces.iter().copied().collect();
        self.delete_faces(&fs);
        let orphan: Vec<EdgeId> = all_edges.into_iter().filter(|e| self.faces_of_edge(*e).is_empty()).collect();
        self.erase_edges(&orphan);
        out
    }

    /// Incorpora outra malha (transformada) a esta, com aderência normal da geometria
    /// (Explodir). Faces mantêm material e orientação; `fallback` pinta as sem material.
    pub fn merge_from(
        &mut self,
        other: &Mesh,
        xf: DAffine3,
        fallback: Option<[f32; 3]>,
        fallback_texture: Option<u32>,
        toward_viewer: DVec3,
    ) {
        let src = other.transformed(xf);
        let segs: Vec<(DVec3, DVec3)> = src.edges.keys().map(|e| src.edge_points(e)).collect();
        self.add_segments(&segs, toward_viewer);
        for f in src.faces.keys() {
            let face = &src.faces[f];
            let c = face.outer.iter().map(|v| src.verts[*v]).sum::<DVec3>() / face.outer.len() as f64;
            let n = face.normal;
            let found = self.faces.keys().find(|g| {
                let gf = &self.faces[*g];
                let gc = gf.outer.iter().map(|v| self.verts[*v]).sum::<DVec3>() / gf.outer.len() as f64;
                gc.distance(c) < 1e-6 && gf.normal.dot(n).abs() > 0.999
            });
            if let Some(g) = found {
                self.orient_toward(g, n);
                self.faces[g].material = face.material.or(fallback);
                self.faces[g].texture = if face.material.is_some() { face.texture } else { fallback_texture };
            }
        }
    }

    /// Copia arestas transformadas. Faces copiadas mantêm material e orientação.
    pub fn copy_transformed(&mut self, edges: &HashSet<EdgeId>, xf: DAffine3, toward_viewer: DVec3) {
        let edges: HashSet<EdgeId> = edges.iter().copied().filter(|e| self.edges.contains_key(*e)).collect();
        // Faces de origem: todas as bordas dentro da cópia.
        let sources: Vec<(DVec3, DVec3, (Option<[f32; 3]>, Option<u32>))> = self
            .faces
            .keys()
            .filter(|f| self.face_edges(*f).iter().all(|e| edges.contains(e)))
            .map(|f| {
                let c = self.faces[f].outer.iter().map(|v| self.verts[*v]).sum::<DVec3>()
                    / self.faces[f].outer.len() as f64;
                let n = (xf.matrix3.inverse().transpose() * self.faces[f].normal).normalize();
                (xf.transform_point3(c), n, (self.faces[f].material, self.faces[f].texture))
            })
            .collect();
        let segs: Vec<(DVec3, DVec3)> = edges
            .iter()
            .map(|e| {
                let (a, b) = self.edge_points(*e);
                (xf.transform_point3(a), xf.transform_point3(b))
            })
            .collect();
        self.add_segments(&segs, toward_viewer);
        for (c, n, (material, texture)) in sources {
            let found = self.faces.keys().find(|f| {
                let face = &self.faces[*f];
                let fc = face.outer.iter().map(|v| self.verts[*v]).sum::<DVec3>() / face.outer.len() as f64;
                fc.distance(c) < 1e-6 && face.normal.dot(n).abs() > 0.999
            });
            if let Some(f) = found {
                self.orient_toward(f, n);
                self.faces[f].material = material;
                self.faces[f].texture = texture;
            }
        }
    }

    /// Equidistância (Offset): novo contorno a `dist` metros da borda externa da face,
    /// para dentro (positivo) ou para fora (negativo).
    pub fn offset_face(&mut self, f: FaceId, dist: f64, toward_viewer: DVec3) -> Result<(), &'static str> {
        if dist.abs() < TOL {
            return Err("Distância nula.");
        }
        let Some(face) = self.faces.get(f) else {
            return Err("Face inexistente.");
        };
        let loop_pts = offset_loop(&face.outer.iter().map(|v| self.verts[*v]).collect::<Vec<_>>(), face.normal, dist)?;
        self.add_polyline(&loop_pts, true, toward_viewer);
        Ok(())
    }

    /// Espessura do sólido atrás de um ponto da face: distância até a face paralela
    /// oposta (a outra face da parede), procurando para trás da normal `n`.
    pub fn thickness_behind(&self, p: DVec3, n: DVec3) -> Option<f64> {
        let mut best: Option<f64> = None;
        for f in self.faces.keys() {
            let face = &self.faces[f];
            if face.normal.dot(n) > -0.999 {
                continue; // só faces paralelas viradas para o lado oposto
            }
            let plane = self.face_plane(f);
            let d = (p - self.verts[face.outer[0]]).dot(n);
            if d <= TOL {
                continue; // tem que estar atrás
            }
            let q = p - n * d;
            let basis = plane.basis();
            let poly: Vec<_> = face.outer.iter().map(|v| plane.to_2d(self.verts[*v], basis)).collect();
            let q2 = plane.to_2d(q, basis);
            let in_hole = face.holes.iter().any(|h| {
                let hp: Vec<_> = h.iter().map(|v| plane.to_2d(self.verts[*v], basis)).collect();
                super::plane::point_in_polygon(q2, &hp)
            });
            if super::plane::point_in_polygon(q2, &poly) && !in_hole && best.is_none_or(|b| d < b) {
                best = Some(d);
            }
        }
        best
    }

    /// Abre um vão (porta/janela) atravessando a parede: desenha o contorno na face e
    /// empurra até a face oposta. Devolve a espessura atravessada.
    pub fn cut_opening(&mut self, outline: &[DVec3], n: DVec3) -> Result<f64, &'static str> {
        let center = outline.iter().sum::<DVec3>() / outline.len() as f64;
        let thickness = self.thickness_behind(center, n).ok_or("Não há parede atrás do contorno.")?;

        // Lados do contorno que encostam na borda da parede (ex.: a base de uma porta):
        // a lateral do vão nesses lados não deve virar face (não sobra "soleira").
        let wall = self.faces.keys().find(|f| {
            let face = &self.faces[*f];
            let plane = self.face_plane(*f);
            if face.normal.dot(n) < 0.999 || !plane.contains(center) {
                return false;
            }
            let b = plane.basis();
            let poly: Vec<_> = face.outer.iter().map(|v| plane.to_2d(self.verts[*v], b)).collect();
            super::plane::point_in_polygon(plane.to_2d(center, b), &poly)
        });
        let k = outline.len();
        let on_border: Vec<bool> = (0..k)
            .map(|i| {
                let Some(w) = wall else { return false };
                let (a, b) = (outline[i], outline[(i + 1) % k]);
                let mid = (a + b) * 0.5;
                let mut out = n.cross(b - a).normalize_or_zero();
                if out.dot(mid - center) < 0.0 {
                    out = -out;
                }
                let probe = mid + out * 1e-4;
                let plane = self.face_plane(w);
                let basis = plane.basis();
                let poly: Vec<_> = self.faces[w].outer.iter().map(|v| plane.to_2d(self.verts[*v], basis)).collect();
                !super::plane::point_in_polygon(plane.to_2d(probe, basis), &poly)
            })
            .collect();

        // Contorno na frente e no fundo; as duas faces do vão somem.
        let back: Vec<DVec3> = outline.iter().map(|p| *p - n * thickness).collect();
        self.add_polyline(outline, true, n);
        self.add_polyline(&back, true, -n);
        let find_quad = |m: &Mesh, pts: &[DVec3]| -> Vec<FaceId> {
            let c = pts.iter().sum::<DVec3>() / pts.len() as f64;
            m.faces
                .keys()
                .filter(|g| {
                    let face = &m.faces[*g];
                    let fc = face.outer.iter().map(|v| m.verts[*v]).sum::<DVec3>() / face.outer.len() as f64;
                    face.holes.is_empty()
                        && fc.distance(c) < 1e-6
                        && face.outer.iter().all(|v| pts.iter().any(|q| q.distance(m.verts[*v]) < TOL))
                })
                .collect()
        };
        let mut gone = find_quad(self, outline);
        gone.extend(find_quad(self, &back));
        if gone.len() < 2 {
            return Err("O contorno não formou o vão na parede.");
        }
        self.delete_faces(&gone);

        // Requadro: liga frente e fundo; as faces laterais se formam sozinhas.
        let segs: Vec<(DVec3, DVec3)> = outline.iter().zip(back.iter()).map(|(a, b)| (*a, *b)).collect();
        self.add_segments(&segs, n);
        for i in 0..k {
            let (a, b) = (outline[i], outline[(i + 1) % k]);
            let quad = [a, b, back[(i + 1) % k], back[i]];
            let sides = find_quad(self, &quad);
            if on_border[i] {
                // Coincide com a base/topo/lateral da parede: o vão fica aberto ali.
                self.delete_faces(&sides);
            } else {
                // Requadro virado para dentro do vão.
                let qc = quad.iter().sum::<DVec3>() / 4.0;
                for f in sides {
                    self.orient_toward(f, center - n * thickness * 0.5 - qc);
                }
            }
        }
        Ok(thickness)
    }

    /// Siga-me: varre o contorno da face `profile` ao longo do caminho `path`
    /// (rodapés, sancas, molduras, corrimãos). Em cada dobra do caminho o perfil é
    /// projetado no plano bissetor (meia-esquadria). Num caminho fechado o perfil
    /// original é consumido; num aberto ele vira a tampa inicial.
    pub fn follow_me(&mut self, profile: FaceId, path: &[DVec3], closed: bool, toward_viewer: DVec3) -> Result<(), &'static str> {
        let face = self.faces.get(profile).ok_or("Perfil inexistente.")?.clone();
        let prof: Vec<DVec3> = face.outer.iter().map(|v| self.verts[*v]).collect();
        let mut path: Vec<DVec3> = path.to_vec();
        path.dedup_by(|a, b| a.distance(*b) < TOL);
        if closed && path.len() > 2 && path[0].distance(*path.last().unwrap()) < TOL {
            path.pop();
        }
        let n = path.len();
        if n < 2 || (closed && n < 3) {
            return Err("O caminho precisa de pelo menos dois pontos.");
        }
        let segs_n = if closed { n } else { n - 1 };
        let dirs: Vec<DVec3> = (0..segs_n).map(|i| (path[(i + 1) % n] - path[i]).normalize()).collect();
        let bisector = |a: DVec3, b: DVec3| {
            let m = a + b;
            if m.length() < 1e-9 { a } else { m.normalize() }
        };
        // Projeta um anel ao longo de `d` sobre o plano (ponto `at`, normal `nrm`).
        let project = |ring: &[DVec3], d: DVec3, at: DVec3, nrm: DVec3| -> Result<Vec<DVec3>, &'static str> {
            let dn = d.dot(nrm);
            if dn.abs() < 1e-6 {
                return Err("O caminho dobra demais (volta sobre si mesmo).");
            }
            Ok(ring.iter().map(|q| *q + d * ((at - *q).dot(nrm) / dn)).collect())
        };

        let mut rings: Vec<Vec<DVec3>> = Vec::new();
        let first = if closed {
            project(&prof, dirs[0], path[0], bisector(dirs[n - 1], dirs[0]))?
        } else {
            prof.clone()
        };
        rings.push(first);
        for i in 0..segs_n {
            let next = (i + 1) % n;
            if closed && next == 0 {
                break; // fecha no primeiro anel
            }
            let nrm = if !closed && next == n - 1 { dirs[i] } else { bisector(dirs[i], dirs[next % segs_n]) };
            let ring = project(rings.last().unwrap(), dirs[i], path[next], nrm)?;
            rings.push(ring);
        }

        let k = prof.len();
        let mut segs = Vec::new();
        for ring in &rings {
            for j in 0..k {
                segs.push((ring[j], ring[(j + 1) % k]));
            }
        }
        let pairs = if closed { rings.len() } else { rings.len() - 1 };
        for r in 0..pairs {
            let (a, b) = (&rings[r], &rings[(r + 1) % rings.len()]);
            for j in 0..k {
                segs.push((a[j], b[j]));
            }
        }
        if closed {
            // O perfil original sai antes (seu lugar é ocupado pelo primeiro anel e as
            // arestas dele cortariam as faces da varredura).
            let pe = self.face_edges(profile);
            self.faces.remove(profile);
            let orphan: Vec<EdgeId> = pe.into_iter().filter(|e| self.faces_of_edge(*e).is_empty()).collect();
            self.erase_edges(&orphan);
        }
        self.add_segments(&segs, toward_viewer);

        // Os anéis intermediários (meias-esquadrias) fecham contornos e viram faces
        // dentro do sólido: saem. Num caminho aberto ficam a tampa inicial e a final.
        let keep = |r: usize| !closed && (r == 0 || r == rings.len() - 1);
        for (r, ring) in rings.iter().enumerate() {
            if keep(r) {
                continue;
            }
            let inner: Vec<FaceId> = self
                .faces
                .keys()
                .filter(|f| {
                    let o = &self.faces[*f].outer;
                    o.len() == k && o.iter().all(|v| ring.iter().any(|q| q.distance(self.verts[*v]) < TOL))
                })
                .collect();
            self.delete_faces(&inner);
        }

        // Faces coplanares vizinhas (ex.: o topo de dois trechos retos) se juntam;
        // curvas do caminho ou do perfil ficam lisas (arestas suaves).
        let on_sweep = |p: DVec3| rings.iter().flatten().any(|q| q.distance(p) < TOL);
        let smooth = 40f64.to_radians().cos();
        let mut merge = Vec::new();
        let edges: Vec<EdgeId> = self.edges.keys().collect();
        for e in &edges {
            let (a, b) = self.edge_points(*e);
            if !(on_sweep(a) && on_sweep(b)) {
                continue;
            }
            let fs = self.faces_of_edge(*e);
            if fs.len() != 2 {
                continue;
            }
            let dot = self.faces[fs[0]].normal.dot(self.faces[fs[1]].normal);
            if dot > 1.0 - 1e-9 && self.face_plane(fs[0]).same_surface(&self.face_plane(fs[1])) {
                merge.push(*e);
            } else if dot > smooth {
                self.edges[*e].soft = true;
            }
        }
        self.erase_edges(&merge);
        // Num caminho fechado o contorno interno também fecha e vira uma face que só
        // preenche o "furo" de outra (ex.: o teto inteiro dentro da sanca): sai.
        let filler: Vec<FaceId> = self
            .faces
            .keys()
            .filter(|f| {
                let o: HashSet<VertId> = self.faces[*f].outer.iter().copied().collect();
                o.iter().all(|v| on_sweep(self.verts[*v]))
                    && self.faces.values().any(|g| g.holes.iter().any(|h| h.len() == o.len() && h.iter().all(|v| o.contains(v))))
            })
            .collect();
        self.delete_faces(&filler);
        // Com as faces internas fora, o sólido fecha: garante as faces para fora.
        let sweep_faces: Vec<FaceId> = self
            .faces
            .keys()
            .filter(|f| self.faces[*f].outer.iter().all(|v| on_sweep(self.verts[*v])))
            .collect();
        self.orient_closed_shells(&sweep_faces);
        Ok(())
    }

    /// Empurrar/Puxar: desloca a face `d` metros ao longo da sua normal.
    pub fn push_pull(&mut self, f: FaceId, d: f64, toward_viewer: DVec3) -> Result<(), &'static str> {
        if d.abs() < TOL {
            return Err("Distância nula.");
        }
        let Some(face) = self.faces.get(f).cloned() else {
            return Err("Face inexistente.");
        };
        let n = face.normal;
        let off = n * d;
        let border = self.face_edges(f);

        // 1) Vizinhas perpendiculares: a face só desliza e as vizinhas esticam.
        let stretch = !border.is_empty()
            && border.iter().all(|e| {
                let others: Vec<FaceId> = self.faces_of_edge(*e).into_iter().filter(|g| *g != f).collect();
                others.len() == 1 && self.faces[others[0]].normal.dot(n).abs() < 1e-9
            });
        if stretch {
            let fverts = self.face_vertices(f);
            for &v in &fverts {
                for &e in self.edges_of(v) {
                    let [a, b] = self.edges[e].v;
                    let w = if a == v { b } else { a };
                    if fverts.contains(&w) {
                        continue;
                    }
                    let dv = self.verts[w] - self.verts[v];
                    let along = dv.dot(n);
                    let parallel = dv.cross(n).length() < 1e-9 * dv.length().max(1.0);
                    if parallel && along * d > 0.0 && d.abs() >= along.abs() - TOL {
                        return Err("A face atravessaria a geometria.");
                    }
                }
            }
            self.translate(&fverts, off);
            return Ok(());
        }

        // 2) Extrusão: laterais + tampa.
        let free = border.iter().all(|e| self.faces_of_edge(*e).len() == 1);
        let cap_plane = Plane::from_point_normal(self.verts[face.outer[0]] + off, n);
        // Vão passante: a tampa cai dentro de uma face existente (ex.: o outro lado da parede).
        let cap_center = face.outer.iter().map(|v| self.verts[*v]).sum::<DVec3>() / face.outer.len() as f64 + off;
        let through = self.faces.keys().any(|g| {
            if g == f || !self.face_plane(g).same_surface(&cap_plane) {
                return false;
            }
            let basis = cap_plane.basis();
            let to2 = |vs: &[VertId]| -> Vec<glam::DVec2> {
                vs.iter().map(|v| cap_plane.to_2d(self.verts[*v], basis)).collect()
            };
            let c = cap_plane.to_2d(cap_center, basis);
            let gf = &self.faces[g];
            super::plane::point_in_polygon(c, &to2(&gf.outer))
                && !gf.holes.iter().any(|h| super::plane::point_in_polygon(c, &to2(h)))
        });
        let loops: Vec<Vec<VertId>> = face.loops().cloned().collect();
        let base_border: Vec<(VertId, VertId)> = face.directed_edges().collect();
        let cap_outer: Vec<DVec3> = face.outer.iter().map(|v| self.verts[*v] + off).collect();

        self.faces.remove(f);
        let mut segs = Vec::new();
        for l in &loops {
            for i in 0..l.len() {
                let (a, b) = (self.verts[l[i]], self.verts[l[(i + 1) % l.len()]]);
                segs.push((a + off, b + off));
                segs.push((a, a + off));
            }
        }
        self.add_segments(&segs, toward_viewer);
        let sgn = d.signum();

        // Base: se a face era solta, fica uma cópia no lugar (o sólido fecha).
        if free {
            let mut base = Face {
                outer: face.outer.clone(),
                holes: face.holes.clone(),
                normal: n,
                material: face.material,
                texture: face.texture,
            };
            if sgn > 0.0 {
                base.reverse();
            }
            self.faces.insert(base);
        }

        // Orientação, pelo lado que o usuário está vendo (frente = voltada para o observador):
        // * face solta: sólido fechado com tudo para fora;
        // * puxar em direção ao observador: volume com laterais para fora;
        // * empurrar para longe dele: rebaixo, com laterais voltadas para dentro do vão.
        let front = if n.dot(toward_viewer) >= 0.0 { n } else { -n };
        let toward_front = (n * d).dot(front) > 0.0;
        let cap_dir = if free { n * sgn } else { front };
        let wall_sign = if free || toward_front { 1.0 } else { -1.0 };
        let matches_cap = |m: &Mesh, g: FaceId| {
            let fo = &m.faces[g].outer;
            fo.len() == cap_outer.len()
                && cap_outer
                    .iter()
                    .all(|p| fo.iter().any(|v| m.verts[*v].distance(*p) < TOL))
        };
        let cap = self.faces.keys().find(|g| matches_cap(self, *g));
        if let Some(cap) = cap {
            if through {
                self.faces.remove(cap);
            } else {
                self.orient_toward(cap, cap_dir);
                self.faces[cap].material = face.material;
                self.faces[cap].texture = face.texture;
            }
        }
        for &(a, b) in &base_border {
            let (pa, pb) = (self.verts[a], self.verts[b]);
            // (b - a) × n aponta para fora do contorno (contorno anti-horário em torno de n).
            let outward = (pb - pa).cross(n) * wall_sign;
            // A lateral tem a aresta da base no contorno EXTERNO (uma face vizinha pode ter
            // essa mesma aresta no contorno de um furo, e não deve ser reorientada).
            let side = self.faces.iter().find(|(_, g)| {
                let o = &g.outer;
                let has_base = (0..o.len()).any(|i| {
                    let (p, q) = (o[i], o[(i + 1) % o.len()]);
                    (p == a && q == b) || (p == b && q == a)
                });
                has_base && o.iter().any(|v| self.verts[*v].distance(pa + off) < TOL)
            });
            if let Some((sid, _)) = side {
                self.orient_toward(sid, outward);
            }
        }

        // Laterais coplanares com faces vizinhas se juntam (sem linha entre elas).
        let mut merge = Vec::new();
        for &(a, b) in &base_border {
            if let Some(e) = self.edge_between(a, b) {
                let fs = self.faces_of_edge(e);
                // Só junta faces que apontam para o mesmo lado: uma lateral encostada de
                // costas numa face vizinha (puxador na porta) não pode herdar o lado dela.
                let same_side = fs.len() == 2 && self.faces[fs[0]].normal.dot(self.faces[fs[1]].normal) > 0.0;
                if same_side && self.face_plane(fs[0]).same_surface(&self.face_plane(fs[1])) {
                    merge.push(e);
                }
            }
        }
        if !merge.is_empty() {
            self.erase_edges(&merge);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64, z: f64) -> Vec<DVec3> {
        vec![
            DVec3::new(x0, y0, z),
            DVec3::new(x1, y0, z),
            DVec3::new(x1, y1, z),
            DVec3::new(x0, y1, z),
        ]
    }

    fn face_at(m: &Mesh, pred: impl Fn(&Mesh, FaceId) -> bool) -> FaceId {
        m.faces.keys().find(|f| pred(m, *f)).expect("face não encontrada")
    }

    fn centroid(m: &Mesh, f: FaceId) -> DVec3 {
        let o = &m.faces[f].outer;
        o.iter().map(|v| m.verts[*v]).sum::<DVec3>() / o.len() as f64
    }

    fn assert_outward(m: &Mesh, center: DVec3) {
        for f in m.faces.keys() {
            assert!(
                m.faces[f].normal.dot(centroid(m, f) - center) > 0.0,
                "face virada para dentro em {:?}",
                centroid(m, f)
            );
        }
    }

    #[test]
    fn pull_free_rectangle_makes_closed_box() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 1.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        // Face no chão aponta para baixo: puxar para cima é distância negativa na normal.
        m.push_pull(f, -0.9, DVec3::new(-1.0, -1.0, 1.0)).unwrap();
        assert_eq!(m.faces.len(), 6);
        assert_eq!(m.edges.len(), 12);
        assert_outward(&m, DVec3::new(1.0, 0.5, 0.45));
    }

    #[test]
    fn push_box_top_stretches_without_new_faces() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 1.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        m.push_pull(f, -1.0, DVec3::Z).unwrap();
        let top = face_at(&m, |m, f| centroid(m, f).z > 0.99);
        m.push_pull(top, 0.5, DVec3::Z).unwrap();
        assert_eq!(m.faces.len(), 6);
        assert_eq!(m.edges.len(), 12);
        let max_z = m.verts.values().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((max_z - 1.5).abs() < 1e-9);
        // Empurrar para baixo também estica.
        let top = face_at(&m, |m, f| centroid(m, f).z > 1.49);
        m.push_pull(top, -1.2, DVec3::Z).unwrap();
        let max_z = m.verts.values().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((max_z - 0.3).abs() < 1e-9);
        // Atravessar o fundo é recusado.
        let top = face_at(&m, |m, f| centroid(m, f).z > 0.29);
        assert!(m.push_pull(top, -0.3, DVec3::Z).is_err());
    }

    #[test]
    fn pull_rectangle_inside_floor_leaves_hole_under_box() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 5.0, 4.0, 0.0), true, DVec3::Z);
        m.add_polyline(&rect(1.0, 1.0, 2.0, 2.0, 0.0), true, DVec3::Z);
        let inner = face_at(&m, |m, f| m.faces[f].holes.is_empty() && m.face_area(f) < 1.5);
        let n = m.faces[inner].normal;
        m.push_pull(inner, 0.75 / n.z, DVec3::Z).unwrap(); // 0,75 m para cima
        // piso com furo + 4 laterais + tampa
        assert_eq!(m.faces.len(), 6);
        assert!(m.faces.values().any(|f| f.holes.len() == 1));
        let top = face_at(&m, |m, f| centroid(m, f).z > 0.7);
        assert!(m.faces[top].normal.z > 0.99);
        for f in m.faces.keys() {
            let c = centroid(&m, f);
            if c.z > 0.01 && c.z < 0.7 {
                assert!(m.faces[f].normal.dot(c - DVec3::new(1.5, 1.5, c.z)) > 0.0);
            }
        }
    }

    #[test]
    fn push_down_into_floor_makes_pocket() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 5.0, 4.0, 0.0), true, DVec3::Z);
        m.add_polyline(&rect(1.0, 1.0, 2.0, 2.0, 0.0), true, DVec3::Z);
        let inner = face_at(&m, |m, f| m.faces[f].holes.is_empty() && m.face_area(f) < 1.5);
        let n = m.faces[inner].normal;
        m.push_pull(inner, -0.2 / n.z, DVec3::Z).unwrap(); // 0,2 m para baixo
        assert_eq!(m.faces.len(), 6);
        let bottom = face_at(&m, |m, f| centroid(m, f).z < -0.19);
        assert!(m.faces[bottom].normal.z > 0.99, "fundo do rebaixo voltado para cima");
    }

    #[test]
    fn push_window_through_wall_opens_it() {
        let mut m = Mesh::default();
        // parede: x 0..4, y 0..0.15, z 0..2.7
        m.add_polyline(&rect(0.0, 0.0, 4.0, 0.15, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        m.push_pull(f, -2.7, DVec3::new(0.0, -1.0, 0.0)).unwrap();
        assert_eq!(m.faces.len(), 6);
        // janela na face y = 0 (normal -Y)
        let w = vec![
            DVec3::new(1.0, 0.0, 1.0),
            DVec3::new(2.5, 0.0, 1.0),
            DVec3::new(2.5, 0.0, 2.2),
            DVec3::new(1.0, 0.0, 2.2),
        ];
        m.add_polyline(&w, true, DVec3::new(0.0, -1.0, 0.0));
        let win = face_at(&m, |m, f| m.faces[f].holes.is_empty() && (m.face_area(f) - 1.8).abs() < 1e-9);
        assert!(m.faces[win].normal.y < -0.99);
        m.push_pull(win, -0.15, DVec3::new(0.0, -1.0, 0.0)).unwrap();
        // 6 da parede (frente e fundo com furo) + 4 do requadro; nenhuma face no vão
        assert_eq!(m.faces.len(), 10);
        assert_eq!(m.faces.values().filter(|f| f.holes.len() == 1).count(), 2);
        let wall_area = 2.0 * (4.0 * 2.7 - 1.8) + 2.0 * 4.0 * 0.15 + 2.0 * 2.7 * 0.15 + 2.0 * (1.5 + 1.2) * 0.15;
        let total: f64 = m.faces.keys().map(|f| m.face_area(f)).sum();
        assert!((total - wall_area).abs() < 1e-9, "{total} != {wall_area}");
    }

    #[test]
    fn l_shaped_walls_pulled_to_same_height_keep_both_tops() {
        let mut m = Mesh::default();
        let up = DVec3::new(-1.0, -1.0, 1.0);
        m.add_polyline(&rect(-0.15, 4.0, 5.0, 4.15, 0.0), true, up);
        m.add_polyline(&rect(-0.15, 0.0, 0.0, 4.0, 0.0), true, up);
        for c in [DVec3::new(2.5, 4.075, 0.0), DVec3::new(-0.075, 2.0, 0.0)] {
            // (o contorno pode ter vértices extras onde as paredes se encostam)
            let f = face_at(&m, |m, f| {
                m.faces[f].normal.z.abs() > 0.99 && centroid(m, f).z.abs() < 1e-9 && centroid(m, f).distance(c) < 1.0
            });
            let nz = m.faces[f].normal.z;
            m.push_pull(f, 2.7 / nz, up).unwrap();
        }
        let tops: Vec<FaceId> = m.faces.keys().filter(|f| centroid(&m, *f).z > 2.69).collect();
        let area: f64 = tops.iter().map(|f| m.face_area(*f)).sum();
        let expected = 5.15 * 0.15 + 0.15 * 4.0;
        assert!((area - expected).abs() < 1e-9, "área dos topos {area} != {expected}");
        assert!(tops.iter().all(|f| m.faces[*f].normal.z > 0.99), "topo virado para baixo");
    }

    /// Parede 4 × 0,15 × 2,7 m (face da frente em y = 0, normal −Y).
    fn wall() -> Mesh {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 4.0, 0.15, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        let nz = m.faces[f].normal.z;
        m.push_pull(f, 2.7 / nz, DVec3::new(0.0, -1.0, 1.0)).unwrap();
        m
    }

    fn open_area(m: &Mesh) -> f64 {
        m.faces
            .keys()
            .filter(|f| m.faces[*f].normal.y.abs() > 0.99)
            .map(|f| m.face_area(f))
            .sum()
    }

    #[test]
    fn window_opening_through_wall() {
        let mut m = wall();
        let full = open_area(&m);
        let w = [
            DVec3::new(1.0, 0.0, 1.0),
            DVec3::new(2.2, 0.0, 1.0),
            DVec3::new(2.2, 0.0, 2.2),
            DVec3::new(1.0, 0.0, 2.2),
        ];
        assert!((m.thickness_behind(DVec3::new(1.5, 0.0, 1.5), -DVec3::Y).unwrap() - 0.15).abs() < 1e-9);
        let t = m.cut_opening(&w, -DVec3::Y).unwrap();
        assert!((t - 0.15).abs() < 1e-9);
        // frente e fundo perderam 1,2 × 1,2 cada
        assert!((open_area(&m) - (full - 2.0 * 1.44)).abs() < 1e-9);
    }

    #[test]
    fn door_opening_down_to_the_floor() {
        let mut m = wall();
        let full = open_area(&m);
        let d = [
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(1.9, 0.0, 0.0),
            DVec3::new(1.9, 0.0, 2.1),
            DVec3::new(1.0, 0.0, 2.1),
        ];
        m.cut_opening(&d, -DVec3::Y).unwrap();
        assert!((open_area(&m) - (full - 2.0 * 0.9 * 2.1)).abs() < 1e-9, "{}", open_area(&m));
        // a base da parede também foi recortada no vão (não sobra "soleira" fechando a porta)
        let bottom: f64 = m
            .faces
            .keys()
            .filter(|f| m.faces[*f].normal.z < -0.99 && m.verts[m.faces[*f].outer[0]].z.abs() < 1e-9)
            .map(|f| m.face_area(f))
            .sum();
        assert!((bottom - (4.0 - 0.9) * 0.15).abs() < 1e-9, "base {bottom}");
        // e a verga (topo do vão, a 2,10 m) existe e está virada para dentro do vão
        let head = m
            .faces
            .keys()
            .find(|f| {
                let face = &m.faces[*f];
                face.normal.z.abs() > 0.99 && face.outer.iter().all(|v| (m.verts[*v].z - 2.1).abs() < 1e-9)
            })
            .expect("verga");
        assert!(m.faces[head].normal.z < -0.99);
    }

    fn volume_and_outward(m: &Mesh) -> f64 {
        m.faces
            .keys()
            .flat_map(|f| m.face_triangles(f))
            .map(|[a, b, c]| a.dot(b.cross(c)) / 6.0)
            .sum()
    }

    /// Quadrado de lado `s` no plano YZ, centrado na origem (perfil para seguir +X).
    fn square_profile(m: &mut Mesh, s: f64) -> FaceId {
        let h = s / 2.0;
        m.add_polyline(
            &[DVec3::new(0.0, -h, -h), DVec3::new(0.0, h, -h), DVec3::new(0.0, h, h), DVec3::new(0.0, -h, h)],
            true,
            DVec3::new(-1.0, 0.0, 0.0),
        );
        m.faces.keys().next().unwrap()
    }

    #[test]
    fn follow_me_along_an_l_path() {
        let mut m = Mesh::default();
        let f = square_profile(&mut m, 0.2);
        let path = [DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 1.0, 0.0)];
        m.follow_me(f, &path, false, DVec3::new(-1.0, -1.0, 1.0)).unwrap();
        // 2 tampas + 4 lados em cada trecho; topo e base dos dois trechos se juntam
        assert_eq!(m.faces.len(), 8);
        // volume = área × comprimento da linha de centro (meia-esquadria conserva o volume)
        assert!((volume_and_outward(&m) - 0.04 * 3.0).abs() < 1e-9, "{}", volume_and_outward(&m));
        assert!(m.edges.values().all(|e| !e.soft), "dobra de 90° não é suavizada");
    }

    #[test]
    fn follow_me_around_a_room_like_a_crown_molding() {
        let mut m = Mesh::default();
        // perfil 5 × 10 cm no canto (0,0), parado no plano YZ
        m.add_polyline(
            &[DVec3::new(0.0, 0.0, 2.6), DVec3::new(0.0, 0.05, 2.6), DVec3::new(0.0, 0.05, 2.7), DVec3::new(0.0, 0.0, 2.7)],
            true,
            DVec3::new(-1.0, 0.0, 0.0),
        );
        let f = m.faces.keys().next().unwrap();
        let path = [DVec3::new(0.0, 0.0, 2.7), DVec3::new(4.0, 0.0, 2.7), DVec3::new(4.0, 3.0, 2.7), DVec3::new(0.0, 3.0, 2.7)];
        m.follow_me(f, &path, true, DVec3::new(-1.0, -1.0, 1.0)).unwrap();
        // anel fechado sem tampas nem o perfil solto: topo e base viram uma face cada
        // (com furo), mais as 4 faces externas e as 4 internas
        assert_eq!(m.faces.len(), 10);
        assert_eq!(m.faces.values().filter(|f| f.holes.len() == 1).count(), 2);
        assert!(volume_and_outward(&m) > 0.0, "faces para fora");
    }

    #[test]
    fn follow_me_round_profile_on_curved_path_is_smooth() {
        let mut m = Mesh::default();
        // perfil circular (12 lados) no plano YZ, raio 2 cm
        let prof: Vec<DVec3> = (0..12)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 12.0;
                DVec3::new(0.0, a.cos() * 0.02, a.sin() * 0.02)
            })
            .collect();
        m.add_polyline(&prof, true, DVec3::new(-1.0, 0.0, 0.0));
        let f = m.faces.keys().next().unwrap();
        // quarto de círculo de raio 1 m em 6 trechos
        let path: Vec<DVec3> = (0..=6)
            .map(|i| {
                let a = std::f64::consts::FRAC_PI_2 * i as f64 / 6.0;
                DVec3::new(a.sin(), 1.0 - a.cos(), 0.0)
            })
            .collect();
        m.follow_me(f, &path, false, DVec3::Z).unwrap();
        let soft = m.edges.values().filter(|e| e.soft).count();
        let hard = m.edges.values().filter(|e| !e.soft).count();
        assert!(soft > hard, "superfície lisa: {soft} suaves × {hard} visíveis");
        assert!(volume_and_outward(&m) > 0.0);
    }

    #[test]
    fn pull_rectangle_on_box_top_edge_merges_side() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 2.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        m.push_pull(f, -1.0, DVec3::Z).unwrap();
        // retângulo no tampo encostado na borda x = 0
        m.add_polyline(&rect(0.0, 0.5, 1.0, 1.5, 1.0), true, DVec3::Z);
        let small = face_at(&m, |m, f| centroid(m, f).z > 0.99 && (m.face_area(f) - 1.0).abs() < 1e-9);
        m.push_pull(small, 0.5, DVec3::Z).unwrap();
        // A lateral nova em x = 0 se junta à lateral da caixa (uma face só nesse plano).
        let in_x0 = m.faces.keys().filter(|f| m.face_plane(*f).same_surface(&Plane::from_point_normal(DVec3::ZERO, DVec3::X))).count();
        assert_eq!(in_x0, 1);
    }

    #[test]
    fn offset_inward_and_outward() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 2.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        m.offset_face(f, 0.5, DVec3::Z).unwrap();
        assert_eq!(m.faces.len(), 2);
        assert!(m.faces.keys().any(|f| (m.face_area(f) - 1.0).abs() < 1e-9));
        let total: f64 = m.faces.keys().map(|f| m.face_area(f)).sum();
        assert!((total - 4.0).abs() < 1e-9);

        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 2.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        m.offset_face(f, -0.5, DVec3::Z).unwrap();
        let total: f64 = m.faces.keys().map(|f| m.face_area(f)).sum();
        assert!((total - 9.0).abs() < 1e-9, "{total}");

        // grande demais
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 2.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        assert!(m.offset_face(f, 1.0, DVec3::Z).is_err());
    }

    #[test]
    fn offset_l_shape_keeps_corners() {
        let l = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(3.0, 0.0, 0.0),
            DVec3::new(3.0, 1.0, 0.0),
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(1.0, 3.0, 0.0),
            DVec3::new(0.0, 3.0, 0.0),
        ];
        let out = offset_loop(&l, DVec3::Z, 0.1).unwrap();
        assert!(out[0].distance(DVec3::new(0.1, 0.1, 0.0)) < 1e-9);
        assert!(out[3].distance(DVec3::new(0.9, 0.9, 0.0)) < 1e-9);
    }

    #[test]
    fn rotate_and_scale_keep_faces_planar() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 1.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        m.push_pull(f, -1.0, DVec3::Z).unwrap();
        let all: HashSet<VertId> = m.verts.keys().collect();
        let rot = DAffine3::from_rotation_z(std::f64::consts::FRAC_PI_4);
        assert!(m.transform(&all, rot));
        let sc = DAffine3::from_scale(DVec3::new(2.0, 1.0, 1.0));
        assert!(m.transform(&all, sc));
        // continua fechado e com faces para fora
        let c = m.verts.values().sum::<DVec3>() / m.verts.len() as f64;
        for f in m.faces.keys() {
            assert!(m.faces[f].normal.dot(centroid(&m, f) - c) > 0.0);
        }
        // girar só o topo em torno de X entorta as laterais: recusado
        let top: HashSet<VertId> = m.verts.iter().filter(|(_, p)| p.z > 0.5).map(|(v, _)| v).collect();
        assert!(!m.transform(&top, DAffine3::from_rotation_x(0.3)));
    }

    #[test]
    fn copy_keeps_material_and_orientation() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 1.0, 1.0, 0.0), true, DVec3::Z);
        let f = m.faces.keys().next().unwrap();
        m.reverse_face(f);
        m.faces[f].material = Some([0.5, 0.2, 0.1]);
        let edges: HashSet<EdgeId> = m.edges.keys().collect();
        m.copy_translated(&edges, DVec3::new(3.0, 0.0, 0.0), DVec3::Z);
        assert_eq!(m.faces.len(), 2);
        assert!(m.faces.values().all(|f| f.material == Some([0.5, 0.2, 0.1]) && f.normal.z > 0.99));
    }

    #[test]
    fn move_and_copy() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 1.0, 1.0, 0.0), true, DVec3::Z);
        let all: HashSet<VertId> = m.verts.keys().collect();
        assert!(m.translate(&all, DVec3::new(2.0, 0.0, 0.0)));
        assert!(m.verts.values().all(|p| p.x >= 2.0 - 1e-9));
        let edges: HashSet<EdgeId> = m.edges.keys().collect();
        m.copy_translated(&edges, DVec3::new(0.0, 3.0, 0.0), DVec3::Z);
        assert_eq!(m.faces.len(), 2);
        assert_eq!(m.edges.len(), 8);
    }

    #[test]
    fn moving_one_vertex_out_of_plane_is_detected() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 1.0, 1.0, 0.0), true, DVec3::Z);
        let v = m.verts.keys().next().unwrap();
        let one: HashSet<VertId> = [v].into_iter().collect();
        assert!(!m.translate(&one, DVec3::new(0.0, 0.0, 0.5)));
        // dentro do plano continua válido
        let mut m2 = Mesh::default();
        m2.add_polyline(&rect(0.0, 0.0, 1.0, 1.0, 0.0), true, DVec3::Z);
        let v = m2.verts.keys().next().unwrap();
        let one: HashSet<VertId> = [v].into_iter().collect();
        assert!(m2.translate(&one, DVec3::new(-0.2, 0.0, 0.0)));
    }
}
