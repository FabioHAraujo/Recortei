//! Layout e desenho da janela de Configurações, puros: recebem um [`View`] e
//! desenham num `Pixmap` (unidades lógicas × escala HiDPI). Sem winit, para
//! testar o clique e gerar imagens de referência sem tela.

use tiny_skia::{Color, Pixmap, Transform};

use crate::draw::{self, Line, Pb, Rgb};
use crate::overlay::R;
use crate::text::{self, Weight};

/// Largura da janela (px lógicos).
pub const WIDTH: f32 = 560.0;
/// Altura do conteúdo, sem a barra de título própria.
const CONTENT_H: f32 = 420.0;
/// Barra de título desenhada pela janela (Wayland sem decoração do servidor).
pub const HEADER: f32 = 46.0;
const PAD: f32 = 24.0;
const FOOTER_H: f32 = 60.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    Preset(usize),
    Record,
    FixSnipping,
    Close,
    /// "×" da barra de título própria.
    HeaderClose,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Ok,
    Info,
    Warn,
    Error,
    Busy,
}

/// Tudo o que a janela mostra.
#[derive(Clone, Debug)]
pub struct View {
    pub light: bool,
    pub header: bool,
    /// Partes do atalho atual (`["Ctrl", "Shift", "S"]`).
    pub keys: Vec<String>,
    pub presets: Vec<String>,
    pub active_preset: Option<usize>,
    pub recording: bool,
    pub record_hint: Option<String>,
    pub status: (Tone, String),
    pub fix_button: bool,
    pub note: Option<String>,
    pub focus: Option<Btn>,
    pub hover: Option<Btn>,
    pub pressed: Option<Btn>,
}

/// Altura total da janela (px lógicos).
pub fn window_height(header: bool) -> f32 {
    CONTENT_H + if header { HEADER } else { 0.0 }
}

// ---------------------------------------------------------------- cores
struct Palette {
    bg: Rgb,
    header: Rgb,
    footer: Rgb,
    surface: Rgb,
    surface_hover: Rgb,
    surface_press: Rgb,
    border: (Rgb, f32),
    text: Rgb,
    muted: Rgb,
    accent: Rgb,
    /// Fundo do botão principal (texto branco com contraste AA).
    accent_fill: Rgb,
    accent_text: Rgb,
    /// Texto do atalho sugerido ativo, sobre o fundo azulado.
    accent_fg: Rgb,
    cap: Rgb,
    cap_edge: Rgb,
    ok: Rgb,
    info: Rgb,
    warn: Rgb,
    error: Rgb,
}

fn rgb(hex: u32) -> Rgb {
    let c = |s: u32| ((hex >> s) & 0xFF) as f32 / 255.0;
    [c(16), c(8), c(0)]
}

/// Escuro como a barra da sobreposição; claro no estilo do GNOME/Windows.
fn palette(light: bool) -> Palette {
    if light {
        Palette {
            bg: rgb(0xFAFAFB),
            header: rgb(0xEDEDF0),
            footer: rgb(0xF2F2F4),
            surface: rgb(0xFFFFFF),
            surface_hover: rgb(0xF0F2F6),
            surface_press: rgb(0xE3E6EC),
            border: ([0.0, 0.0, 0.0], 0.14),
            text: rgb(0x1D1D22),
            muted: rgb(0x5E5E6A),
            accent: rgb(0x1A66D9),
            accent_fill: rgb(0x1A66D9),
            accent_text: rgb(0xFFFFFF),
            accent_fg: rgb(0x1558C0),
            cap: rgb(0xFFFFFF),
            cap_edge: rgb(0xC9CBD3),
            ok: rgb(0x1E7F3B),
            info: rgb(0x1A66D9),
            warn: rgb(0xB06A00),
            error: rgb(0xC4291C),
        }
    } else {
        Palette {
            bg: rgb(0x1E1E22),
            header: rgb(0x18181B),
            footer: rgb(0x19191D),
            surface: rgb(0x2A2A30),
            surface_hover: rgb(0x34343B),
            surface_press: rgb(0x3D3D45),
            border: ([1.0, 1.0, 1.0], 0.10),
            text: rgb(0xEDEDF0),
            muted: rgb(0xA3A3AE),
            accent: rgb(0x3D8BFF),
            accent_fill: rgb(0x2563EB),
            accent_text: rgb(0xFFFFFF),
            accent_fg: rgb(0x6AA8FF),
            cap: rgb(0x3A3A42),
            cap_edge: rgb(0x121215),
            ok: rgb(0x5BD27A),
            info: rgb(0x6AA8FF),
            warn: rgb(0xF2B640),
            error: rgb(0xFF7A70),
        }
    }
}

