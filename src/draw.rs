//! Primitivas de desenho no estilo do Cairo sobre o tiny-skia, e os ícones da barra.

use std::f32::consts::PI;

use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke,
    StrokeDash, Transform,
};

use crate::text::{self, Weight};

pub type Rgb = [f32; 3];

pub fn paint(c: Rgb, a: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba(c[0], c[1], c[2], a.clamp(0.0, 1.0)).unwrap_or(Color::BLACK));
    p.anti_alias = true;
    p
}

pub fn fill(pm: &mut Pixmap, path: Option<Path>, c: Rgb, a: f32, ts: Transform) {
    if let Some(p) = path {
        pm.fill_path(&p, &paint(c, a), FillRule::Winding, ts, None);
    }
}

#[derive(Clone)]
pub struct Line {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub dash: Option<Vec<f32>>,
}

impl Line {
    pub fn round(width: f32) -> Self {
        Line { width, cap: LineCap::Round, join: LineJoin::Round, dash: None }
    }
    pub fn butt(width: f32) -> Self {
        Line { width, cap: LineCap::Butt, join: LineJoin::Miter, dash: None }
    }
}

pub fn stroke(pm: &mut Pixmap, path: Option<Path>, c: Rgb, a: f32, line: &Line, ts: Transform) {
    let Some(p) = path else { return };
    let mut s = Stroke {
        width: line.width.max(0.01),
        line_cap: line.cap,
        line_join: line.join,
        ..Default::default()
    };
    if let Some(d) = &line.dash {
        s.dash = StrokeDash::new(d.clone(), 0.0);
    }
    pm.stroke_path(&p, &paint(c, a), &s, ts, None);
}

/// Construtor de caminhos com semântica de "ponto corrente" igual à do Cairo
/// (`arc` liga com uma linha ao ponto anterior, `new_sub_path` desliga isso).
pub struct Pb {
    b: PathBuilder,
    cur: bool,
}

impl Default for Pb {
    fn default() -> Self {
        Self::new()
    }
}

impl Pb {
    pub fn new() -> Self {
        Pb { b: PathBuilder::new(), cur: false }
    }
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.b.move_to(x, y);
        self.cur = true;
        self
    }
    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        if self.cur {
            self.b.line_to(x, y);
        } else {
            self.move_to(x, y);
        }
        self
    }
    pub fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) -> &mut Self {
        if !self.cur {
            self.move_to(x1, y1);
        }
        self.b.cubic_to(x1, y1, x2, y2, x, y);
        self
    }
    pub fn close(&mut self) -> &mut Self {
        self.b.close();
        self
    }
    pub fn new_sub_path(&mut self) -> &mut Self {
        self.cur = false;
        self
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        if let Some(r) = Rect::from_xywh(x, y, w, h) {
            self.b.push_rect(r);
        }
        self.cur = false;
        self
    }
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32) -> &mut Self {
        self.b.push_circle(cx, cy, r);
        self.cur = false;
        self
    }
    pub fn oval(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        if let Some(r) = Rect::from_xywh(x, y, w, h) {
            self.b.push_oval(r);
        }
        self.cur = false;
        self
    }
    /// Arco no sentido horário (ângulos crescentes, y para baixo), como `cairo_arc`.
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> &mut Self {
        let mut a1 = a1;
        while a1 < a0 {
            a1 += 2.0 * PI;
        }
        self.line_to(cx + r * a0.cos(), cy + r * a0.sin());
        let n = ((a1 - a0) / (PI / 2.0)).ceil().max(1.0) as usize;
        let step = (a1 - a0) / n as f32;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let mut a = a0;
        for _ in 0..n {
            let b = a + step;
            let (ca, sa, cb, sb) = (a.cos(), a.sin(), b.cos(), b.sin());
            self.b.cubic_to(
                cx + r * (ca - k * sa),
                cy + r * (sa + k * ca),
                cx + r * (cb + k * sb),
                cy + r * (sb - k * cb),
                cx + r * cb,
                cy + r * sb,
            );
            a = b;
        }
        self
    }
    pub fn finish(self) -> Option<Path> {
        self.b.finish()
    }
}

pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let mut p = Pb::new();
    p.new_sub_path()
        .arc(x + w - r, y + r, r, -PI / 2.0, 0.0)
        .arc(x + w - r, y + h - r, r, 0.0, PI / 2.0)
        .arc(x + r, y + h - r, r, PI / 2.0, PI)
        .arc(x + r, y + r, r, PI, 3.0 * PI / 2.0)
        .close();
    p.finish()
}

