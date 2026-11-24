// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Relatórios: lista de peças (CSV), plano de corte (SVG) e orçamento (HTML para imprimir/PDF).

use std::fmt::Write as _;

use crate::budget::{Budget, brl, num};
use crate::cutting::Plan;
use crate::project::{Project, Takeoff};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Lista de peças no formato aceito pelos otimizadores (separador `;`, medidas em mm).
pub fn parts_csv(p: &Project, t: &Takeoff) -> String {
    let mut s = String::from("Módulo;Peça;Comprimento;Largura;Espessura;Material;Fita C1;Fita C2;Fita L1;Fita L2;Veio\n");
    for (mi, b) in &t.boards {
        let m = p.catalog.material(b.material);
        let e = b.edges.map(|x| if x { "S" } else { "N" });
        let _ = writeln!(
            s,
            "{} {};{};{};{};{};{};{};{};{};{};{}",
            mi + 1,
            p.modules[*mi].name,
            b.name,
            num(b.length),
            num(b.width),
            num(b.thickness),
            m.name,
            e[0],
            e[1],
            e[2],
            e[3],
            if m.grain { "S" } else { "N" }
        );
    }
    s
}

/// Cor estável por módulo para desenhar as peças.
pub fn module_color(i: usize) -> [u8; 3] {
    const P: [[u8; 3]; 10] = [
        [0xd6, 0xe4, 0xf7],
        [0xf7, 0xe0, 0xc8],
        [0xd9, 0xef, 0xd4],
        [0xf3, 0xd5, 0xe1],
        [0xe6, 0xdd, 0xf5],
        [0xf5, 0xef, 0xc4],
        [0xcf, 0xec, 0xec],
        [0xf2, 0xd9, 0xd0],
        [0xdf, 0xe6, 0xc9],
        [0xe1, 0xe1, 0xe6],
    ];
    P[i % P.len()]
}

