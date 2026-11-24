// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Câmera orbital no estilo de modeladores de arquitetura: Z para cima, unidades em metros.

use glam::camera::rh::proj::directx as wgpu_proj;
use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Projection {
    Perspective,
    Parallel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StandardView {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
    Iso,
}

impl StandardView {
    pub const ALL: [StandardView; 7] = [
        StandardView::Top,
        StandardView::Bottom,
        StandardView::Front,
        StandardView::Back,
        StandardView::Left,
        StandardView::Right,
        StandardView::Iso,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "top" => StandardView::Top,
            "bottom" => StandardView::Bottom,
            "front" => StandardView::Front,
            "back" => StandardView::Back,
            "left" => StandardView::Left,
            "right" => StandardView::Right,
            "iso" => StandardView::Iso,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            StandardView::Top => "Superior",
            StandardView::Bottom => "Inferior",
            StandardView::Front => "Frontal",
            StandardView::Back => "Posterior",
            StandardView::Left => "Esquerda",
            StandardView::Right => "Direita",
            StandardView::Iso => "Isométrica",
        }
    }

    /// (yaw, pitch) em graus da direção alvo -> olho.
    fn angles(self) -> (f32, f32) {
        match self {
            StandardView::Top => (-90.0, 90.0),
            StandardView::Bottom => (-90.0, -90.0),
            StandardView::Front => (-90.0, 0.0),
            StandardView::Back => (90.0, 0.0),
            StandardView::Left => (180.0, 0.0),
            StandardView::Right => (0.0, 0.0),
            StandardView::Iso => (-135.0, 35.264),
        }
    }
}

/// Um raio no espaço do mundo.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    /// Interseção com o plano `dot(n, p) = d`.
    pub fn hit_plane(&self, n: Vec3, d: f32) -> Option<Vec3> {
        let denom = n.dot(self.dir);
        if denom.abs() < 1e-8 {
            return None;
        }
        let t = (d - n.dot(self.origin)) / denom;
        (t > 0.0).then(|| self.origin + self.dir * t)
    }
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub target: Vec3,
    /// Ângulo horizontal (rad) da direção alvo -> olho, medido a partir de +X.
    pub yaw: f32,
    /// Elevação (rad) da direção alvo -> olho.
    pub pitch: f32,
    pub distance: f32,
    /// Campo de visão vertical (graus).
    pub fov_deg: f32,
    pub projection: Projection,
}

pub const MIN_DISTANCE: f32 = 0.02;

impl Default for Camera {
    fn default() -> Self {
        let mut cam = Self {
            target: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            distance: 12.0,
            fov_deg: 35.0,
            projection: Projection::Perspective,
        };
        cam.set_view(StandardView::Iso);
        cam
    }
}

