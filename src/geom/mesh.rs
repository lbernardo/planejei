// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Malha topológica: vértices, arestas e faces (com furos).
//!
//! Regras de comportamento (as mesmas que o usuário de SketchUp espera):
//! * arestas "grudam": se cruzam ou passam por vértices, são divididas;
//! * um contorno fechado e plano vira face automaticamente;
//! * uma aresta sobre uma face a divide; um contorno dentro de uma face vira um furo + face nova;
//! * apagar a aresta entre duas faces coplanares as une; apagar a borda de uma face a remove.

use std::collections::{HashMap, HashSet};

use glam::{DVec2, DVec3};
use slotmap::{new_key_type, SecondaryMap, SlotMap};

use super::plane::{point_in_polygon, signed_area, triangulate, Plane};
use super::TOL;

new_key_type! {
    pub struct VertId;
    pub struct EdgeId;
    pub struct FaceId;
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub v: [VertId; 2],
    /// Aresta suave: não é desenhada nem selecionada (curvas, malhas importadas).
    pub soft: bool,
}

#[derive(Clone, Debug)]
pub struct Face {
    /// Contorno externo, anti-horário visto pela frente (lado da normal).
    pub outer: Vec<VertId>,
    /// Furos, em sentido horário visto pela frente.
    pub holes: Vec<Vec<VertId>>,
    pub normal: DVec3,
    pub material: Option<[f32; 3]>,
    /// Textura (índice na lista de texturas do modelo). `material` guarda a cor média.
    pub texture: Option<u32>,
}

impl Face {
    pub fn loops(&self) -> impl Iterator<Item = &Vec<VertId>> {
        std::iter::once(&self.outer).chain(self.holes.iter())
    }

    /// Pares (a, b) consecutivos de todos os contornos, no sentido da face.
    pub fn directed_edges(&self) -> impl Iterator<Item = (VertId, VertId)> + '_ {
        self.loops()
            .flat_map(|l| (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()])))
    }

    pub fn reverse(&mut self) {
        self.outer.reverse();
        for h in &mut self.holes {
            h.reverse();
        }
        self.normal = -self.normal;
    }
}

/// Linha-guia (construção): reta infinita, não faz parte da geometria.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub point: DVec3,
    pub dir: DVec3,
}

#[derive(Clone, Default)]
pub struct Mesh {
    pub verts: SlotMap<VertId, DVec3>,
    pub edges: SlotMap<EdgeId, Edge>,
    pub faces: SlotMap<FaceId, Face>,
    pub guides: Vec<Guide>,
    vert_edges: SecondaryMap<VertId, Vec<EdgeId>>,
    edge_lookup: HashMap<(VertId, VertId), EdgeId>,
}

/// Face indexada: (contorno externo, furos, normal, material).
pub type RawFace = (Vec<usize>, Vec<Vec<usize>>, DVec3, Option<[f32; 3]>);

/// Malha em forma de listas indexadas (para salvar em arquivo).
pub struct RawMesh {
    pub verts: Vec<DVec3>,
    pub edges: Vec<[usize; 2]>,
    /// Índices (em `edges`) das arestas suaves.
    pub soft: Vec<usize>,
    pub faces: Vec<RawFace>,
    /// Textura de cada face (mesma ordem de `faces`; pode ficar vazio).
    pub face_textures: Vec<Option<u32>>,
    pub guides: Vec<Guide>,
}

fn key(a: VertId, b: VertId) -> (VertId, VertId) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Parâmetros (s, t) dos pontos mais próximos entre os segmentos p1+s·d1 e p2+t·d2,
/// ou None se forem paralelos.
fn closest_params(p1: DVec3, d1: DVec3, p2: DVec3, d2: DVec3) -> Option<(f64, f64)> {
    let r = p1 - p2;
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (b, c) = (d1.dot(d2), d1.dot(r));
    let denom = a * e - b * b;
    if denom.abs() <= 1e-12 * a * e {
        return None;
    }
    let s = (b * f - c * e) / denom;
    let t = (a * f - b * c) / denom;
    Some((s, t))
}

impl Mesh {
    // ------------------------------------------------------------ arquivo

    pub fn to_raw(&self) -> RawMesh {
        let index: HashMap<VertId, usize> = self.verts.keys().enumerate().map(|(i, v)| (v, i)).collect();
        let idx = |l: &Vec<VertId>| l.iter().map(|v| index[v]).collect::<Vec<_>>();
        RawMesh {
            verts: self.verts.values().copied().collect(),
            edges: self.edges.values().map(|e| [index[&e.v[0]], index[&e.v[1]]]).collect(),
            soft: self.edges.values().enumerate().filter(|(_, e)| e.soft).map(|(i, _)| i).collect(),
            faces: self
                .faces
                .values()
                .map(|f| (idx(&f.outer), f.holes.iter().map(idx).collect(), f.normal, f.material))
                .collect(),
            face_textures: self.faces.values().map(|f| f.texture).collect(),
            guides: self.guides.clone(),
        }
    }