/// Plano de corte em SVG (uma chapa embaixo da outra).
pub fn cut_svg(p: &Project, plan: &Plan) -> String {
    let scale = 0.3; // px por mm
    let trim = p.catalog.cut.trim;
    let mut y = 20.0;
    let mut body = String::new();
    let mut width: f64 = 0.0;
    for (si, s) in plan.sheets.iter().enumerate() {
        let m = p.catalog.material(s.material);
        let (sw, sh) = (m.sheet[0] * scale, m.sheet[1] * scale);
        width = width.max(sw + 40.0);
        let _ = writeln!(
            body,
            r#"<text x="20" y="{:.1}" font-size="14" font-weight="600">Chapa {} — {} ({} × {} mm) — aproveitamento {:.0}%</text>"#,
            y + 14.0,
            si + 1,
            esc(&m.name),
            m.sheet[0],
            m.sheet[1],
            s.usage * 100.0
        );
        y += 24.0;
        let _ = writeln!(body, r##"<rect x="20" y="{y:.1}" width="{sw:.1}" height="{sh:.1}" fill="#eee" stroke="#555"/>"##);
        for pl in &s.placed {
            let piece = &plan.pieces[pl.piece];
            let c = module_color(piece.module);
            let (x0, y0) = (20.0 + (trim + pl.x) * scale, y + (trim + pl.y) * scale);
            let (w, h) = (pl.w * scale, pl.h * scale);
            let _ = writeln!(
                body,
                r##"<rect x="{x0:.1}" y="{y0:.1}" width="{w:.1}" height="{h:.1}" fill="rgb({},{},{})" stroke="#333" stroke-width="0.6"/>"##,
                c[0], c[1], c[2]
            );
            if w > 40.0 && h > 18.0 {
                let fs = (h * 0.28).clamp(6.0, 11.0);
                let _ = writeln!(
                    body,
                    r#"<text x="{:.1}" y="{:.1}" font-size="{fs:.1}" text-anchor="middle">{} {}</text><text x="{:.1}" y="{:.1}" font-size="{fs:.1}" text-anchor="middle">{} × {}</text>"#,
                    x0 + w / 2.0,
                    y0 + h / 2.0 - fs * 0.2,
                    piece.module + 1,
                    esc(&piece.label),
                    x0 + w / 2.0,
                    y0 + h / 2.0 + fs,
                    num(piece.length),
                    num(piece.width)
                );
            }
        }
        y += sh + 30.0;
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{y:.0}" viewBox="0 0 {width:.0} {y:.0}" font-family="Helvetica, Arial, sans-serif">{body}</svg>"#
    )
}

/// Proposta/orçamento em HTML (abre no navegador; Imprimir › Salvar como PDF).
pub fn budget_html(p: &Project, t: &Takeoff, plan: &Plan, b: &Budget) -> String {
    let mut rows = String::new();
    let mut group = "";
    for l in &b.lines {
        if l.group != group {
            group = l.group;
            let _ = write!(rows, r#"<tr class="g"><td colspan="5">{group}</td></tr>"#);
        }
        let _ = write!(
            rows,
            "<tr><td>{}</td><td class=n>{}</td><td>{}</td><td class=n>{}</td><td class=n>{}</td></tr>",
            esc(&l.desc),
            num(l.qty),
            l.unit,
            brl(l.unit_price),
            brl(l.total)
        );
    }
    let mut modules = String::new();
    for (i, m) in p.modules.iter().enumerate() {
        let parts = t.boards.iter().filter(|(mi, _)| *mi == i).count();
        let _ = write!(
            modules,
            "<tr><td>{}</td><td>{}</td><td class=n>{} × {} × {}</td><td>{}</td><td class=n>{parts}</td></tr>",
            i + 1,
            esc(&m.name),
            num(m.w),
            num(m.h),
            num(m.d),
            esc(&p.catalog.material(m.front_mat).name)
        );
    }
    let mut parts = String::new();
    for (mi, bd) in &t.boards {
        let _ = write!(
            parts,
            "<tr><td>{}</td><td>{}</td><td class=n>{}</td><td class=n>{}</td><td class=n>{}</td><td>{}</td><td class=n>{}</td></tr>",
            mi + 1,
            esc(&bd.name),
            num(bd.length),
            num(bd.width),
            num(bd.thickness),
            esc(&p.catalog.material(bd.material).name),
            num(bd.edge_m())
        );
    }
    format!(
        r#"<!doctype html><html lang="pt-BR"><head><meta charset="utf-8"><title>Orçamento — {name}</title>
<style>
body{{font-family:Helvetica,Arial,sans-serif;color:#222;margin:32px;max-width:1000px}}
h1{{margin:0 0 4px}} h2{{margin-top:32px;border-bottom:2px solid #222;padding-bottom:4px}}
table{{border-collapse:collapse;width:100%;font-size:13px}} td,th{{padding:5px 8px;border-bottom:1px solid #ddd;text-align:left}}
.n{{text-align:right;white-space:nowrap}} tr.g td{{background:#f2f2f2;font-weight:600}}
.tot td{{font-size:15px}} .big td{{font-size:20px;font-weight:700;border-top:2px solid #222}}
.muted{{color:#777}} @media print{{h2{{page-break-after:avoid}} .pb{{page-break-before:always}}}}
</style></head><body>
<h1>{name}</h1><div class=muted>Cliente: {client} · {n} módulos · {np} peças · {ns} chapas</div>
<h2>Módulos</h2>
<table><tr><th>#</th><th>Módulo</th><th class=n>L × A × P (mm)</th><th>Frente</th><th class=n>Peças</th></tr>{modules}</table>
<h2>Orçamento</h2>
<table><tr><th>Item</th><th class=n>Qtd.</th><th>Un.</th><th class=n>Preço un.</th><th class=n>Total</th></tr>{rows}
<tr class=tot><td colspan=4>Materiais</td><td class=n>{mat}</td></tr>
<tr class=tot><td colspan=4>Mão de obra ({lp}%)</td><td class=n>{labor}</td></tr>
<tr class=tot><td colspan=4>Margem ({mp}%)</td><td class=n>{margin}</td></tr>
<tr class=big><td colspan=4>Total</td><td class=n>{total}</td></tr></table>
<h2 class=pb>Lista de peças</h2>
<table><tr><th>Mód.</th><th>Peça</th><th class=n>Compr.</th><th class=n>Larg.</th><th class=n>Esp.</th><th>Material</th><th class=n>Fita (m)</th></tr>{parts}</table>
<h2 class=pb>Plano de corte</h2>{svg}
</body></html>"#,
        name = esc(&p.name),
        client = esc(if p.client.is_empty() { "—" } else { &p.client }),
        n = p.modules.len(),
        np = t.boards.len(),
        ns = plan.sheets.len(),
        mat = brl(b.material),
        lp = num(p.catalog.budget.labor_pct),
        labor = brl(b.labor),
        mp = num(p.catalog.budget.margin_pct),
        margin = brl(b.margin),
        total = brl(b.total),
        svg = cut_svg(p, plan).replacen("<svg ", r#"<svg style="max-width:100%;height:auto" "#, 1),
    )
}