impl Camera {
    /// Vetor unitário do alvo para o olho.
    pub fn to_eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.to_eye() * self.distance
    }

    pub fn forward(&self) -> Vec3 {
        -self.to_eye()
    }

    /// Vetor "para cima" da tela; sempre ortogonal, sem singularidade nas vistas de topo.
    pub fn up(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sp * cy, -sp * sy, cp)
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(self.up()).normalize()
    }

    fn tan_half_fov(&self) -> f32 {
        (self.fov_deg.to_radians() * 0.5).tan()
    }

    /// Metade da altura visível no plano do alvo (metros).
    pub fn half_height_at_target(&self) -> f32 {
        self.distance * self.tan_half_fov()
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_to_mat4(self.eye(), self.forward(), self.up())
    }

    /// Projeção com Z reverso (mais precisão de profundidade para cenas grandes e detalhes pequenos).
    pub fn proj(&self, aspect: f32, scene_radius: f32) -> Mat4 {
        match self.projection {
            Projection::Perspective => {
                let near = (self.distance * 0.002).clamp(0.0005, 0.5);
                wgpu_proj::perspective_infinite_reverse(self.fov_deg.to_radians(), aspect, near)
            }
            Projection::Parallel => {
                let hh = self.half_height_at_target();
                let hw = hh * aspect;
                let span = (self.distance + scene_radius) * 4.0 + 10.0;
                // near/far trocados = Z reverso.
                wgpu_proj::orthographic(-hw, hw, -hh, hh, span, -span)
            }
        }
    }

    /// Fatores de deslocamento de profundidade (multiplicativo, aditivo) para desenhar
    /// arestas por cima das faces sem z-fighting.
    pub fn depth_bias(&self, scene_radius: f32) -> [f32; 2] {
        match self.projection {
            Projection::Perspective => [1.5e-3, 0.0],
            Projection::Parallel => {
                let span = (self.distance + scene_radius) * 4.0 + 10.0;
                [0.0, 1.5e-3 * self.distance / (2.0 * span)]
            }
        }
    }

    pub fn set_view(&mut self, view: StandardView) {
        let (yaw, pitch) = view.angles();
        self.yaw = yaw.to_radians();
        self.pitch = pitch.to_radians();
    }

    /// Raio do cursor. `ndc` em [-1, 1], com Y para cima.
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> Ray {
        let hh = self.tan_half_fov();
        let (f, r, u) = (self.forward(), self.right(), self.up());
        match self.projection {
            Projection::Perspective => Ray {
                origin: self.eye(),
                dir: (f + r * (ndc.x * hh * aspect) + u * (ndc.y * hh)).normalize(),
            },
            Projection::Parallel => {
                let h = self.half_height_at_target();
                Ray {
                    origin: self.eye() - f * self.distance * 50.0
                        + r * (ndc.x * h * aspect)
                        + u * (ndc.y * h),
                    dir: f,
                }
            }
        }
    }

    pub fn orbit(&mut self, delta_px: Vec2) {
        const SPEED: f32 = 0.006;
        self.yaw -= delta_px.x * SPEED;
        self.pitch = (self.pitch + delta_px.y * SPEED)
            .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
    }

    /// Interpolação suave entre duas câmeras (transição de cenas).
    pub fn lerp(a: &Camera, b: &Camera, t: f32) -> Camera {
        let t = t * t * (3.0 - 2.0 * t);
        let tau = std::f32::consts::TAU;
        // menor caminho no ângulo horizontal
        let mut dy = (b.yaw - a.yaw) % tau;
        if dy > std::f32::consts::PI {
            dy -= tau;
        } else if dy < -std::f32::consts::PI {
            dy += tau;
        }
        Camera {
            target: a.target.lerp(b.target, t),
            yaw: a.yaw + dy * t,
            pitch: a.pitch + (b.pitch - a.pitch) * t,
            distance: (a.distance.ln() + (b.distance.ln() - a.distance.ln()) * t).exp(),
            fov_deg: a.fov_deg + (b.fov_deg - a.fov_deg) * t,
            projection: if t < 0.5 { a.projection } else { b.projection },
        }
    }

    /// Orbita em torno de um ponto (o ponto sob o cursor no início do arraste).
    pub fn orbit_around(&mut self, delta_px: Vec2, pivot: Vec3) {
        let frame = |c: &Camera| glam::Mat3::from_cols(c.right(), c.up(), c.to_eye());
        let old = frame(self);
        self.orbit(delta_px);
        let rot = frame(self) * old.transpose();
        self.target = pivot + rot * (self.target - pivot);
    }

    /// Pan: o conteúdo acompanha o cursor. `viewport_h_px` em pixels lógicos.
    pub fn pan(&mut self, delta_px: Vec2, viewport_h_px: f32) {
        let world_per_px = 2.0 * self.half_height_at_target() / viewport_h_px.max(1.0);
        self.target -= self.right() * (delta_px.x * world_per_px);
        self.target += self.up() * (delta_px.y * world_per_px);
    }

    /// Zoom em direção ao ponto sob o cursor. `factor < 1` aproxima.
    pub fn zoom_at(&mut self, factor: f32, ndc: Vec2, aspect: f32) {
        let factor = factor.clamp(0.2, 5.0);
        let f = self.forward();
        let anchor = self
            .ray(ndc, aspect)
            .hit_plane(f, f.dot(self.target))
            .unwrap_or(self.target);
        let new_distance = (self.distance * factor).max(MIN_DISTANCE);
        let k = new_distance / self.distance;
        self.target = anchor + (self.target - anchor) * k;
        self.distance = new_distance;
    }

    /// Enquadra uma esfera envolvente.
    pub fn zoom_extents(&mut self, center: Vec3, radius: f32, aspect: f32) {
        let radius = radius.max(0.1);
        let half_fov_v = self.fov_deg.to_radians() * 0.5;
        let half_fov_h = (half_fov_v.tan() * aspect).atan();
        let half = half_fov_v.min(half_fov_h);
        self.target = center;
        self.distance = match self.projection {
            Projection::Perspective => radius / half.sin(),
            Projection::Parallel => radius / half.tan(),
        } * 1.05;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_between_scenes_takes_the_short_way() {
        let mut a = Camera::default();
        let mut b = Camera::default();
        a.yaw = 170f32.to_radians();
        b.yaw = (-170f32).to_radians();
        b.distance = a.distance * 4.0;
        let mid = Camera::lerp(&a, &b, 0.5);
        // passa por 180°, não por 0°
        assert!((mid.yaw.cos() + 1.0).abs() < 1e-3, "{}", mid.yaw.to_degrees());
        assert!((mid.distance - a.distance * 2.0).abs() < 1e-3, "distância geométrica");
        assert_eq!(Camera::lerp(&a, &b, 1.0).distance, b.distance);
    }

    #[test]
    fn orbit_around_keeps_pivot_fixed_on_screen() {
        let mut cam = Camera::default();
        let pivot = Vec3::new(2.0, 1.0, 0.5);
        let to_cam = |c: &Camera, p: Vec3| c.view().transform_point3(p);
        let before = to_cam(&cam, pivot);
        cam.orbit_around(Vec2::new(40.0, -25.0), pivot);
        let after = to_cam(&cam, pivot);
        assert!((before - after).length() < 1e-4, "{before:?} != {after:?}");
    }
}