pub fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    let mut p = Pb::new();
    p.rect(x, y, w, h);
    p.finish()
}

/// Ponta de seta preenchida apontando para (x2, y2).
pub fn arrow_head(x1: f32, y1: f32, x2: f32, y2: f32, head: f32) -> Option<Path> {
    let ang = (y2 - y1).atan2(x2 - x1);
    let mut p = Pb::new();
    p.move_to(x2, y2)
        .line_to(x2 - head * (ang - 0.42).cos(), y2 - head * (ang - 0.42).sin())
        .line_to(x2 - head * (ang + 0.42).cos(), y2 - head * (ang + 0.42).sin())
        .close();
    p.finish()
}

/// Desenha `s` com a linha de base em (x, y).
#[allow(clippy::too_many_arguments)]
pub fn text(pm: &mut Pixmap, w: Weight, size: f32, s: &str, x: f32, y: f32, c: Rgb, a: f32, ts: Transform) {
    fill(pm, text::path(w, size, s, x, y), c, a, ts);
}

/// Desenha `s` centralizado (pela tinta) em (cx, cy).
#[allow(clippy::too_many_arguments)]
pub fn text_centered(pm: &mut Pixmap, w: Weight, size: f32, s: &str, cx: f32, cy: f32, c: Rgb, ts: Transform) {
    let e = text::extents(w, size, s);
    text(pm, w, size, s, cx - e.width / 2.0 - e.x_bearing, cy - e.height / 2.0 - e.y_bearing, c, 1.0, ts);
}

pub const ICON_FG: Rgb = [0.93, 0.93, 0.95];

/// Ícones da barra, desenhados em ~18 unidades centrados em (cx, cy) com escala `u`.
pub fn icon(pm: &mut Pixmap, kind: &str, cx: f32, cy: f32, u: f32, color: Rgb, ts: Transform) {
    // tudo é desenhado num sistema local de 1 unidade = 1 px lógico, centrado no ícone
    let ts = ts.pre_translate(cx, cy).pre_scale(u, u);
    let ln = Line::round(1.8);
    let fg = ICON_FG;
    match kind {
        "move" => {
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let mut p = Pb::new();
                p.move_to(0.0, 0.0).line_to(dx * 8.0, dy * 8.0);
                stroke(pm, p.finish(), fg, 1.0, &ln, ts);
                fill(pm, arrow_head(0.0, 0.0, dx * 9.0, dy * 9.0, 5.0), fg, 1.0, ts);
            }
        }
        "pen" => {
            let mut p = Pb::new();
            p.move_to(-8.0, 5.0)
                .curve_to(-4.0, -8.0, 0.0, 8.0, 3.0, -2.0)
                .curve_to(5.0, -7.0, 7.0, -4.0, 8.0, -6.0);
            stroke(pm, p.finish(), fg, 1.0, &ln, ts);
        }
        "line" => {
            let mut p = Pb::new();
            p.move_to(-7.0, 7.0).line_to(7.0, -7.0);
            stroke(pm, p.finish(), fg, 1.0, &ln, ts);
        }
        "arrow" => {
            let mut p = Pb::new();
            p.move_to(-7.0, 7.0).line_to(4.0, -4.0);
            stroke(pm, p.finish(), fg, 1.0, &ln, ts);
            fill(pm, arrow_head(-7.0, 7.0, 8.0, -8.0, 8.0), fg, 1.0, ts);
        }
        "rect" => {
            stroke(pm, rect_path(-8.0, -6.0, 16.0, 12.0), fg, 1.0, &ln, ts);
        }
        "ellipse" => {
            let mut p = Pb::new();
            p.oval(-9.0, -7.0, 18.0, 14.0);
            stroke(pm, p.finish(), fg, 1.0, &ln, ts);
        }
        "text" => text_centered(pm, Weight::Bold, 19.0, "T", 0.0, 0.0, fg, ts),
        "marker" => {
            fill(pm, rect_path(-9.0, -4.0, 18.0, 8.0), color, 0.55, ts);
            let mut p = Pb::new();
            p.move_to(-9.0, 7.0).line_to(9.0, 7.0);
            stroke(pm, p.finish(), fg, 1.0, &ln, ts);
        }
        "pixelate" => {
            for i in 0..3 {
                for j in 0..3 {
                    let shade = 0.35 + 0.6 * ((i + j) % 2) as f32;
                    let r = rect_path(-9.0 + i as f32 * 6.0, -9.0 + j as f32 * 6.0, 6.0, 6.0);
                    fill(pm, r, [shade, shade, shade + 0.02], 1.0, ts);
                }
            }
        }
        "step" => {
            let mut p = Pb::new();
            p.circle(0.0, 0.0, 9.0);
            fill(pm, p.finish(), fg, 1.0, ts);
            text_centered(pm, Weight::Bold, 12.0, "1", 0.0, 0.0, [0.13, 0.13, 0.15], ts);
        }
        "undo" | "redo" => {
            let d = if kind == "undo" { -1.0 } else { 1.0 };
            let mut p = Pb::new();
            p.arc(0.0, 3.0, 6.0, PI, 2.0 * PI); // meia-volta por cima
            stroke(pm, p.finish(), fg, 1.0, &ln, ts);
            let tipx = 6.0 * d;
            fill(pm, arrow_head(tipx, 0.0, tipx, 8.0, 6.0), fg, 1.0, ts);
        }
        "ok" => {
            let mut p = Pb::new();
            p.move_to(-7.0, 0.0).line_to(-2.0, 6.0).line_to(8.0, -6.0);
            stroke(pm, p.finish(), [0.35, 0.85, 0.45], 1.0, &Line::round(2.6), ts);
        }
        "close" => {
            let mut p = Pb::new();
            p.move_to(-6.0, -6.0).line_to(6.0, 6.0).move_to(6.0, -6.0).line_to(-6.0, 6.0);
            stroke(pm, p.finish(), [0.95, 0.45, 0.45], 1.0, &Line::round(2.4), ts);
        }
        _ => {}
    }
}