impl Palette {
    /// Fundo e borda da caixa de situação: no tema claro, tons pastel
    /// opacos (a transparência sobre o branco puxava para o marrom).
    fn tone_box(&self, t: Tone, light: bool) -> ((Rgb, f32), (Rgb, f32)) {
        let c = self.tone(t);
        if !light {
            return ((c, 0.12), (c, 0.35));
        }
        let (fill, border) = match t {
            Tone::Ok => (0xEAF6EE, 0xB7DEC2),
            Tone::Info | Tone::Busy => (0xEAF1FD, 0xB9CFF5),
            Tone::Warn => (0xFFF5DC, 0xF0CF7A),
            Tone::Error => (0xFDEDEB, 0xF2B8B1),
        };
        ((rgb(fill), 1.0), (rgb(border), 1.0))
    }

    fn tone(&self, t: Tone) -> Rgb {
        match t {
            Tone::Ok => self.ok,
            Tone::Info | Tone::Busy => self.info,
            Tone::Warn => self.warn,
            Tone::Error => self.error,
        }
    }
}

// ---------------------------------------------------------------- layout
const TITLE: f32 = 15.0;
const BODY: f32 = 12.5;
const SMALL: f32 = 11.5;
const LINE: f32 = 17.0;

/// Posições calculadas (px lógicos).
pub struct Layout {
    top: f32,
    card: R,
    grid_label_y: f32,
    status_box: R,
    status_lines: Vec<String>,
    footer_y: f32,
    buttons: Vec<(Btn, R)>,
}

impl Layout {
    pub fn rect(&self, b: Btn) -> Option<R> {
        self.buttons.iter().find(|(x, _)| *x == b).map(|(_, r)| *r)
    }
}

