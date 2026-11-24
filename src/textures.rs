// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Texturas: imagens com tamanho real (metros), geradas por código (sem emendas)
//! ou importadas (PNG/JPG). Ficam dentro do arquivo do modelo.

use std::sync::Arc;

use base64::Engine as _;

/// Imagem RGBA (sRGB) com a cor média já calculada.
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    /// Cor média (sRGB 0..1): usada no estilo sem texturas e nas exportações.
    pub avg: [f32; 3],
}

impl Image {
    pub fn new(w: u32, h: u32, rgba: Vec<u8>) -> Self {
        let n = (w * h) as f64;
        let mut sum = [0.0f64; 3];
        for px in rgba.chunks_exact(4) {
            for k in 0..3 {
                sum[k] += px[k] as f64;
            }
        }
        let avg = [(sum[0] / n / 255.0) as f32, (sum[1] / n / 255.0) as f32, (sum[2] / n / 255.0) as f32];
        Self { w, h, rgba, avg }
    }
}

#[derive(Clone)]
pub struct Texture {
    pub name: String,
    /// Tamanho real de uma repetição da imagem (largura, altura) em metros.
    pub size: [f64; 2],
    pub image: Arc<Image>,
    /// Identificador do conteúdo (cache na GPU).
    pub uid: u64,
}

impl Texture {
    pub fn new(name: impl Into<String>, size: [f64; 2], image: Image) -> Self {
        // FNV-1a do conteúdo: a mesma imagem tem o mesmo uid.
        let mut h: u64 = 0xcbf29ce484222325;
        for b in image.rgba.iter().step_by(7).chain(image.w.to_le_bytes().iter()) {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        Self { name: name.into(), size, image: Arc::new(image), uid: h }
    }

    pub fn avg(&self) -> [f32; 3] {
        self.image.avg
    }
}

// ---------------------------------------------------------------- arquivo

pub fn to_png_base64(img: &Image) -> String {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.w, img.h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("cabeçalho PNG");
        w.write_image_data(&img.rgba).expect("dados PNG");
    }
    base64::engine::general_purpose::STANDARD.encode(out)
}

pub fn decode_bytes(bytes: &[u8]) -> Result<Image, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("Imagem inválida: {e}"))?;
    // Texturas grandes demais pesam no arquivo e na placa de vídeo.
    let img = if img.width().max(img.height()) > 1024 {
        img.resize(1024, 1024, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Ok(Image::new(rgba.width(), rgba.height(), rgba.into_raw()))
}

pub fn from_png_base64(s: &str) -> Result<Image, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| format!("Textura corrompida: {e}"))?;
    decode_bytes(&bytes)
}

/// Importa uma imagem do disco; `width_m` é a largura real de uma repetição.
pub fn import(path: &std::path::Path, width_m: f64) -> Result<Texture, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Não foi possível ler: {e}"))?;
    let img = decode_bytes(&bytes)?;
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let h = width_m * img.h as f64 / img.w as f64;
    Ok(Texture::new(name, [width_m, h], img))
}

// ---------------------------------------------------------------- texturas geradas

/// Ruído periódico (repete a cada `period` células): a textura não tem emenda.
fn hash(x: i64, y: i64, seed: u64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F) ^ seed;
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    (h & 0xffffff) as f64 / 0xffffff as f64
}

fn noise(u: f64, v: f64, period: i64, seed: u64) -> f64 {
    let (x, y) = (u * period as f64, v * period as f64);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let g = |i: i64, j: i64| hash((x0 as i64 + i).rem_euclid(period), (y0 as i64 + j).rem_euclid(period), seed);
    let a = g(0, 0) + (g(1, 0) - g(0, 0)) * sx;
    let b = g(0, 1) + (g(1, 1) - g(0, 1)) * sx;
    a + (b - a) * sy
}

