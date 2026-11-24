// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
// Shaders do viewport 3D. Profundidade com Z reverso (1 = perto, 0 = longe).

struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,        // xyz = posição do olho
    target_pt: vec4<f32>,  // xyz = alvo da câmera, w = raio de fade da grade
    viewport: vec4<f32>,   // largura px, altura px, pixels por ponto, 0
    light_dir: vec4<f32>,  // direção PARA a luz (mundo)
    bias: vec4<f32>,       // x = multiplicativo, y = aditivo
    back_color: vec4<f32>, // cor da face de trás (linear)
    bg_top: vec4<f32>,
    bg_bottom: vec4<f32>,
    shading: vec4<f32>,    // x = sombreado, y = texturas, z = raio-x, w = corte ativo
    clip: vec4<f32>,       // plano de seção: normal (xyz) e d (w); some o lado positivo
};

fn clipped(p: vec3<f32>) -> bool {
    return g.shading.w > 0.5 && dot(g.clip.xyz, p) - g.clip.w > 0.0;
}

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var tex_sampler: sampler;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// ---------------------------------------------------------------- fundo

struct BgOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) v: f32,
};

@vertex
fn vs_bg(@builtin(vertex_index) i: u32) -> BgOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: BgOut;
    o.pos = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.v = uv.y * 0.5;
    return o;
}

@fragment
fn fs_bg(in: BgOut) -> @location(0) vec4<f32> {
    let c = mix(g.bg_bottom.rgb, g.bg_top.rgb, clamp(in.v, 0.0, 1.0));
    return vec4<f32>(to_srgb(c), 1.0);
}

// ---------------------------------------------------------------- faces

struct FaceIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) textured: f32,
};

struct FaceOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) textured: f32,
    @location(4) world: vec3<f32>,
};

@vertex
fn vs_face(v: FaceIn) -> FaceOut {
    var o: FaceOut;
    o.pos = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.color = v.color;
    o.uv = v.uv;
    o.textured = v.textured;
    o.world = v.pos;
    return o;
}

@fragment
fn fs_face(in: FaceOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    // Amostra sempre (fluxo uniforme); só usa se a face tem textura e o estilo mostra texturas.
    let texel = textureSample(tex, tex_sampler, in.uv).rgb;
    var base = select(in.color.rgb, texel, in.textured > 0.5 && g.shading.y > 0.5);
    if (!front) {
        n = -n;
        base = g.back_color.rgb;
    }
    var c: vec3<f32>;
    if (g.shading.x < 0.5) {
        // Linhas ocultas: faces planas, sem luz.
        c = select(g.back_color.rgb, vec3<f32>(1.0), front);
    } else {
        let diffuse = max(dot(n, normalize(g.light_dir.xyz)), 0.0);
        c = base * (0.58 + 0.42 * diffuse);
    }
    if (!front && g.shading.w > 0.5) {
        // Com corte ativo, o "miolo" visível das peças cortadas fica escuro (poché).
        c = vec3<f32>(0.06, 0.06, 0.07);
    }
    if (in.color.a < 0.25) {
        // Fora do grupo em edição: esmaecida.
        c = mix(c, vec3<f32>(0.92, 0.93, 0.95), 0.62);
    } else if (in.color.a < 0.99) {
        // Face selecionada: realce azul.
        c = mix(c, vec3<f32>(0.10, 0.30, 1.0), 0.38);
    }
    if (clipped(in.world)) {
        discard;
    }
    // Raio-X (shading.z): face translúcida (alfa pré-multiplicado).
    let a = select(1.0, 0.42, g.shading.z > 0.5);
    return vec4<f32>(to_srgb(c) * a, a);
}

// ---------------------------------------------------------------- linhas
// Cada segmento é uma instância expandida em um retângulo na tela,
// o que permite espessura real e antialiasing (a GPU só desenha linhas de 1px).

struct LineIn {
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>, // largura (pt), sinal do bias, tracejado (pt), fade da grade (0/1)
};

struct LineOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(linear) side_px: f32,
    @location(2) @interpolate(linear) along_px: f32,
    @location(3) half_w: f32,
    @location(4) dash_px: f32,
    @location(5) world: vec3<f32>,
    @location(6) fade: f32,
};