    /// Reconstrói sem recalcular faces (o arquivo já traz a topologia pronta).
    pub fn from_raw(raw: &RawMesh) -> Result<Self, String> {
        let mut m = Mesh::default();
        let ids: Vec<VertId> = raw.verts.iter().map(|p| m.new_vertex(*p)).collect();
        let get = |i: usize| ids.get(i).copied().ok_or_else(|| format!("vértice {i} inexistente"));
        let mut ids = Vec::with_capacity(raw.edges.len());
        for [a, b] in &raw.edges {
            ids.push(m.insert_edge(get(*a)?, get(*b)?));
        }
        for i in &raw.soft {
            if let Some(Some(e)) = ids.get(*i) {
                m.edges[*e].soft = true;
            }
        }
        for (fi, (outer, holes, normal, material)) in raw.faces.iter().enumerate() {
            let lp = |l: &Vec<usize>| l.iter().map(|i| get(*i)).collect::<Result<Vec<_>, _>>();
            let outer = lp(outer)?;
            if outer.len() < 3 {
                return Err("face com menos de 3 vértices".into());
            }
            m.faces.insert(Face {
                outer,
                holes: holes.iter().map(lp).collect::<Result<_, _>>()?,
                normal: normal.normalize_or(DVec3::Z),
                material: *material,
                texture: raw.face_textures.get(fi).copied().flatten(),
            });
        }
        m.guides = raw.guides.clone();
        Ok(m)
    }

    /// Acrescenta outra malha sem interação (sem aderência): peças independentes.
    pub fn append(&mut self, other: &Mesh) {
        let mut map = HashMap::new();
        for (v, p) in &other.verts {
            map.insert(v, self.new_vertex(*p));
        }
        for e in other.edges.values() {
            if let Some(id) = self.insert_edge(map[&e.v[0]], map[&e.v[1]]) {
                self.edges[id].soft = e.soft;
            }
        }
        for f in other.faces.values() {
            let l = |l: &Vec<VertId>| l.iter().map(|v| map[v]).collect::<Vec<_>>();
            self.faces.insert(Face {
                outer: l(&f.outer),
                holes: f.holes.iter().map(l).collect(),
                normal: f.normal,
                material: f.material,
                texture: f.texture,
            });
        }
    }

    /// Caixa fechada com as faces para fora.
    pub fn cuboid(a: DVec3, b: DVec3, material: Option<[f32; 3]>) -> Mesh {
        let (a, b) = (a.min(b), a.max(b));
        let c = |i: usize| DVec3::new(if i & 1 == 0 { a.x } else { b.x }, if i & 2 == 0 { a.y } else { b.y }, if i & 4 == 0 { a.z } else { b.z });
        // Contornos anti-horários vistos de fora.
        let faces: [([usize; 4], DVec3); 6] = [
            ([0, 2, 3, 1], DVec3::NEG_Z),
            ([4, 5, 7, 6], DVec3::Z),
            ([0, 1, 5, 4], DVec3::NEG_Y),
            ([2, 6, 7, 3], DVec3::Y),
            ([0, 4, 6, 2], DVec3::NEG_X),
            ([1, 3, 7, 5], DVec3::X),
        ];
        let edges = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
        Mesh::from_raw(&RawMesh {
            verts: (0..8).map(c).collect(),
            edges: edges.to_vec(),
            soft: Vec::new(),
            faces: faces.iter().map(|(l, n)| (l.to_vec(), Vec::new(), *n, material)).collect(),
            face_textures: Vec::new(),
            guides: Vec::new(),
        })
        .expect("caixa válida")
    }

    // ------------------------------------------------------------ consultas

    pub fn pos(&self, v: VertId) -> DVec3 {
        self.verts[v]
    }

    pub fn edge_between(&self, a: VertId, b: VertId) -> Option<EdgeId> {
        self.edge_lookup.get(&key(a, b)).copied()
    }