fn fbm(u: f64, v: f64, period: i64, octaves: u32, seed: u64) -> f64 {
    let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
    for o in 0..octaves {
        sum += noise(u, v, period << o, seed + o as u64) * amp;
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn render(size: u32, f: impl Fn(f64, f64) -> [f64; 3]) -> Image {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let c = f(x as f64 / size as f64, y as f64 / size as f64);
            for k in c {
                rgba.push((k.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            rgba.push(255);
        }
    }
    Image::new(size, size, rgba)
}

fn rgb(hex: u32) -> [f64; 3] {
    [((hex >> 16) & 255) as f64 / 255.0, ((hex >> 8) & 255) as f64 / 255.0, (hex & 255) as f64 / 255.0]
}

/// Texturas prontas: (nome, tamanho real em metros, gerador).
pub const BUILTIN: &[(&str, [f64; 2])] = &[
    ("Piso de madeira", [1.2, 1.2]),
    ("Porcelanato 60×60", [1.2, 1.2]),
    ("Mármore", [1.6, 1.6]),
    ("Concreto", [2.0, 2.0]),
    ("Tijolo aparente", [0.8, 0.8]),
    ("Linho", [0.3, 0.3]),
    ("Madeira clara", [1.0, 1.0]),
];

pub fn builtin(name: &str) -> Option<Texture> {
    let (_, size) = BUILTIN.iter().find(|(n, _)| *n == name)?;
    let n = 512;
    let img = match name {
        "Piso de madeira" => render(n, |u, v| {
            // 8 réguas na largura, emendas desencontradas
            let plank = (u * 8.0).floor();
            let fu = u * 8.0 - plank;
            let offset = hash(plank as i64, 7, 3) * 0.5;
            let vv = (v + offset).rem_euclid(1.0);
            let joint = (vv * 2.0).fract() < 0.006 || fu < 0.02;
            let tone = hash(plank as i64, (vv * 2.0).floor() as i64, 11) * 0.25;
            let grain = fbm(u * 8.0 / 8.0, vv, 8, 1, 5) * 0.4 + (fbm(fu * 0.2, vv * 6.0, 16, 3, 9) * 30.0).sin() * 0.06;
            let base = mix(rgb(0xb98a5a), rgb(0x8a5b34), tone + grain);
            if joint { mix(base, rgb(0x3d2a1a), 0.7) } else { base }
        }),
        "Porcelanato 60×60" => render(n, |u, v| {
            let grout = (u * 2.0).fract() < 0.004 || (v * 2.0).fract() < 0.004;
            let tile = hash((u * 2.0).floor() as i64, (v * 2.0).floor() as i64, 21) * 0.04;
            let speck = fbm(u, v, 32, 3, 23) * 0.06;
            let base = mix(rgb(0xd9d4cc), rgb(0xbfb8ad), tile + speck);
            if grout { rgb(0x9a958d) } else { base }
        }),
        "Mármore" => render(n, |u, v| {
            let t = fbm(u, v, 4, 5, 31);
            let vein = ((u * 3.0 + v * 2.0 + t * 6.0) * std::f64::consts::PI).sin().abs();
            let vein = (1.0 - vein).powf(10.0);
            mix(mix(rgb(0xf2f0ec), rgb(0xdcd8d2), fbm(u, v, 8, 3, 37)), rgb(0x8d8a86), vein * 0.8)
        }),
        "Concreto" => render(n, |u, v| {
            let t = fbm(u, v, 8, 5, 41);
            let pores = if hash((u * 180.0) as i64, (v * 180.0) as i64, 43) > 0.985 { 0.25 } else { 0.0 };
            mix(rgb(0xb2b0aa), rgb(0x8c8a85), t + pores)
        }),
        "Tijolo aparente" => render(n, |u, v| {
            // tijolo 19 × 5,7 cm, junta 1 cm, num quadro de 80 cm
            let (bw, bh, j) = (0.20 / 0.8, 0.067 / 0.8, 0.01 / 0.8);
            let row = (v / bh).floor();
            let uu = u + if row as i64 % 2 == 0 { 0.0 } else { bw / 2.0 };
            let col = (uu / bw).floor();
            let (fu, fv) = (uu - col * bw, v - row * bh);
            let mortar = fu < j || fv < j;
            let tone = hash(col as i64, row as i64, 51);
            let base = mix(rgb(0xa5553a), rgb(0x7d3a26), tone * 0.7 + fbm(u, v, 32, 2, 53) * 0.3);
            if mortar { mix(rgb(0xc9c2b5), rgb(0xa9a296), fbm(u, v, 64, 2, 55)) } else { base }
        }),
        "Linho" => render(n, |u, v| {
            let warp = ((u * 60.0 * std::f64::consts::TAU).sin() * 0.5 + 0.5) * 0.08;
            let weft = ((v * 60.0 * std::f64::consts::TAU).sin() * 0.5 + 0.5) * 0.08;
            mix(rgb(0xd8ccb4), rgb(0xb7a98f), warp + weft + fbm(u, v, 16, 3, 61) * 0.3)
        }),
        "Madeira clara" => render(n, |u, v| {
            let t = fbm(u * 0.5, v, 4, 4, 71);
            let rings = ((u * 18.0 + t * 4.0) * std::f64::consts::PI).sin() * 0.5 + 0.5;
            mix(rgb(0xe2c9a0), rgb(0xc9a36e), rings * 0.5 + fbm(u, v, 32, 2, 73) * 0.2)
        }),
        _ => return None,
    };
    Some(Texture::new(name, *size, img))
}

/// Coordenadas de textura (planas) de um ponto numa face: segue a base da face
/// (paredes: horizontal/vertical; pisos: X/Y), em repetições da imagem.
pub fn uv(p: glam::DVec3, n: glam::DVec3, size: [f64; 2]) -> [f32; 2] {
    let (bu, bv) = crate::geom::plane::basis_for(n);
    [(p.dot(bu) / size[0]) as f32, (-p.dot(bv) / size[1]) as f32]
}

/// Níveis de mipmap (redução 2×2) para a textura não "chiar" de longe.
pub fn mipmaps(img: &Image) -> Vec<(u32, u32, Vec<u8>)> {
    let mut out = vec![(img.w, img.h, img.rgba.clone())];
    loop {
        let (w, h, data) = out.last().unwrap();
        if *w == 1 && *h == 1 {
            break;
        }
        let (nw, nh) = ((*w / 2).max(1), (*h / 2).max(1));
        let mut next = Vec::with_capacity((nw * nh * 4) as usize);
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let px = |xx: u32, yy: u32| data[((yy.min(h - 1) * w + xx.min(w - 1)) * 4 + c) as usize] as u32;
                    let s = px(2 * x, 2 * y) + px(2 * x + 1, 2 * y) + px(2 * x, 2 * y + 1) + px(2 * x + 1, 2 * y + 1);
                    next.push((s / 4) as u8);
                }
            }
        }
        out.push((nw, nh, next));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_seamless_and_have_average_color() {
        for (name, size) in BUILTIN {
            let t = builtin(name).expect(name);
            assert_eq!(t.size, *size);
            let img = &t.image;
            assert!(img.avg.iter().all(|c| *c > 0.05 && *c < 0.98), "{name}: {:?}", img.avg);
            // Sem emenda: nas texturas de ruído a primeira e a última coluna são parecidas.
            // (Réguas, tijolos e porcelanato têm juntas de propósito e são periódicos por construção.)
            if matches!(*name, "Piso de madeira" | "Tijolo aparente" | "Porcelanato 60×60") {
                continue;
            }
            let col = |x: u32| (0..img.h).map(move |y| img.rgba[((y * img.w + x) * 4) as usize] as f64);
            let diff: f64 = col(0).zip(col(img.w - 1)).map(|(a, b)| (a - b).abs()).sum::<f64>() / img.h as f64;
            assert!(diff < 40.0, "{name}: emenda visível ({diff})");
        }
    }

    #[test]
    fn png_roundtrip_and_mipmaps() {
        let t = builtin("Concreto").unwrap();
        let back = from_png_base64(&to_png_base64(&t.image)).unwrap();
        assert_eq!((back.w, back.h), (t.image.w, t.image.h));
        assert_eq!(back.rgba, t.image.rgba);
        let mips = mipmaps(&t.image);
        assert_eq!(mips.len(), 10); // 512 → 1
        assert_eq!((mips.last().unwrap().0, mips.last().unwrap().1), (1, 1));
    }

    #[test]
    fn uv_follows_real_size_and_wall_orientation() {
        use glam::DVec3;
        // parede (normal −Y): u horizontal, v vertical; 1,2 m = 1 repetição
        let a = uv(DVec3::new(1.2, 0.0, 0.0), -DVec3::Y, [1.2, 1.2]);
        let b = uv(DVec3::new(0.0, 0.0, 1.2), -DVec3::Y, [1.2, 1.2]);
        assert!((a[0].abs() - 1.0).abs() < 1e-6 && a[1].abs() < 1e-6);
        assert!(b[0].abs() < 1e-6 && (b[1].abs() - 1.0).abs() < 1e-6);
    }
}