/// Quebra `s` em linhas que cabem em `max_w`.
/// Palavras maiores que a linha (caminhos, nomes de esquema) são quebradas
/// no meio.
fn wrap(w: Weight, size: f32, s: &str, max_w: f32) -> Vec<String> {
    let fits = |t: &str| text::extents(w, size, t).x_advance <= max_w;
    let mut lines = Vec::new();
    let mut cur = String::new();
    // só o espaço comum quebra linha (o U+00A0 segura "recortei --capture" junto)
    for word in s.split(' ').filter(|w| !w.is_empty()) {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if fits(&cand) {
            cur = cand;
            continue;
        }
        if !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        for ch in word.chars() {
            cur.push(ch);
            if !fits(&cur) && cur.chars().count() > 1 {
                cur.pop();
                lines.push(std::mem::replace(&mut cur, ch.to_string()));
            }
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Fica com as `n` primeiras linhas; se sobrou texto, a última termina em "…".
fn clamp_lines(mut lines: Vec<String>, n: usize, max_w: f32) -> Vec<String> {
    if lines.len() <= n {
        return lines;
    }
    lines.truncate(n);
    if let Some(last) = lines.last_mut() {
        while !last.is_empty()
            && text::extents(Weight::Regular, BODY, &format!("{last}…")).x_advance > max_w
        {
            last.pop();
        }
        last.push('…');
    }
    lines
}

pub fn layout(v: &View) -> Layout {
    let top = if v.header { HEADER } else { 0.0 };
    let inner = WIDTH - 2.0 * PAD;
    let mut buttons = Vec::new();
    if v.header {
        buttons.push((Btn::HeaderClose, R::new(WIDTH - 40.0, 10.0, 26.0, 26.0)));
    }
    // cartão com o atalho atual e o botão de gravar (mais estreito gravando,
    // para sobrar espaço para as instruções)
    let card = R::new(PAD, top + 72.0, inner, 64.0);
    let rec_w = if v.recording { 164.0 } else { 212.0 };
    buttons.push((
        Btn::Record,
        R::new(card.right() - 14.0 - rec_w, card.y + 14.0, rec_w, 36.0),
    ));
    // sugestões em 3 colunas
    let grid_label_y = card.bottom() + 28.0;
    let (cols, gap, bh) = (3usize, 10.0, 36.0);
    let bw = (inner - gap * (cols as f32 - 1.0)) / cols as f32;
    let gy = grid_label_y + 10.0;
    for i in 0..v.presets.len() {
        let (c, r) = ((i % cols) as f32, (i / cols) as f32);
        buttons.push((
            Btn::Preset(i),
            R::new(PAD + c * (bw + gap), gy + r * (bh + 8.0), bw, bh),
        ));
    }
    let rows = v.presets.len().div_ceil(cols) as f32;
    let y = gy + rows * (bh + 8.0) - 8.0 + 18.0;
    // caixa de situação (até 3 linhas)
    let fix_w = 176.0;
    let text_w = inner - 44.0 - if v.fix_button { fix_w + 12.0 } else { 14.0 };
    let status_lines = clamp_lines(wrap(Weight::Regular, BODY, &v.status.1, text_w), 3, text_w);
    let box_h = (status_lines.len() as f32 * LINE + 24.0).max(46.0);
    let status_box = R::new(PAD, y, inner, box_h);
    if v.fix_button {
        buttons.push((
            Btn::FixSnipping,
            R::new(
                status_box.right() - 10.0 - fix_w,
                status_box.cy() - 17.0,
                fix_w,
                34.0,
            ),
        ));
    }
    let footer_y = top + CONTENT_H - FOOTER_H;
    buttons.push((
        Btn::Close,
        R::new(WIDTH - PAD - 104.0, footer_y + 13.0, 104.0, 34.0),
    ));
    Layout {
        top,
        card,
        grid_label_y,
        status_box,
        status_lines,
        footer_y,
        buttons,
    }
}

/// Botão sob o ponto (px lógicos).
pub fn hit(v: &View, x: f32, y: f32) -> Option<Btn> {
    layout(v)
        .buttons
        .into_iter()
        .find(|(_, r)| r.contains(x, y))
        .map(|(b, _)| b)
}

// ---------------------------------------------------------------- desenho
/// Desenha a janela inteira em `pm`, com `scale` px físicos por px lógico.
pub fn render(v: &View, pm: &mut Pixmap, scale: f32) {
    let p = palette(v.light);
    let ts = Transform::from_scale(scale, scale);
    let bgc = p.bg;
    pm.fill(Color::from_rgba(bgc[0], bgc[1], bgc[2], 1.0).unwrap_or(Color::BLACK));
    let l = layout(v);
    if v.header {
        draw_header(pm, v, &p, ts);
    }
    let x = PAD;
    draw::text(
        pm,
        Weight::Bold,
        TITLE,
        "Atalho de captura",
        x,
        l.top + 38.0,
        p.text,
        1.0,
        ts,
    );
    draw::text(
        pm,
        Weight::Regular,
        BODY,
        "Aperte este atalho em qualquer lugar para abrir a captura.",
        x,
        l.top + 58.0,
        p.muted,
        1.0,
        ts,
    );
    draw_card(pm, v, &l, &p, ts);
    draw::text(
        pm,
        Weight::Bold,
        SMALL,
        "SUGESTÕES",
        x,
        l.grid_label_y,
        p.muted,
        1.0,
        ts,
    );
    for (i, label) in v.presets.iter().enumerate() {
        let Some(r) = l.rect(Btn::Preset(i)) else {
            continue;
        };
        let active = v.active_preset == Some(i) && !v.recording;
        button(pm, v, &p, Btn::Preset(i), r, label, active, ts);
    }
    draw_status(pm, v, &l, &p, ts);
    draw_footer(pm, v, &l, &p, ts);
}

fn rr(r: R, radius: f32) -> Option<tiny_skia::Path> {
    draw::rounded_rect(r.x, r.y, r.w, r.h, radius)
}

fn outline(pm: &mut Pixmap, r: R, radius: f32, c: Rgb, a: f32, width: f32, ts: Transform) {
    let h = width / 2.0;
    let r = R::new(r.x + h, r.y + h, r.w - width, r.h - width);
    draw::stroke(pm, rr(r, radius - h), c, a, &Line::butt(width), ts);
}

fn focus_ring(pm: &mut Pixmap, r: R, radius: f32, p: &Palette, ts: Transform) {
    let g = R::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0);
    outline(pm, g, radius + 3.0, p.accent, 0.9, 2.0, ts);
}

/// Texto centralizado horizontalmente pela largura de avanço e verticalmente
/// pela altura das maiúsculas (alinha rótulos com e sem descendentes).
#[allow(clippy::too_many_arguments)]
fn label_centered(
    pm: &mut Pixmap,
    w: Weight,
    size: f32,
    s: &str,
    cx: f32,
    cy: f32,
    c: Rgb,
    ts: Transform,
) {
    let adv = text::extents(w, size, s).x_advance;
    let cap = size * 0.73; // altura das maiúsculas da DejaVu Sans
    draw::text(pm, w, size, s, cx - adv / 2.0, cy + cap / 2.0, c, 1.0, ts);
}

#[allow(clippy::too_many_arguments)]
fn button(
    pm: &mut Pixmap,
    v: &View,
    p: &Palette,
    b: Btn,
    r: R,
    label: &str,
    active: bool,
    ts: Transform,
) {
    let (hover, pressed) = (v.hover == Some(b), v.pressed == Some(b));
    let primary = b == Btn::FixSnipping;
    let radius = 8.0;
    if primary {
        draw::fill(pm, rr(r, radius), p.accent_fill, 1.0, ts);
        // escurece (em vez de clarear) para o texto branco manter o contraste
        let shade = if pressed {
            0.22
        } else if hover {
            0.12
        } else {
            0.0
        };
        if shade > 0.0 {
            draw::fill(pm, rr(r, radius), [0.0, 0.0, 0.0], shade, ts);
        }
    } else if active {
        draw::fill(pm, rr(r, radius), p.accent, 0.14, ts);
        outline(pm, r, radius, p.accent, 1.0, 1.5, ts);
    } else {
        let fill = if pressed {
            p.surface_press
        } else if hover {
            p.surface_hover
        } else {
            p.surface
        };
        draw::fill(pm, rr(r, radius), fill, 1.0, ts);
        outline(pm, r, radius, p.border.0, p.border.1, 1.0, ts);
    }
    let (fg, weight) = if primary {
        (p.accent_text, Weight::Bold)
    } else if active {
        (p.accent_fg, Weight::Bold)
    } else {
        (p.text, Weight::Regular)
    };
    let size = 12.5;
    if b == Btn::Record && !v.recording {
        // bolinha vermelha de "gravar" antes do texto
        let adv = text::extents(weight, size, label).x_advance;
        let total = adv + 16.0;
        let x0 = r.cx() - total / 2.0;
        let mut c = Pb::new();
        c.circle(x0 + 4.5, r.cy(), 4.5);
        draw::fill(pm, c.finish(), [0.90, 0.25, 0.22], 1.0, ts);
        label_centered(
            pm,
            weight,
            size,
            label,
            x0 + 16.0 + adv / 2.0,
            r.cy(),
            fg,
            ts,
        );
    } else {
        label_centered(pm, weight, size, label, r.cx(), r.cy(), fg, ts);
    }
    if v.focus == Some(b) {
        focus_ring(pm, r, radius, p, ts);
    }
}

fn draw_header(pm: &mut Pixmap, v: &View, p: &Palette, ts: Transform) {
    draw::fill(
        pm,
        draw::rect_path(0.0, 0.0, WIDTH, HEADER),
        p.header,
        1.0,
        ts,
    );
    draw::fill(
        pm,
        draw::rect_path(0.0, HEADER - 1.0, WIDTH, 1.0),
        p.border.0,
        p.border.1,
        ts,
    );
    label_centered(
        pm,
        Weight::Bold,
        13.0,
        "Configurações do Recortei",
        WIDTH / 2.0,
        HEADER / 2.0,
        p.text,
        ts,
    );
    let r = R::new(WIDTH - 40.0, 10.0, 26.0, 26.0);
    let b = Btn::HeaderClose;
    let a = if v.pressed == Some(b) {
        0.22
    } else if v.hover == Some(b) {
        0.16
    } else {
        0.08
    };
    let mut c = Pb::new();
    c.circle(r.cx(), r.cy(), 12.0);
    draw::fill(pm, c.finish(), p.text, a, ts);
    let mut x = Pb::new();
    let (cx, cy, d) = (r.cx(), r.cy(), 4.0);
    x.move_to(cx - d, cy - d)
        .line_to(cx + d, cy + d)
        .move_to(cx + d, cy - d)
        .line_to(cx - d, cy + d);
    draw::stroke(pm, x.finish(), p.text, 0.9, &Line::round(1.6), ts);
}

/// Teclas desenhadas como teclas de teclado, com `+` entre elas.
fn keycaps(
    pm: &mut Pixmap,
    keys: &[String],
    x: f32,
    cy: f32,
    max_w: f32,
    p: &Palette,
    ts: Transform,
) {
    let (size, pad, gap, h) = (13.0, 11.0, 18.0, 30.0);
    let widths: Vec<f32> = keys
        .iter()
        .map(|k| (text::extents(Weight::Bold, size, k).x_advance + 2.0 * pad).max(34.0))
        .collect();
    let total = widths.iter().sum::<f32>() + gap * (keys.len().saturating_sub(1)) as f32;
    // atalhos muito longos encolhem para caber
    let k = (max_w / total).min(1.0);
    let ts = ts.pre_translate(x, cy).pre_scale(k, k);
    let mut cx = 0.0;
    for (i, (key, w)) in keys.iter().zip(&widths).enumerate() {
        if i > 0 {
            label_centered(
                pm,
                Weight::Regular,
                13.0,
                "+",
                cx - gap / 2.0,
                0.0,
                p.muted,
                ts,
            );
        }
        let r = R::new(cx, -h / 2.0, *w, h);
        draw::fill(
            pm,
            rr(R::new(r.x, r.y + 2.0, r.w, r.h), 6.0),
            p.cap_edge,
            1.0,
            ts,
        );
        draw::fill(pm, rr(r, 6.0), p.cap, 1.0, ts);
        outline(pm, r, 6.0, p.border.0, p.border.1, 1.0, ts);
        label_centered(pm, Weight::Bold, size, key, r.cx(), 0.0, p.text, ts);
        cx += w + gap;
    }
}

fn draw_card(pm: &mut Pixmap, v: &View, l: &Layout, p: &Palette, ts: Transform) {
    let c = l.card;
    draw::fill(pm, rr(c, 12.0), p.surface, 1.0, ts);
    if v.recording {
        draw::fill(pm, rr(c, 12.0), p.accent, 0.08, ts);
        let mut dash = Line::butt(1.5);
        dash.dash = Some(vec![6.0, 4.0]);
        let h = 0.75;
        let r = R::new(c.x + h, c.y + h, c.w - 1.5, c.h - 1.5);
        draw::stroke(pm, rr(r, 11.0), p.accent, 1.0, &dash, ts);
    } else {
        outline(pm, c, 12.0, p.border.0, p.border.1, 1.0, ts);
    }
    let rec = l.rect(Btn::Record).unwrap_or(c);
    let left = c.x + 18.0;
    let avail = rec.x - 16.0 - left;
    if v.recording {
        // instrução + dica (Esc, aviso do GNOME ou por que a tecla foi recusada)
        let (hint, color) = match (&v.record_hint, &v.note) {
            (Some(h), _) => (h.clone(), p.error),
            (None, Some(n)) => (format!("Esc cancela. {n}"), p.muted),
            (None, None) => ("Esc cancela".into(), p.muted),
        };
        let mut hint = wrap(Weight::Regular, SMALL, &hint, avail);
        hint.truncate(2);
        let block = 17.0 + hint.len() as f32 * 14.0;
        let y0 = c.cy() - block / 2.0 + 12.0;
        draw::text(
            pm,
            Weight::Bold,
            13.5,
            "Pressione a combinação…",
            left,
            y0,
            p.accent,
            1.0,
            ts,
        );
        for (i, line) in hint.iter().enumerate() {
            let y = y0 + 17.0 + i as f32 * 14.0;
            draw::text(pm, Weight::Regular, SMALL, line, left, y, color, 1.0, ts);
        }
    } else {
        keycaps(pm, &v.keys, left, c.cy() - 1.0, avail, p, ts);
    }
    let label = if v.recording {
        "Cancelar gravação"
    } else {
        "Gravar outra combinação"
    };
    button(pm, v, p, Btn::Record, rec, label, false, ts);
}

fn draw_status(pm: &mut Pixmap, v: &View, l: &Layout, p: &Palette, ts: Transform) {
    let (tone, _) = v.status;
    let color = p.tone(tone);
    let r = l.status_box;
    let ((fill, fa), (border, ba)) = p.tone_box(tone, v.light);
    draw::fill(pm, rr(r, 10.0), fill, fa, ts);
    outline(pm, r, 10.0, border, ba, 1.0, ts);
    // ícone: círculo com símbolo conforme o tom
    let (ix, iy) = (r.x + 22.0, r.cy());
    let mut c = Pb::new();
    c.circle(ix, iy, 8.0);
    let ln = Line::round(1.8);
    match tone {
        Tone::Ok => {
            draw::fill(pm, c.finish(), color, 1.0, ts);
            let mut m = Pb::new();
            m.move_to(ix - 3.6, iy + 0.2)
                .line_to(ix - 1.0, iy + 2.8)
                .line_to(ix + 3.8, iy - 2.6);
            draw::stroke(pm, m.finish(), p.bg, 1.0, &ln, ts);
        }
        Tone::Busy => {
            draw::stroke(pm, c.finish(), color, 0.35, &Line::butt(2.0), ts);
            let mut a = Pb::new();
            a.arc(ix, iy, 8.0, -1.57, 0.6);
            draw::stroke(pm, a.finish(), color, 1.0, &Line::round(2.0), ts);
        }
        _ => {
            draw::fill(pm, c.finish(), color, 1.0, ts);
            let sym = if tone == Tone::Info { "i" } else { "!" };
            label_centered(pm, Weight::Bold, 11.0, sym, ix, iy, p.bg, ts);
        }
    }
    let n = l.status_lines.len() as f32;
    let y0 = r.cy() - n * LINE / 2.0 + 13.0;
    for (i, line) in l.status_lines.iter().enumerate() {
        draw::text(
            pm,
            Weight::Regular,
            BODY,
            line,
            r.x + 42.0,
            y0 + i as f32 * LINE,
            p.text,
            1.0,
            ts,
        );
    }
    if let Some(b) = l.rect(Btn::FixSnipping) {
        button(
            pm,
            v,
            p,
            Btn::FixSnipping,
            b,
            "Desativar no Windows",
            false,
            ts,
        );
    }
}

fn draw_footer(pm: &mut Pixmap, v: &View, l: &Layout, p: &Palette, ts: Transform) {
    let y = l.footer_y;
    let h = l.top + CONTENT_H - y;
    draw::fill(pm, draw::rect_path(0.0, y, WIDTH, h), p.footer, 1.0, ts);
    draw::fill(
        pm,
        draw::rect_path(0.0, y, WIDTH, 1.0),
        p.border.0,
        p.border.1,
        ts,
    );
    draw::text(
        pm,
        Weight::Regular,
        SMALL,
        "As mudanças valem na hora e ficam salvas.",
        PAD,
        y + h / 2.0 + 4.0,
        p.muted,
        1.0,
        ts,
    );
    if let Some(r) = l.rect(Btn::Close) {
        button(pm, v, p, Btn::Close, r, "Fechar", false, ts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{self, Hotkey, Status};
    use crate::settings::Model;

    fn model(light: bool, header: bool) -> Model {
        let mut m = Model::new(hotkey::DEFAULT, Status::Active);
        m.light = light;
        m.header = header;
        m
    }

    #[test]
    fn hit_testing() {
        for header in [false, true] {
            let v = model(false, header).view();
            let l = layout(&v);
            for (b, r) in &l.buttons {
                assert_eq!(hit(&v, r.cx(), r.cy()), Some(*b), "{b:?}");
                assert!(
                    r.right() <= WIDTH - PAD + 0.01 || *b == Btn::HeaderClose,
                    "{b:?}"
                );
            }
            // fora de qualquer botão
            assert_eq!(hit(&v, 2.0, window_height(header) - 2.0), None);
            assert_eq!(hit(&v, PAD + 2.0, l.card.cy()), None, "teclas do cartão");
            // botões não se sobrepõem
            for (i, (a, ra)) in l.buttons.iter().enumerate() {
                for (b, rb) in &l.buttons[i + 1..] {
                    let overlap = ra.x < rb.right()
                        && rb.x < ra.right()
                        && ra.y < rb.bottom()
                        && rb.y < ra.bottom();
                    assert!(!overlap, "{a:?} × {b:?}");
                }
            }
            assert_eq!(l.rect(Btn::HeaderClose).is_some(), header);
        }
    }

    /// Contraste WCAG entre duas cores opacas.
    fn contrast(a: Rgb, b: Rgb) -> f32 {
        let lum = |c: Rgb| {
            let l = |v: f32| {
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * l(c[0]) + 0.7152 * l(c[1]) + 0.0722 * l(c[2])
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    fn over(c: Rgb, a: f32, bg: Rgb) -> Rgb {
        [0, 1, 2].map(|i| c[i] * a + bg[i] * (1.0 - a))
    }

    /// Texto de 12,5 px em negrito não conta como "grande": precisa de 4,5:1.
    #[test]
    fn button_text_contrast() {
        for light in [false, true] {
            let p = palette(light);
            for shade in [0.0, 0.12, 0.22] {
                let fill = over([0.0; 3], shade, p.accent_fill);
                let c = contrast(p.accent_text, fill);
                assert!(c >= 4.5, "principal (claro={light}, {shade}): {c:.2}");
            }
            let active = over(p.accent, 0.14, p.bg);
            let c = contrast(p.accent_fg, active);
            assert!(c >= 4.5, "sugestão ativa (claro={light}): {c:.2}");
            for t in [Tone::Ok, Tone::Info, Tone::Warn, Tone::Error] {
                let ((fill, a), _) = p.tone_box(t, light);
                let bg = over(fill, a, p.bg);
                let c = contrast(p.text, bg);
                assert!(c >= 4.5, "texto da situação {t:?} (claro={light}): {c:.2}");
                // o ícone é gráfico: 3:1 basta
                let c = contrast(p.tone(t), bg);
                assert!(c >= 3.0, "ícone da situação {t:?} (claro={light}): {c:.2}");
            }
        }
    }

    /// Todos os estados cabem acima do rodapé, inclusive o pior caso.
    #[test]
    fn content_fits_above_footer() {
        for m in states() {
            let v = m.1.view();
            let l = layout(&v);
            assert!(
                l.status_box.bottom() + 12.0 <= l.footer_y,
                "{}: caixa de situação invade o rodapé ({} > {})",
                m.0,
                l.status_box.bottom(),
                l.footer_y
            );
        }
    }

    fn states() -> Vec<(String, Model)> {
        let mut out = Vec::new();
        for light in [false, true] {
            let mut push = |name: &str, m: Model| {
                let theme = if light { "light" } else { "dark" };
                out.push((format!("{name}-{theme}"), m));
            };
            let mut m = model(light, false);
            m.focus = Some(Btn::Record);
            m.focus_visible = true;
            push("active", m);
            let mut g = model(light, true);
            g.gnome = true;
            g.hover = Some(Btn::Preset(2));
            g.focus = None;
            push("gnome", g);
            let mut r = model(light, false);
            r.recording = true;
            push("recording", r.clone());
            let mut rg = r.clone();
            rg.gnome = true;
            rg.header = true;
            push("recording-gnome", rg);
            r.record_hint = Some("Letras e números precisam de Ctrl, Alt ou Super.".into());
            push("recording-hint", r);
            let mut w = model(light, false);
            w.snipping = true;
            w.focus = Some(Btn::FixSnipping);
            w.focus_visible = true;
            push("snipping", w.clone());
            // com a opção ligada, o Windows costuma recusar a tecla ao Recortei
            w.status = Status::Taken;
            push("snipping-taken", w.clone());
            w.snipping_fixed = true;
            w.focus = Some(Btn::Close);
            push("snipping-fixed-taken", w);
            let mut t = model(light, false);
            t.hotkey = Hotkey::parse("Ctrl+Alt+Shift+Super+F12").unwrap();
            t.status = Status::Taken;
            push("taken", t);
            let mut n = model(light, false);
            n.hotkey = Hotkey::parse("Ctrl+Shift+S").unwrap();
            n.status = Status::Manual;
            push("manual", n);
            let mut e = model(light, true);
            e.gnome = true;
            e.status = Status::Error(
                "Não foi possível aplicar o atalho no GNOME: o gsettings recusou a chave \
                 /org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/recortei/binding."
                    .into(),
            );
            push("error", e);
        }
        out
    }

    /// Gera `target/settings-snapshots/*.png` (1× e 2×) para revisão visual.
    #[test]
    fn snapshots() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/settings-snapshots");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, m) in states() {
            let v = m.view();
            for scale in [1.0f32, 2.0] {
                let w = (WIDTH * scale).round() as u32;
                let h = (window_height(v.header) * scale).round() as u32;
                let mut pm = Pixmap::new(w, h).unwrap();
                render(&v, &mut pm, scale);
                assert!(
                    pm.pixels().iter().all(|p| p.alpha() == 255),
                    "{name}: fundo opaco"
                );
                pm.save_png(dir.join(format!("{name}@{}x.png", scale as u32)))
                    .unwrap();
            }
        }
    }
}