    pub fn edges_of(&self, v: VertId) -> &[EdgeId] {
        self.vert_edges.get(v).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn edge_points(&self, e: EdgeId) -> (DVec3, DVec3) {
        let [a, b] = self.edges[e].v;
        (self.verts[a], self.verts[b])
    }

    pub fn face_plane(&self, f: FaceId) -> Plane {
        let face = &self.faces[f];
        Plane::from_point_normal(self.verts[face.outer[0]], face.normal)
    }

    /// Faces que usam a aresta.
    pub fn faces_of_edge(&self, e: EdgeId) -> Vec<FaceId> {
        let [a, b] = self.edges[e].v;
        self.faces
            .iter()
            .filter(|(_, f)| f.directed_edges().any(|(p, q)| key(p, q) == key(a, b)))
            .map(|(id, _)| id)
            .collect()
    }

    /// Triângulos da face (para renderização e seleção).
    pub fn face_triangles(&self, f: FaceId) -> Vec<[DVec3; 3]> {
        let face = &self.faces[f];
        let plane = self.face_plane(f);
        let basis = plane.basis();
        let pts: Vec<DVec3> = face.loops().flatten().map(|v| self.verts[*v]).collect();
        let outer: Vec<DVec2> = face.outer.iter().map(|v| plane.to_2d(self.verts[*v], basis)).collect();
        let holes: Vec<Vec<DVec2>> = face
            .holes
            .iter()
            .map(|h| h.iter().map(|v| plane.to_2d(self.verts[*v], basis)).collect())
            .collect();
        triangulate(&outer, &holes)
            .into_iter()
            .map(|[a, b, c]| {
                let t = [pts[a], pts[b], pts[c]];
                // Garante que o triângulo segue a normal da face.
                if (t[1] - t[0]).cross(t[2] - t[0]).dot(face.normal) < 0.0 {
                    [t[0], t[2], t[1]]
                } else {
                    t
                }
            })
            .collect()
    }

    // ------------------------------------------------------------ primitivas

    fn find_vertex(&self, p: DVec3) -> Option<VertId> {
        self.verts
            .iter()
            .find(|(_, q)| q.distance_squared(p) < TOL * TOL)
            .map(|(id, _)| id)
    }

    fn new_vertex(&mut self, p: DVec3) -> VertId {
        let v = self.verts.insert(p);
        self.vert_edges.insert(v, Vec::new());
        v
    }

    fn insert_edge(&mut self, a: VertId, b: VertId) -> Option<EdgeId> {
        if a == b || self.edge_lookup.contains_key(&key(a, b)) {
            return None;
        }
        let e = self.edges.insert(Edge { v: [a, b], soft: false });
        self.edge_lookup.insert(key(a, b), e);
        self.vert_edges[a].push(e);
        self.vert_edges[b].push(e);
        Some(e)
    }

    fn remove_edge(&mut self, e: EdgeId) {
        let Some(edge) = self.edges.remove(e) else {
            return;
        };
        let [a, b] = edge.v;
        self.edge_lookup.remove(&key(a, b));
        for v in [a, b] {
            if let Some(list) = self.vert_edges.get_mut(v) {
                list.retain(|x| *x != e);
            }
        }
    }

    /// Divide a aresta no ponto `p`, mantendo os contornos das faces consistentes.
    fn split_edge(&mut self, e: EdgeId, p: DVec3) -> VertId {
        let [a, b] = self.edges[e].v;
        let m = self.new_vertex(p);
        self.remove_edge(e);
        self.insert_edge(a, m);
        self.insert_edge(m, b);
        for (_, face) in self.faces.iter_mut() {
            for l in std::iter::once(&mut face.outer).chain(face.holes.iter_mut()) {
                let n = l.len();
                let mut i = 0;
                while i < l.len() {
                    let (x, y) = (l[i], l[(i + 1) % n]);
                    if (x == a && y == b) || (x == b && y == a) {
                        l.insert(i + 1, m);
                        break;
                    }
                    i += 1;
                }
            }
        }
        m
    }

    /// Vértice no ponto `p`: reutiliza um existente, divide uma aresta ou cria um novo.
    fn vertex_at(&mut self, p: DVec3) -> VertId {
        if let Some(v) = self.find_vertex(p) {
            return v;
        }
        let on_edge = self.edges.iter().find_map(|(id, e)| {
            let (a, b) = (self.verts[e.v[0]], self.verts[e.v[1]]);
            let d = b - a;
            let len = d.length();
            let s = (p - a).dot(d) / (len * len);
            let inside = s * len > TOL && (1.0 - s) * len > TOL;
            (inside && (a + d * s).distance(p) < TOL).then_some(id)
        });
        match on_edge {
            Some(e) => self.split_edge(e, p),
            None => self.new_vertex(p),
        }
    }

    /// Insere um segmento, dividindo-o (e as arestas existentes) nos cruzamentos.
    fn insert_segment(&mut self, a: DVec3, b: DVec3, created: &mut Vec<EdgeId>) {
        if a.distance(b) < TOL {
            return;
        }
        let va = self.vertex_at(a);
        let vb = self.vertex_at(b);
        if va == vb {
            return;
        }
        let (pa, pb) = (self.verts[va], self.verts[vb]);
        let dir = pb - pa;
        let len = dir.length();

        // Cruzamentos com arestas existentes.
        let mut crossings = Vec::new();
        for (id, e) in &self.edges {
            if e.v.contains(&va) || e.v.contains(&vb) {
                continue;
            }
            let (p, q) = (self.verts[e.v[0]], self.verts[e.v[1]]);
            let d2 = q - p;
            let Some((s, t)) = closest_params(pa, dir, p, d2) else {
                continue;
            };
            let l2 = d2.length();
            if s * len > TOL && (1.0 - s) * len > TOL && t * l2 > TOL && (1.0 - t) * l2 > TOL {
                let (x, y) = (pa + dir * s, p + d2 * t);
                if x.distance(y) < TOL {
                    crossings.push((id, (x + y) * 0.5));
                }
            }
        }
        for (e, p) in crossings {
            if self.edges.contains_key(e) {
                self.split_edge(e, p);
            }
        }

        // Vértices existentes sobre o segmento (inclui os cruzamentos acima).
        let mut stops: Vec<(f64, VertId)> = self
            .verts
            .iter()
            .filter(|(id, _)| *id != va && *id != vb)
            .filter_map(|(id, p)| {
                let s = (*p - pa).dot(dir) / (len * len);
                let inside = s * len > TOL && (1.0 - s) * len > TOL;
                (inside && (pa + dir * s).distance(*p) < TOL).then_some((s, id))
            })
            .collect();
        stops.sort_by(|x, y| x.0.total_cmp(&y.0));

        let mut prev = va;
        for v in stops.into_iter().map(|(_, v)| v).chain(std::iter::once(vb)) {
            if let Some(e) = self.insert_edge(prev, v) {
                created.push(e);
            }
            prev = v;
        }
    }

    // ------------------------------------------------------------ operações públicas

    /// Adiciona segmentos e forma as faces resultantes.
    /// `toward_viewer` orienta faces novas que não têm outra referência.
    pub fn add_segments(&mut self, segs: &[(DVec3, DVec3)], toward_viewer: DVec3) -> Vec<EdgeId> {
        let mut created = Vec::new();
        for &(a, b) in segs {
            self.insert_segment(a, b, &mut created);
        }
        // Arestas pré-existentes que coincidem com o desenho também contam como "novas":
        // redesenhar sobre um contorno sem face recria a face (como no SketchUp).
        for &(a, b) in segs {
            if let (Some(va), Some(vb)) = (self.find_vertex(a), self.find_vertex(b))
                && let Some(e) = self.edge_between(va, vb)
                    && !created.contains(&e) {
                        created.push(e);
                    }
        }
        self.rebuild_around(&created, toward_viewer);
        created
    }

    /// Adiciona uma polilinha (fechada ou não).
    pub fn add_polyline(&mut self, pts: &[DVec3], closed: bool, toward_viewer: DVec3) -> Vec<EdgeId> {
        let mut segs: Vec<(DVec3, DVec3)> = pts.windows(2).map(|w| (w[0], w[1])).collect();
        if closed && pts.len() > 2 {
            segs.push((pts[pts.len() - 1], pts[0]));
        }
        self.add_segments(&segs, toward_viewer)
    }

    /// Apaga arestas. Faces apoiadas nelas somem, exceto quando a aresta separa
    /// duas faces coplanares: nesse caso elas se unem.
    pub fn erase_edges(&mut self, edges: &[EdgeId]) {
        let mut planes: Vec<Plane> = Vec::new();
        for &e in edges {
            if !self.edges.contains_key(e) {
                continue;
            }
            let faces = self.faces_of_edge(e);
            let coplanar_pair = faces.len() == 2
                && self.face_plane(faces[0]).same_surface(&self.face_plane(faces[1]));
            if coplanar_pair {
                push_plane(&mut planes, self.face_plane(faces[0]));
            } else {
                for f in faces {
                    self.faces.remove(f);
                }
            }
            let [a, b] = self.edges[e].v;
            self.remove_edge(e);
            for v in [a, b] {
                if self.edges_of(v).is_empty() {
                    self.verts.remove(v);
                    self.vert_edges.remove(v);
                }
            }
        }
        let none = HashSet::new();
        for p in planes {
            self.rebuild_plane(p, &none, DVec3::Z);
        }
    }

    pub fn reverse_face(&mut self, f: FaceId) {
        if let Some(face) = self.faces.get_mut(f) {
            face.reverse();
        }
    }

    // ------------------------------------------------------------ formação de faces

    fn rebuild_around(&mut self, new_edges: &[EdgeId], toward_viewer: DVec3) {
        let mut planes: Vec<Plane> = Vec::new();
        for &e in new_edges {
            let [a, b] = self.edges[e].v;
            let (pa, pb) = (self.verts[a], self.verts[b]);
            // Planos formados com as arestas vizinhas.
            for v in [a, b] {
                for &e2 in self.edges_of(v) {
                    if e2 == e {
                        continue;
                    }
                    let [x, y] = self.edges[e2].v;
                    let w = if x == v { y } else { x };
                    if let Some(p) = Plane::from_points(pa, pb, self.verts[w]) {
                        push_plane(&mut planes, p);
                    }
                }
            }
            // Planos de faces existentes que contêm a aresta.
            for (id, _) in &self.faces {
                let p = self.face_plane(id);
                if p.contains(pa) && p.contains(pb) {
                    push_plane(&mut planes, p);
                }
            }
        }
        let new_set: HashSet<EdgeId> = new_edges.iter().copied().collect();
        let mut created = Vec::new();
        for p in planes {
            created.extend(self.rebuild_plane(p, &new_set, toward_viewer));
        }
        self.orient_closed_shells(&created);
    }

    /// Recalcula todas as faces de um plano a partir das arestas contidas nele.
    fn rebuild_plane(&mut self, plane: Plane, new_edges: &HashSet<EdgeId>, toward_viewer: DVec3) -> Vec<FaceId> {
        let plane = plane.canonical();
        let basis = plane.basis();

        // Arestas e vértices do plano.
        let plane_edges: Vec<EdgeId> = self
            .edges
            .iter()
            .filter(|(_, e)| plane.contains(self.verts[e.v[0]]) && plane.contains(self.verts[e.v[1]]))
            .map(|(id, _)| id)
            .collect();
        let old_faces: Vec<(FaceId, Face)> = self
            .faces
            .iter()
            .filter(|(id, _)| self.face_plane(*id).same_surface(&plane))
            .map(|(id, f)| (id, f.clone()))
            .collect();
        for (id, _) in &old_faces {
            self.faces.remove(*id);
        }
        if plane_edges.is_empty() {
            return Vec::new();
        }

        let p2 = |v: VertId| plane.to_2d(self.verts[v], basis);

        // Arestas de saída de cada vértice, ordenadas por ângulo.
        let mut out: HashMap<VertId, Vec<VertId>> = HashMap::new();
        for &e in &plane_edges {
            let [a, b] = self.edges[e].v;
            out.entry(a).or_default().push(b);
            out.entry(b).or_default().push(a);
        }
        for (v, list) in out.iter_mut() {
            let o = p2(*v);
            list.sort_by(|x, y| {
                let (dx, dy) = (p2(*x) - o, p2(*y) - o);
                dx.y.atan2(dx.x).total_cmp(&dy.y.atan2(dy.x))
            });
        }

        // Percorre os ciclos: cada semi-aresta pertence a exatamente um ciclo,
        // com a região à esquerda.
        let mut visited: HashSet<(VertId, VertId)> = HashSet::new();
        let mut cycles: Vec<Vec<VertId>> = Vec::new();
        for &e in &plane_edges {
            let [a, b] = self.edges[e].v;
            for start in [(a, b), (b, a)] {
                if visited.contains(&start) {
                    continue;
                }
                let mut cycle = Vec::new();
                let mut h = start;
                loop {
                    visited.insert(h);
                    cycle.push(h.0);
                    let list = &out[&h.1];
                    let idx = list.iter().position(|w| *w == h.0).expect("aresta gêmea");
                    let next = list[(idx + list.len() - 1) % list.len()];
                    h = (h.1, next);
                    if h == start || cycle.len() > plane_edges.len() * 2 + 2 {
                        break;
                    }
                }
                cycles.push(cycle);
            }
        }

        // Componentes conexas (para associar furos às faces certas).
        let mut comp: HashMap<VertId, usize> = HashMap::new();
        let mut next_comp = 0;
        for &v in out.keys() {
            if comp.contains_key(&v) {
                continue;
            }
            let mut stack = vec![v];
            comp.insert(v, next_comp);
            while let Some(x) = stack.pop() {
                for &y in &out[&x] {
                    if let std::collections::hash_map::Entry::Vacant(en) = comp.entry(y) {
                        en.insert(next_comp);
                        stack.push(y);
                    }
                }
            }
            next_comp += 1;
        }

        let poly = |c: &[VertId]| -> Vec<DVec2> { c.iter().map(|v| p2(*v)).collect() };
        let areas: Vec<f64> = cycles.iter().map(|c| signed_area(&poly(c))).collect();
        const AREA_EPS: f64 = 1e-10;
        let bounded: Vec<usize> = (0..cycles.len()).filter(|&i| areas[i] > AREA_EPS).collect();

        // Contornos externos de componentes viram furos da menor face que os contém.
        let mut holes_of: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in (0..cycles.len()).filter(|&i| areas[i] <= AREA_EPS) {
            let rep = cycles[i][0];
            let pt = p2(rep);
            let host = bounded
                .iter()
                .copied()
                .filter(|&j| comp[&cycles[j][0]] != comp[&rep])
                .filter(|&j| point_in_polygon(pt, &poly(&cycles[j])))
                .min_by(|&x, &y| areas[x].total_cmp(&areas[y]));
            if let Some(j) = host {
                holes_of.entry(j).or_default().push(i);
            }
        }

        let old_polys: Vec<(Vec<DVec2>, Vec<Vec<DVec2>>)> = old_faces
            .iter()
            .map(|(_, f)| (poly(&f.outer), f.holes.iter().map(|h| poly(h)).collect()))
            .collect();

        let mut created = Vec::new();
        for &i in &bounded {
            let outer = clean_spikes(&cycles[i]);
            if outer.len() < 3 {
                continue;
            }
            let holes: Vec<Vec<VertId>> = holes_of
                .get(&i)
                .into_iter()
                .flatten()
                .map(|&h| clean_spikes(&cycles[h]))
                .filter(|h| h.len() >= 3 && signed_area(&poly(h)).abs() > AREA_EPS)
                .collect();

            // Ponto interno de amostra: centro do maior triângulo.
            let outer2 = poly(&outer);
            let holes2: Vec<Vec<DVec2>> = holes.iter().map(|h| poly(h)).collect();
            let all2: Vec<DVec2> = outer2.iter().chain(holes2.iter().flatten()).copied().collect();
            let Some(sample) = triangulate(&outer2, &holes2)
                .into_iter()
                .map(|[a, b, c]| {
                    let (a, b, c) = (all2[a], all2[b], all2[c]);
                    ((b - a).perp_dot(c - a).abs(), (a + b + c) / 3.0)
                })
                .max_by(|x, y| x.0.total_cmp(&y.0))
                .map(|(_, c)| c)
            else {
                continue;
            };

            let from_old = old_polys.iter().position(|(o, hs)| {
                point_in_polygon(sample, o) && !hs.iter().any(|h| point_in_polygon(sample, h))
            });
            let n = cycles[i].len();
            let closed_by_new = (0..n).any(|k| {
                self.edge_between(cycles[i][k], cycles[i][(k + 1) % n])
                    .is_some_and(|e| new_edges.contains(&e))
            });
            if from_old.is_none() && !closed_by_new {
                continue;
            }

            let mut face = Face {
                outer,
                holes,
                normal: plane.n,
                material: from_old.and_then(|k| old_faces[k].1.material),
                texture: from_old.and_then(|k| old_faces[k].1.texture),
            };
            let flip = match from_old {
                Some(k) => old_faces[k].1.normal.dot(plane.n) < 0.0,
                None => self.orientation_flip(&face, &plane, toward_viewer),
            };
            if flip {
                face.reverse();
            }
            created.push(self.faces.insert(face));
        }
        created
    }

    /// Decide se uma face nova deve ser invertida.
    fn orientation_flip(&self, face: &Face, plane: &Plane, toward_viewer: DVec3) -> bool {
        // 1) Consistência com faces vizinhas: a aresta comum deve ser percorrida em sentidos
        //    opostos. Uma aresta pode ter várias vizinhas (ex.: rodapé entre piso e parede),
        //    então todas votam.
        let mine: Vec<(VertId, VertId)> = face.directed_edges().collect();
        let (mut flip, mut keep) = (0, 0);
        for (_, other) in &self.faces {
            for (p, q) in other.directed_edges() {
                if mine.contains(&(p, q)) {
                    flip += 1;
                } else if mine.contains(&(q, p)) {
                    keep += 1;
                }
            }
        }
        if flip != keep {
            return flip > keep;
        }
        let has_neighbors = flip > 0;
        // 2) Faces no chão (z = 0) ficam viradas para baixo, como no SketchUp,
        //    para que Empurrar/Puxar para cima gere um sólido com faces para fora.
        if plane.n.z.abs() > 0.999 && !has_neighbors {
            let on_ground = (plane.d / plane.n.z).abs() < TOL;
            let want_up = !on_ground;
            return (plane.n.z > 0.0) != want_up;
        }
        // 3) Demais: virada para quem está olhando.
        plane.n.dot(toward_viewer) < 0.0
    }

    /// Sólidos fechados recém-formados ficam com as faces para fora.
    pub(crate) fn orient_closed_shells(&mut self, seeds: &[FaceId]) {
        let mut edge_faces: HashMap<(VertId, VertId), Vec<FaceId>> = HashMap::new();
        for (id, f) in &self.faces {
            for (p, q) in f.directed_edges() {
                edge_faces.entry(key(p, q)).or_default().push(id);
            }
        }
        let mut done: HashSet<FaceId> = HashSet::new();
        for &seed in seeds {
            if done.contains(&seed) || !self.faces.contains_key(seed) {
                continue;
            }
            // Percorre o sólido e, ao mesmo tempo, deixa as faces consistentes entre si:
            // a aresta comum a duas faces deve ser percorrida em sentidos opostos.
            let mut shell = vec![seed];
            let mut closed = true;
            done.insert(seed);
            let mut i = 0;
            while i < shell.len() {
                let f = shell[i];
                i += 1;
                let dirs: Vec<(VertId, VertId)> = self.faces[f].directed_edges().collect();
                for (p, q) in dirs {
                    let users = edge_faces[&key(p, q)].clone();
                    if users.len() != 2 {
                        closed = false;
                        continue;
                    }
                    for g in users {
                        if g == f || !done.insert(g) {
                            continue;
                        }
                        if self.faces[g].directed_edges().any(|(a, b)| a == p && b == q) {
                            self.faces[g].reverse();
                        }
                        shell.push(g);
                    }
                }
            }
            if !closed {
                continue;
            }
            let volume: f64 = shell
                .iter()
                .flat_map(|&f| self.face_triangles(f))
                .map(|[a, b, c]| a.dot(b.cross(c)))
                .sum();
            if volume < 0.0 {
                for f in shell {
                    self.faces[f].reverse();
                }
            }
        }
    }
}

fn push_plane(planes: &mut Vec<Plane>, p: Plane) {
    if !planes.iter().any(|q| q.same_surface(&p)) {
        planes.push(p);
    }
}

/// Remove "espinhos" (a → b → a) de um ciclo, deixados por arestas soltas.
fn clean_spikes(cycle: &[VertId]) -> Vec<VertId> {
    let mut c = cycle.to_vec();
    loop {
        let n = c.len();
        if n < 3 {
            return c;
        }
        let spike = (0..n).find(|&i| c[(i + n - 1) % n] == c[(i + 1) % n]);
        match spike {
            Some(i) => {
                // remove c[i] e uma das cópias vizinhas
                let j = (i + 1) % n;
                let (hi, lo) = if i > j { (i, j) } else { (j, i) };
                c.remove(hi);
                c.remove(lo);
            }
            None => return c,
        }
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

    fn area(m: &Mesh, f: FaceId) -> f64 {
        m.face_triangles(f)
            .iter()
            .map(|[a, b, c]| (*b - *a).cross(*c - *a).length() * 0.5)
            .sum()
    }

    fn total_area(m: &Mesh) -> f64 {
        m.faces.keys().map(|f| area(m, f)).sum()
    }

    #[test]
    fn closed_rectangle_makes_one_face() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 2.0, 1.0, 0.0), true, DVec3::Z);
        assert_eq!(m.edges.len(), 4);
        assert_eq!(m.faces.len(), 1);
        assert!((total_area(&m) - 2.0).abs() < 1e-9);
        // No chão, a face fica virada para baixo (convenção do SketchUp).
        assert!(m.faces.values().next().unwrap().normal.z < -0.99);
    }

    #[test]
    fn open_polyline_makes_no_face() {
        let mut m = Mesh::default();
        let r = rect(0.0, 0.0, 2.0, 1.0, 0.0);
        m.add_polyline(&r, false, DVec3::Z);
        assert_eq!(m.faces.len(), 0);
        // Fechar com a última linha cria a face.
        m.add_segments(&[(r[3], r[0])], DVec3::Z);
        assert_eq!(m.faces.len(), 1);
    }

    #[test]
    fn elevated_horizontal_face_points_up() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 1.0, 1.0, 0.9), true, DVec3::Z);
        assert!(m.faces.values().next().unwrap().normal.z > 0.99);
    }

    #[test]
    fn line_across_face_splits_it() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 4.0, 2.0, 0.0), true, DVec3::Z);
        m.add_segments(&[(DVec3::new(1.0, 0.0, 0.0), DVec3::new(1.0, 2.0, 0.0))], DVec3::Z);
        assert_eq!(m.faces.len(), 2);
        assert_eq!(m.edges.len(), 7);
        assert!((total_area(&m) - 8.0).abs() < 1e-9);
        // Orientação preservada.
        assert!(m.faces.values().all(|f| f.normal.z < -0.99));
    }

    #[test]
    fn crossing_lines_split_each_other() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 4.0, 4.0, 0.0), true, DVec3::Z);
        m.add_segments(&[(DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 4.0, 0.0))], DVec3::Z);
        m.add_segments(&[(DVec3::new(0.0, 2.0, 0.0), DVec3::new(4.0, 2.0, 0.0))], DVec3::Z);
        assert_eq!(m.faces.len(), 4);
        assert!((total_area(&m) - 16.0).abs() < 1e-9);
    }

    #[test]
    fn rectangle_inside_face_makes_hole_and_inner_face() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 5.0, 4.0, 0.0), true, DVec3::Z);
        m.add_polyline(&rect(1.0, 1.0, 2.0, 3.0, 0.0), true, DVec3::Z);
        assert_eq!(m.faces.len(), 2);
        let with_hole = m.faces.values().filter(|f| f.holes.len() == 1).count();
        assert_eq!(with_hole, 1);
        assert!((total_area(&m) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn rectangle_on_empty_plane_inside_unfilled_loop_does_not_fill_outer() {
        let mut m = Mesh::default();
        let big = rect(0.0, 0.0, 5.0, 5.0, 0.0);
        m.add_polyline(&big, true, DVec3::Z);
        let outer_edge = m.edges.keys().next().unwrap();
        // Apaga uma aresta: face some; desenha de volta só o contorno interno.
        m.erase_edges(&[outer_edge]);
        assert_eq!(m.faces.len(), 0);
        m.add_polyline(&rect(1.0, 1.0, 2.0, 2.0, 0.0), true, DVec3::Z);
        assert_eq!(m.faces.len(), 1);
        assert!((total_area(&m) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn erasing_shared_coplanar_edge_merges_faces() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 4.0, 2.0, 0.0), true, DVec3::Z);
        m.add_segments(&[(DVec3::new(1.0, 0.0, 0.0), DVec3::new(1.0, 2.0, 0.0))], DVec3::Z);
        assert_eq!(m.faces.len(), 2);
        let mid = m
            .edges
            .iter()
            .find(|(_, e)| {
                let (a, b) = (m.verts[e.v[0]], m.verts[e.v[1]]);
                (a.x - 1.0).abs() < 1e-9 && (b.x - 1.0).abs() < 1e-9
            })
            .map(|(id, _)| id)
            .unwrap();
        m.erase_edges(&[mid]);
        assert_eq!(m.faces.len(), 1);
        assert!((total_area(&m) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn erasing_border_edge_removes_face() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 1.0, 1.0, 0.0), true, DVec3::Z);
        let e = m.edges.keys().next().unwrap();
        m.erase_edges(&[e]);
        assert_eq!(m.faces.len(), 0);
        assert_eq!(m.edges.len(), 3);
    }

    #[test]
    fn box_from_edges_has_six_outward_faces() {
        let mut m = Mesh::default();
        let (a, b) = (DVec3::ZERO, DVec3::new(2.0, 1.0, 3.0));
        let c = |x: bool, y: bool, z: bool| {
            DVec3::new(if x { b.x } else { a.x }, if y { b.y } else { a.y }, if z { b.z } else { a.z })
        };
        let segs = [
            (c(false, false, false), c(true, false, false)),
            (c(true, false, false), c(true, true, false)),
            (c(true, true, false), c(false, true, false)),
            (c(false, true, false), c(false, false, false)),
            (c(false, false, true), c(true, false, true)),
            (c(true, false, true), c(true, true, true)),
            (c(true, true, true), c(false, true, true)),
            (c(false, true, true), c(false, false, true)),
            (c(false, false, false), c(false, false, true)),
            (c(true, false, false), c(true, false, true)),
            (c(true, true, false), c(true, true, true)),
            (c(false, true, false), c(false, true, true)),
        ];
        m.add_segments(&segs, DVec3::new(-1.0, -1.0, 1.0));
        assert_eq!(m.faces.len(), 6);
        let center = (a + b) * 0.5;
        for id in m.faces.keys() {
            let f = &m.faces[id];
            let p = m.verts[f.outer[0]];
            assert!(f.normal.dot(p - center) > 0.0, "face virada para dentro");
        }
    }

    #[test]
    fn wall_box_on_floor_edge_faces_outward() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 5.0, 4.0, 0.0), true, DVec3::Z);
        let (a, b) = (DVec3::new(-0.15, 4.0, 0.0), DVec3::new(5.0, 4.15, 2.7));
        let c = |x: bool, y: bool, z: bool| {
            DVec3::new(if x { b.x } else { a.x }, if y { b.y } else { a.y }, if z { b.z } else { a.z })
        };
        let mut segs = Vec::new();
        for z in [false, true] {
            segs.push((c(false, false, z), c(true, false, z)));
            segs.push((c(true, false, z), c(true, true, z)));
            segs.push((c(true, true, z), c(false, true, z)));
            segs.push((c(false, true, z), c(false, false, z)));
        }
        for (x, y) in [(false, false), (true, false), (true, true), (false, true)] {
            segs.push((c(x, y, false), c(x, y, true)));
        }
        m.add_segments(&segs, DVec3::new(-1.0, -1.0, 1.0));
        let center = (a + b) * 0.5;
        for (id, f) in &m.faces {
            let p = m.verts[f.outer[0]];
            let on_floor = m.face_plane(id).n.z.abs() > 0.99 && p.z.abs() < 1e-9 && p.y < 4.0 - 1e-9;
            if on_floor {
                continue;
            }
            // Todas as faces da parede apontam para fora da caixa.
            let fc = f.outer.iter().map(|v| m.verts[*v]).sum::<DVec3>() / f.outer.len() as f64;
            assert!(f.normal.dot(fc - center) > 0.0, "face da parede virada para dentro: {:?}", f.normal);
        }
    }

    #[test]
    fn dangling_edge_inside_face_keeps_face() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 4.0, 4.0, 0.0), true, DVec3::Z);
        m.add_segments(&[(DVec3::new(1.0, 1.0, 0.0), DVec3::new(2.0, 2.0, 0.0))], DVec3::Z);
        assert_eq!(m.faces.len(), 1);
        assert!((total_area(&m) - 16.0).abs() < 1e-9);
        // Aresta saindo da borda para dentro também não quebra a face.
        m.add_segments(&[(DVec3::new(0.0, 3.0, 0.0), DVec3::new(1.0, 3.0, 0.0))], DVec3::Z);
        assert_eq!(m.faces.len(), 1);
        assert!((total_area(&m) - 16.0).abs() < 1e-9);
    }

    #[test]
    fn rectangle_touching_border_splits_face() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 5.0, 4.0, 0.0), true, DVec3::Z);
        // Canto no canto do piso, dois lados sobre as bordas.
        m.add_polyline(&rect(0.0, 0.0, 1.0, 1.0, 0.0), true, DVec3::Z);
        assert_eq!(m.faces.len(), 2);
        assert!(m.faces.values().all(|f| f.holes.is_empty()));
        assert!((total_area(&m) - 20.0).abs() < 1e-9);
        // 4 bordas originais viram 6 + 2 internas = 8 arestas.
        assert_eq!(m.edges.len(), 8);
    }

    #[test]
    fn collinear_overlap_reuses_existing_edges() {
        let mut m = Mesh::default();
        m.add_segments(&[(DVec3::new(0.0, 0.0, 0.0), DVec3::new(4.0, 0.0, 0.0))], DVec3::Z);
        m.add_segments(&[(DVec3::new(2.0, 0.0, 0.0), DVec3::new(6.0, 0.0, 0.0))], DVec3::Z);
        // 0-2, 2-4, 4-6
        assert_eq!(m.edges.len(), 3);
        assert_eq!(m.verts.len(), 4);
    }

    #[test]
    fn rectangle_crossing_border_splits_both() {
        let mut m = Mesh::default();
        m.add_polyline(&rect(0.0, 0.0, 4.0, 4.0, 0.0), true, DVec3::Z);
        // Metade dentro, metade fora da face.
        m.add_polyline(&rect(3.0, 1.0, 6.0, 2.0, 0.0), true, DVec3::Z);
        // Face original (com entalhe) + parte interna + parte externa.
        assert_eq!(m.faces.len(), 3);
        assert!((total_area(&m) - (16.0 + 2.0)).abs() < 1e-9);
    }

    #[test]
    fn line_on_wall_splits_vertical_face() {
        let mut m = Mesh::default();
        let wall = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(4.0, 0.0, 0.0),
            DVec3::new(4.0, 0.0, 2.7),
            DVec3::new(0.0, 0.0, 2.7),
        ];
        m.add_polyline(&wall, true, DVec3::new(0.0, -1.0, 0.0));
        assert!(m.faces.values().next().unwrap().normal.y < -0.99);
        m.add_segments(&[(DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 2.7))], DVec3::new(0.0, -1.0, 0.0));
        assert_eq!(m.faces.len(), 2);
        assert!(m.faces.values().all(|f| f.normal.y < -0.99));
    }
}
