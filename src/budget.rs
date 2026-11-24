// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Orçamento: chapas + fita de borda + ferragens, mão de obra e margem.

use crate::cutting::Plan;
use crate::project::{Project, Takeoff};

#[derive(Clone, Debug)]
pub struct Line {
    pub group: &'static str,
    pub desc: String,
    pub qty: f64,
    pub unit: &'static str,
    pub unit_price: f64,
    pub total: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Budget {
    pub lines: Vec<Line>,
    pub material: f64,
    pub labor: f64,
    pub cost: f64,
    pub margin: f64,
    pub total: f64,
}

pub fn compute(p: &Project, t: &Takeoff, plan: &Plan) -> Budget {
    let cat = &p.catalog;
    let mut lines = Vec::new();
    for mi in plan.materials() {
        let m = cat.material(mi);
        if cat.budget.whole_sheets {
            let n = plan.sheets_of(mi) as f64;
            lines.push(Line { group: "Chapas", desc: m.name.clone(), qty: n, unit: "chapa", unit_price: m.sheet_price, total: n * m.sheet_price });
        } else {
            let sheet_m2 = m.sheet[0] * m.sheet[1] / 1e6;
            let area = plan.piece_area_m2(mi) * (1.0 + cat.budget.loss_pct / 100.0);
            let price = m.sheet_price / sheet_m2;
            lines.push(Line { group: "Chapas", desc: m.name.clone(), qty: round2(area), unit: "m²", unit_price: price, total: area * price });
        }
    }
    let mut edges: Vec<(usize, f64)> = Vec::new();
    for (_, b) in &t.boards {
        let e = b.edge_m();
        if e > 0.0 {
            match edges.iter_mut().find(|(m, _)| *m == b.material) {
                Some((_, s)) => *s += e,
                None => edges.push((b.material, e)),
            }
        }
    }
    edges.sort_by_key(|e| e.0);
    for (mi, meters) in edges {
        let m = cat.material(mi);
        // 10% a mais para as pontas aparadas.
        let qty = (meters * 1.1).ceil();
        lines.push(Line { group: "Fita de borda", desc: format!("Fita {}", m.name), qty, unit: "m", unit_price: m.edge_price, total: qty * m.edge_price });
    }
    for (k, q) in &t.hardware {
        let price = cat.hardware_price(*k);
        lines.push(Line { group: "Ferragens", desc: k.label().into(), qty: round2(*q), unit: k.unit(), unit_price: price, total: q * price });
    }
    let material: f64 = lines.iter().map(|l| l.total).sum();
    let labor = material * cat.budget.labor_pct / 100.0;
    let cost = material + labor;
    let margin = cost * cat.budget.margin_pct / 100.0;
    Budget { lines, material, labor, cost, margin, total: cost + margin }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// R$ 1.234,56
pub fn brl(v: f64) -> String {
    let cents = (v * 100.0).round() as i64;
    let (int, frac) = (cents.abs() / 100, cents.abs() % 100);
    let s = int.to_string();
    let mut grouped = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    format!("{}R$ {grouped},{frac:02}", if cents < 0 { "−" } else { "" })
}

/// Número com vírgula decimal (sem zeros à toa).
pub fn num(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_format() {
        assert_eq!(brl(1234.5), "R$ 1.234,50");
        assert_eq!(brl(0.4), "R$ 0,40");
        assert_eq!(brl(1_000_000.0), "R$ 1.000.000,00");
        assert_eq!(num(2.5), "2,5");
        assert_eq!(num(3.0), "3");
    }

    #[test]
    fn budget_adds_labor_and_margin() {
        let p = Project::demo_kitchen();
        let t = p.takeoff();
        let plan = p.cut_plan(&t);
        let b = compute(&p, &t, &plan);
        assert!(b.material > 0.0);
        assert!((b.labor - b.material * 0.45).abs() < 1e-6);
        assert!((b.total - b.material * 1.45 * 1.30).abs() < 1e-6);
        assert!(b.lines.iter().any(|l| l.group == "Ferragens"));
    }
}