@vertex
fn vs_line(@builtin(vertex_index) vi: u32, l: LineIn) -> LineOut {
    var o: LineOut;
    // Cantos do retângulo: (t ao longo, lado).
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let corner = corners[vi];

    var ca = g.view_proj * vec4<f32>(l.a, 1.0);
    var cb = g.view_proj * vec4<f32>(l.b, 1.0);
    var ta = 0.0;
    var tb = 1.0;

    // Recorte no plano próximo (Z reverso: dentro quando z <= w).
    let da = ca.w - ca.z;
    let db = cb.w - cb.z;
    if (da < 0.0 && db < 0.0) {
        o.pos = vec4<f32>(0.0, 0.0, 2.0, 1.0); // descartado
        return o;
    }
    if (da < 0.0) {
        ta = min(da / (da - db) + 1e-4, 1.0);
    } else if (db < 0.0) {
        tb = max(1.0 - db / (db - da) - 1e-4, 0.0);
    }
    let c0 = g.view_proj * vec4<f32>(mix(l.a, l.b, ta), 1.0);
    let c1 = g.view_proj * vec4<f32>(mix(l.a, l.b, tb), 1.0);

    let half = g.viewport.xy * 0.5;
    let p0 = c0.xy / c0.w * half;
    let p1 = c1.xy / c1.w * half;
    var d = p1 - p0;
    let len = length(d);
    if (len > 1e-6) { d = d / len; } else { d = vec2<f32>(1.0, 0.0); }
    let n = vec2<f32>(-d.y, d.x);

    let ppp = g.viewport.z;
    let half_w = l.params.x * ppp * 0.5;
    let ext = half_w + 1.0; // +1px para antialiasing

    let at_end = corner.x > 0.5;
    let base_c = select(c0, c1, at_end);
    let base_p = select(p0, p1, at_end);
    let along_sign = corner.x * 2.0 - 1.0;
    let p = base_p + n * corner.y * ext + d * along_sign * ext;

    var clip = vec4<f32>(p / half * base_c.w, base_c.z, base_c.w);
    let k = l.params.y;
    clip.z = clip.z * (1.0 + k * g.bias.x) + k * g.bias.y * clip.w;
    o.pos = clip;

    o.color = l.color;
    o.side_px = corner.y * ext;
    o.along_px = select(0.0, len, at_end) + along_sign * ext;
    o.half_w = half_w;
    o.dash_px = l.params.z * ppp;
    o.world = mix(l.a, l.b, select(ta, tb, at_end));
    o.fade = l.params.w;
    return o;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    // params.w: 0 = aresta do modelo (o corte esconde), 1 = grade, 2 = auxiliar (não corta)
    if (in.fade < 0.5 && clipped(in.world)) {
        discard;
    }
    var alpha = in.color.a * clamp(in.half_w + 0.5 - abs(in.side_px), 0.0, 1.0);
    if (in.dash_px > 0.0 && fract(in.along_px / (2.0 * in.dash_px)) > 0.5) {
        discard;
    }
    if (in.fade > 0.5 && in.fade < 1.5) {
        let r = length(in.world.xy - g.target_pt.xy);
        alpha = alpha * (1.0 - smoothstep(g.target_pt.w * 0.45, g.target_pt.w, r));
        // Some em ângulos rasantes (evita a faixa densa/moiré perto do horizonte).
        let to_pt = in.world - g.eye.xyz;
        let grazing = abs(to_pt.z) / max(length(to_pt), 1e-6);
        let ortho_grazing = abs(g.eye.w);
        alpha = alpha * smoothstep(0.02, 0.18, select(grazing, ortho_grazing, g.viewport.w > 0.5));
    }
    if (alpha <= 0.002) {
        discard;
    }
    return vec4<f32>(to_srgb(in.color.rgb) * alpha, alpha);
}

// Arestas atrás das faces: tracejadas e cinza.
@fragment
fn fs_line_hidden(in: LineOut) -> @location(0) vec4<f32> {
    if (clipped(in.world)) {
        discard;
    }
    let dash = 3.0 * g.viewport.z;
    if (fract(in.along_px / (2.0 * dash)) > 0.5) {
        discard;
    }
    let alpha = 0.55 * clamp(in.half_w + 0.5 - abs(in.side_px), 0.0, 1.0);
    if (alpha <= 0.002) {
        discard;
    }
    return vec4<f32>(vec3<f32>(0.42, 0.42, 0.44) * alpha, alpha);
}