/// Ícone do app (o mesmo desenho de `recortei.svg`) em RGBA não pré-multiplicado.
/// Com `badge`, desenha sobre um quadrado azul arredondado (bandejas claras do Windows).
pub fn app_icon_rgba(size: u32, badge: bool) -> Vec<u8> {
    let mut pm = Pixmap::new(size, size).expect("tamanho do ícone");
    let s = size as f32 / 22.0;
    let ts = Transform::from_scale(s, s);
    let ln = Line { width: 2.0, cap: LineCap::Round, join: LineJoin::Miter, dash: None };
    let marks = || {
        let mut p = Pb::new();
        p.move_to(6.0, 2.0).line_to(6.0, 16.0).line_to(20.0, 16.0);
        p.move_to(2.0, 6.0).line_to(16.0, 6.0).line_to(16.0, 20.0);
        p.finish()
    };
    if badge {
        fill(&mut pm, rounded_rect(0.0, 0.0, 22.0, 22.0, 5.0), [0.13, 0.47, 0.95], 1.0, ts);
        // marcas um pouco menores para caber dentro do quadrado
        let inner = ts.pre_translate(2.2, 2.2).pre_scale(0.8, 0.8);
        stroke(&mut pm, marks(), [1.0, 1.0, 1.0], 1.0, &Line { width: 2.4, ..ln }, inner);
    } else {
        // sombra leve para o ícone aparecer em painéis claros
        stroke(&mut pm, marks(), [0.0, 0.0, 0.0], 0.45, &Line { width: 3.6, ..ln.clone() }, ts);
        stroke(&mut pm, marks(), [1.0, 1.0, 1.0], 1.0, &ln, ts);
    }
    pm.take_demultiplied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_render_without_panicking() {
        let mut pm = Pixmap::new(64, 64).unwrap();
        for k in [
            "move", "pen", "line", "arrow", "rect", "ellipse", "text", "marker", "pixelate", "step",
            "undo", "redo", "ok", "close",
        ] {
            pm.fill(Color::TRANSPARENT);
            icon(&mut pm, k, 32.0, 32.0, 1.5, [1.0, 0.0, 0.0], Transform::identity());
            assert!(pm.pixels().iter().any(|p| p.alpha() > 0), "ícone vazio: {k}");
        }
        for badge in [false, true] {
            let rgba = app_icon_rgba(32, badge);
            assert_eq!(rgba.len(), 32 * 32 * 4);
            assert!(rgba.chunks_exact(4).any(|p| p[3] == 255));
        }
    }

    #[test]
    fn arc_matches_circle() {
        let mut p = Pb::new();
        p.arc(0.0, 0.0, 10.0, 0.0, PI);
        let b = p.finish().unwrap().bounds();
        assert!((b.left() + 10.0).abs() < 0.01 && (b.right() - 10.0).abs() < 0.01);
        assert!(b.top().abs() < 0.01 && (b.bottom() - 10.0).abs() < 0.2);
    }
}
