// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Kernel de geometria em f64 (unidades: metros).

pub mod mesh;
pub mod ops;
pub mod plane;

pub use mesh::{EdgeId, FaceId, Mesh, RawMesh};

/// Tolerância de coincidência: 0,01 mm.
pub const TOL: f64 = 1e-5;
