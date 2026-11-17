//! Texto vetorial: os glifos da fonte embutida viram caminhos do tiny-skia,
//! então o texto é desenhado com antialiasing em qualquer escala/transformação.

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, OutlineCurve};
use tiny_skia::{Path, PathBuilder};

static BOLD_TTF: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
static REGULAR_TTF: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Regular,
    Bold,
}

fn font(weight: Weight) -> &'static FontRef<'static> {
    static BOLD: OnceLock<FontRef<'static>> = OnceLock::new();
    static REGULAR: OnceLock<FontRef<'static>> = OnceLock::new();
    match weight {
        Weight::Bold => BOLD.get_or_init(|| FontRef::try_from_slice(BOLD_TTF).expect("fonte embutida")),
        Weight::Regular => {
            REGULAR.get_or_init(|| FontRef::try_from_slice(REGULAR_TTF).expect("fonte embutida"))
        }
    }
}

/// Fator que converte unidades da fonte em pixels para um tamanho de "em" (como no Cairo).
fn units_scale(f: &FontRef, size: f32) -> f32 {
    size / f.units_per_em().unwrap_or(2048.0)
}

/// Métricas verticais, equivalentes a `cairo_font_extents`.
#[derive(Clone, Copy, Debug)]
pub struct FontMetrics {
    pub ascent: f32,
    /// Altura de linha (ascent + descent + line gap).
    pub height: f32,
}

pub fn metrics(weight: Weight, size: f32) -> FontMetrics {
    let f = font(weight);
    let sf = units_scale(f, size);
    FontMetrics {
        ascent: f.ascent_unscaled() * sf,
        height: (f.ascent_unscaled() - f.descent_unscaled() + f.line_gap_unscaled()) * sf,
    }
}

/// Extensões de um texto, equivalentes a `cairo_text_extents`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Extents {
    pub x_bearing: f32,
    pub y_bearing: f32,
    pub width: f32,
    pub height: f32,
    pub x_advance: f32,
}

/// Percorre os glifos de `s`, chamando `each(glyph_id, x_da_origem)`; devolve o avanço total.
fn layout(f: &FontRef, sf: f32, s: &str, mut each: impl FnMut(ab_glyph::GlyphId, f32)) -> f32 {
    let mut pen = 0.0;
    let mut prev = None;
    for ch in s.chars() {
        let gid = f.glyph_id(ch);
        if let Some(p) = prev {
            pen += f.kern_unscaled(p, gid) * sf;
        }
        each(gid, pen);
        pen += f.h_advance_unscaled(gid) * sf;
        prev = Some(gid);
    }
    pen
}

pub fn extents(weight: Weight, size: f32, s: &str) -> Extents {
    let f = font(weight);
    let sf = units_scale(f, size);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let adv = layout(f, sf, s, |gid, pen| {
        if let Some(o) = f.outline(gid) {
            // unidades da fonte têm y para cima; convertemos para y para baixo.
            // (o ab_glyph guarda min.y = topo, max.y = base; usamos min/max por segurança)
            let (top, bottom) = (o.bounds.min.y.max(o.bounds.max.y), o.bounds.min.y.min(o.bounds.max.y));
            x0 = x0.min(pen + o.bounds.min.x * sf);
            x1 = x1.max(pen + o.bounds.max.x * sf);
            y0 = y0.min(-top * sf);
            y1 = y1.max(-bottom * sf);
        }
    });
    if x0 > x1 {
        return Extents { x_advance: adv, ..Default::default() };
    }
    Extents { x_bearing: x0, y_bearing: y0, width: x1 - x0, height: y1 - y0, x_advance: adv }
}

/// Caminho do texto com a linha de base começando em (x, y), como `cairo_show_text`.
pub fn path(weight: Weight, size: f32, s: &str, x: f32, y: f32) -> Option<Path> {
    let f = font(weight);
    let sf = units_scale(f, size);
    let mut pb = PathBuilder::new();
    layout(f, sf, s, |gid, pen| {
        let Some(o) = f.outline(gid) else { return };
        let tx = |p: ab_glyph::Point| (x + pen + p.x * sf, y - p.y * sf);
        let mut last: Option<ab_glyph::Point> = None;
        for c in &o.curves {
            let (start, end) = match *c {
                OutlineCurve::Line(a, b) => (a, b),
                OutlineCurve::Quad(a, _, b) => (a, b),
                OutlineCurve::Cubic(a, _, _, b) => (a, b),
            };
            if last != Some(start) {
                if last.is_some() {
                    pb.close();
                }
                let (px, py) = tx(start);
                pb.move_to(px, py);
            }
            match *c {
                OutlineCurve::Line(_, b) => {
                    let (bx, by) = tx(b);
                    pb.line_to(bx, by);
                }
                OutlineCurve::Quad(_, c1, b) => {
                    let ((cx, cy), (bx, by)) = (tx(c1), tx(b));
                    pb.quad_to(cx, cy, bx, by);
                }
                OutlineCurve::Cubic(_, c1, c2, b) => {
                    let ((ax, ay), (cx, cy), (bx, by)) = (tx(c1), tx(c2), tx(b));
                    pb.cubic_to(ax, ay, cx, cy, bx, by);
                }
            }
            last = Some(end);
        }
        if last.is_some() {
            pb.close();
        }
    });
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extents_are_sane() {
        let e = extents(Weight::Bold, 20.0, "Ab");
        assert!(e.width > 10.0 && e.width < 40.0, "{e:?}");
        assert!(e.y_bearing < 0.0 && e.height > 10.0, "{e:?}");
        assert!(e.x_advance >= e.width);
        let m = metrics(Weight::Bold, 20.0);
        assert!(m.ascent > 14.0 && m.height > m.ascent);
        assert!(path(Weight::Regular, 12.0, "Olá ção", 0.0, 0.0).is_some());
        // o caminho fica acima da linha de base e bate com as extensões
        let b = path(Weight::Bold, 20.0, "Ab", 10.0, 50.0).unwrap().bounds();
        assert!((b.top() - (50.0 + e.y_bearing)).abs() < 0.5, "{b:?} {e:?}");
        assert!((b.bottom() - (50.0 + e.y_bearing + e.height)).abs() < 0.5, "{b:?}");
        assert!((b.left() - (10.0 + e.x_bearing)).abs() < 0.5, "{b:?}");
        assert!(path(Weight::Regular, 12.0, " ", 0.0, 0.0).is_none());
    }
}
