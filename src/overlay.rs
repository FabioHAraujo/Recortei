//! Estado e lógica da sobreposição de captura: seleção, anotações, barra de
//! ferramentas, mouse/teclado e renderização. Independe de plataforma.
//!
//! Todas as coordenadas aqui são em pixels do screenshot ("px da imagem").
//! Cada monitor (`Mon`) mostra uma região da imagem; `ui` é quantos px da imagem
//! equivalem a 1 px lógico de interface naquele monitor (escala HiDPI).

use std::cell::RefCell;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

use tiny_skia::{FilterQuality, IntRect, LineCap, LineJoin, Pixmap, PixmapPaint, Transform};
use winit::event::MouseButton;
use winit::keyboard::{ModifiersState, NamedKey};
use winit::window::CursorIcon;

use crate::config::Config;
use crate::draw::{self, Line, Pb, Rgb};
use crate::text::{self, Weight};

pub const HANDLE: f32 = 8.0; // tamanho da zona de redimensionamento (px lógicos)
const BTN: f32 = 32.0;
const SEP: f32 = 9.0;
const PAD: f32 = 5.0;
const DOUBLE_CLICK: Duration = Duration::from_millis(450);
const CLICK_ITEM_TTL: Duration = Duration::from_millis(1000);

// ---------------------------------------------------------------- ferramentas, cores
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Move,
    Pen,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Text,
    Marker,
    Pixelate,
    Step,
}

pub struct ToolInfo {
    pub tool: Tool,
    pub id: &'static str,
    pub tip: &'static str,
    pub key: char,
}

pub const TOOLS: [ToolInfo; 10] = [
    ToolInfo {
        tool: Tool::Move,
        id: "move",
        tip: "Mover / ajustar área",
        key: 'v',
    },
    ToolInfo {
        tool: Tool::Pen,
        id: "pen",
        tip: "Caneta",
        key: 'p',
    },
    ToolInfo {
        tool: Tool::Line,
        id: "line",
        tip: "Linha",
        key: 'l',
    },
    ToolInfo {
        tool: Tool::Arrow,
        id: "arrow",
        tip: "Seta",
        key: 'a',
    },
    ToolInfo {
        tool: Tool::Rect,
        id: "rect",
        tip: "Retângulo",
        key: 'r',
    },
    ToolInfo {
        tool: Tool::Ellipse,
        id: "ellipse",
        tip: "Elipse",
        key: 'e',
    },
    ToolInfo {
        tool: Tool::Text,
        id: "text",
        tip: "Texto",
        key: 't',
    },
    ToolInfo {
        tool: Tool::Marker,
        id: "marker",
        tip: "Marca-texto",
        key: 'h',
    },
    ToolInfo {
        tool: Tool::Pixelate,
        id: "pixelate",
        tip: "Pixelizar / censurar",
        key: 'b',
    },
    ToolInfo {
        tool: Tool::Step,
        id: "step",
        tip: "Numeração",
        key: 'n',
    },
];

impl Tool {
    pub fn info(self) -> &'static ToolInfo {
        TOOLS
            .iter()
            .find(|t| t.tool == self)
            .expect("toda ferramenta está em TOOLS")
    }
    pub fn id(self) -> &'static str {
        self.info().id
    }
    pub fn from_id(id: &str) -> Option<Tool> {
        TOOLS.iter().find(|t| t.id == id).map(|t| t.tool)
    }
    pub fn from_key(c: char) -> Option<Tool> {
        TOOLS.iter().find(|t| t.key == c).map(|t| t.tool)
    }
}

pub const COLORS: [Rgb; 7] = [
    [0.92, 0.20, 0.20], // vermelho
    [1.00, 0.78, 0.05], // amarelo
    [0.20, 0.78, 0.30], // verde
    [0.20, 0.55, 1.00], // azul
    [0.65, 0.35, 0.95], // roxo
    [0.00, 0.00, 0.00], // preto
    [1.00, 1.00, 1.00], // branco
];
pub const WIDTHS: [i32; 5] = [2, 4, 6, 10, 16];

fn luminance(c: Rgb) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

// ---------------------------------------------------------------- geometria
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        R { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.x <= x && x <= self.right() && self.y <= y && y <= self.bottom()
    }
    fn contains_half_open(&self, x: f32, y: f32) -> bool {
        self.x <= x && x < self.right() && self.y <= y && y < self.bottom()
    }
}

fn norm_rect(p0: (f32, f32), p1: (f32, f32)) -> R {
    R::new(
        p0.0.min(p1.0),
        p0.1.min(p1.1),
        (p1.0 - p0.0).abs(),
        (p1.1 - p0.1).abs(),
    )
}

/// Geometria de um monitor/janela de sobreposição.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mon {
    /// Região do screenshot mostrada neste monitor (px da imagem).
    pub img: R,
    /// px da imagem por px lógico de interface.
    pub ui: f32,
}

// ---------------------------------------------------------------- seleção
const N: u8 = 1;
const S: u8 = 2;
const W: u8 = 4;
const E: u8 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    New,
    Move,
    Edge(u8),
}

#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub rect: Option<R>,
    pub mode: Option<Mode>,
    anchor: (f32, f32),
    orig: Option<R>,
}

impl Selection {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.rect.is_some_and(|r| r.contains(x, y))
    }

    pub fn hit(&self, x: f32, y: f32, handle: f32) -> Option<Mode> {
        let r = self.rect?;
        if !(r.x - handle <= x
            && x <= r.right() + handle
            && r.y - handle <= y
            && y <= r.bottom() + handle)
        {
            return None;
        }
        let mut edge = 0;
        if (y - r.y).abs() <= handle {
            edge |= N;
        } else if (y - r.bottom()).abs() <= handle {
            edge |= S;
        }
        if (x - r.x).abs() <= handle {
            edge |= W;
        } else if (x - r.right()).abs() <= handle {
            edge |= E;
        }
        if edge != 0 {
            return Some(Mode::Edge(edge));
        }
        if r.x < x && x < r.right() && r.y < y && y < r.bottom() {
            return Some(Mode::Move);
        }
        None
    }

    pub fn press(&mut self, x: f32, y: f32, mode: Option<Mode>, handle: f32) {
        self.anchor = (x, y);
        self.orig = self.rect;
        let mode = mode.or_else(|| self.hit(x, y, handle)).unwrap_or(Mode::New);
        self.mode = Some(mode);
        if mode == Mode::New {
            self.rect = Some(R::new(x, y, 0.0, 0.0));
        }
    }

    pub fn motion(&mut self, x: f32, y: f32) -> bool {
        let Some(mode) = self.mode else { return false };
        let (ax, ay) = self.anchor;
        match (mode, self.orig) {
            (Mode::New, _) => self.rect = Some(norm_rect((ax, ay), (x, y))),
            (Mode::Move, Some(o)) => self.rect = Some(R::new(o.x + x - ax, o.y + y - ay, o.w, o.h)),
            (Mode::Edge(e), Some(o)) => {
                let (mut x1, mut y1, mut x2, mut y2) = (o.x, o.y, o.right(), o.bottom());
                if e & W != 0 {
                    x1 = x;
                }
                if e & E != 0 {
                    x2 = x;
                }
                if e & N != 0 {
                    y1 = y;
                }
                if e & S != 0 {
                    y2 = y;
                }
                self.rect = Some(norm_rect((x1, y1), (x2, y2)));
            }
            _ => {}
        }
        true
    }

    pub fn release(&mut self, min: f32) {
        self.mode = None;
        if self.rect.is_some_and(|r| r.w < min || r.h < min) {
            self.rect = self.orig.filter(|o| o.w >= min && o.h >= min);
        }
    }
}

fn edge_cursor(e: u8) -> CursorIcon {
    match e {
        N => CursorIcon::NResize,
        S => CursorIcon::SResize,
        E => CursorIcon::EResize,
        W => CursorIcon::WResize,
        x if x == N | E => CursorIcon::NeResize,
        x if x == N | W => CursorIcon::NwResize,
        x if x == S | E => CursorIcon::SeResize,
        x if x == S | W => CursorIcon::SwResize,
        _ => CursorIcon::Move,
    }
}

// ---------------------------------------------------------------- anotações
struct PixCache {
    key: (i32, i32, i32, i32, i32),
    cols: usize,
    cells: Vec<[u8; 4]>,
}

pub struct Ann {
    pub id: u64,
    pub tool: Tool,
    pub color: usize,
    /// Espessura em px lógicos (1–40).
    pub width: i32,
    /// Escala da interface no momento da criação (px da imagem por px lógico).
    pub ui: f32,
    pub pts: Vec<(f32, f32)>,
    pub text: String,
    pub n: u32,
    cache: RefCell<Option<PixCache>>,
}

impl Ann {
    fn rgb(&self) -> Rgb {
        COLORS[self.color]
    }
}

// ---------------------------------------------------------------- barra de ferramentas
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Tool(Tool),
    Color(usize),
    Width,
    Undo,
    Redo,
    Upload,
    Ok,
    Close,
    Bar,
}

enum Slot {
    It(Item),
    Sep,
}

fn toolbar_slots() -> Vec<Slot> {
    let mut v: Vec<Slot> = TOOLS.iter().map(|t| Slot::It(Item::Tool(t.tool))).collect();
    v.push(Slot::Sep);
    v.extend((0..COLORS.len()).map(|i| Slot::It(Item::Color(i))));
    v.push(Slot::It(Item::Width));
    v.push(Slot::Sep);
    v.extend([
        Slot::It(Item::Undo),
        Slot::It(Item::Redo),
        Slot::Sep,
        Slot::It(Item::Upload),
        Slot::It(Item::Ok),
        Slot::It(Item::Close),
    ]);
    v
}

struct Toolbar {
    rect: R,
    u: f32,
    mon: usize,
    items: Vec<(Item, R)>,
    seps: Vec<f32>,
}

// ---------------------------------------------------------------- entrada
/// Tecla pressionada, já traduzida do evento da plataforma.
#[derive(Clone, Debug, Default)]
pub struct KeyInput {
    pub named: Option<NamedKey>,
    /// Caractere da tecla lógica, em minúsculas (ignora Ctrl).
    pub ch: Option<char>,
    /// Texto que a tecla produz (já composto com teclas mortas).
    pub text: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exit {
    Confirm,
    /// Confirma e envia para a nuvem (vkdl.link).
    Upload,
    Cancel,
}

// ---------------------------------------------------------------- overlay
pub struct Overlay {
    pub base: Pixmap,
    pub mons: Vec<Mon>,
    pub sel: Selection,
    pub tool: Tool,
    pub color: usize,
    pub width: i32,
    pub mods: ModifiersState,
    pub cursor: CursorIcon,
    /// Algo mudou e as janelas precisam ser redesenhadas.
    pub dirty: bool,
    pub exit: Option<Exit>,
    anns: Vec<Ann>,
    redo_stack: Vec<Ann>,
    drawing: Option<Ann>,
    editing: Option<Ann>,
    click_items: Vec<(Instant, u64)>,
    hover: Option<Item>,
    pointer: (f32, f32),
    last_press: Option<(Instant, f32, f32)>,
    next_id: u64,
    scroll_accum: f64,
    scene: Option<(IntRect, Pixmap)>,
    scene_valid: bool,
}

impl Overlay {
    pub fn new(base: Pixmap, mons: Vec<Mon>, cfg: &Config) -> Self {
        let tool = cfg.tool().and_then(Tool::from_id).unwrap_or(Tool::Rect);
        let color = cfg
            .color()
            .filter(|c| (0..COLORS.len() as i64).contains(c))
            .unwrap_or(0) as usize;
        let width = cfg.width().filter(|w| (1..=40).contains(w)).unwrap_or(4) as i32;
        Overlay {
            base,
            mons,
            sel: Selection::default(),
            tool,
            color,
            width,
            mods: ModifiersState::empty(),
            cursor: CursorIcon::Crosshair,
            dirty: true,
            exit: None,
            anns: Vec::new(),
            redo_stack: Vec::new(),
            drawing: None,
            editing: None,
            click_items: Vec::new(),
            hover: None,
            pointer: (0.0, 0.0),
            last_press: None,
            next_id: 1,
            scroll_accum: 0.0,
            scene: None,
            scene_valid: false,
        }
    }

    pub fn set_mons(&mut self, mons: Vec<Mon>) {
        if mons != self.mons {
            self.mons = mons;
            self.redraw();
        }
    }

    fn redraw(&mut self) {
        self.dirty = true;
        self.scene_valid = false;
    }

    /// Redesenho que não muda a cena (dica/hover da barra).
    fn redraw_ui(&mut self) {
        self.dirty = true;
    }

    fn monitor_of(&self, x: f32, y: f32) -> usize {
        if let Some(i) = self
            .mons
            .iter()
            .position(|m| m.img.contains_half_open(x, y))
        {
            return i;
        }
        let d = |m: &Mon| (m.img.cx() - x).abs() + (m.img.cy() - y).abs();
        (0..self.mons.len())
            .min_by(|&a, &b| d(&self.mons[a]).total_cmp(&d(&self.mons[b])))
            .unwrap_or(0)
    }

    fn ui_at(&self, x: f32, y: f32) -> f32 {
        self.mons.get(self.monitor_of(x, y)).map_or(1.0, |m| m.ui)
    }

    /// Escala usada para anotações e para a barra: a do monitor da seleção.
    fn sel_ui(&self) -> f32 {
        match self.sel.rect {
            Some(r) => self.ui_at(r.cx(), r.cy()),
            None => self.ui_at(self.pointer.0, self.pointer.1),
        }
    }

    // -- anotações
    fn new_ann(&mut self, tool: Tool, x: f32, y: f32) -> Ann {
        let id = self.next_id;
        self.next_id += 1;
        let pts = if matches!(tool, Tool::Pen | Tool::Text | Tool::Step) {
            vec![(x, y)]
        } else {
            vec![(x, y), (x, y)]
        };
        Ann {
            id,
            tool,
            color: self.color,
            width: self.width,
            ui: self.sel_ui(),
            pts,
            text: String::new(),
            n: 0,
            cache: RefCell::new(None),
        }
    }

    fn next_step(&self) -> u32 {
        1 + self.anns.iter().filter(|a| a.tool == Tool::Step).count() as u32
    }

    fn push_ann(&mut self, a: Ann) {
        self.anns.push(a);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if self.editing.is_some() {
            self.editing = None;
        } else if let Some(a) = self.anns.pop() {
            self.redo_stack.push(a);
        }
        self.redraw();
    }

    pub fn redo(&mut self) {
        if let Some(a) = self.redo_stack.pop() {
            self.anns.push(a);
        }
        self.redraw();
    }

    fn commit_text(&mut self) {
        if let Some(a) = self.editing.take() {
            if !a.text.trim().is_empty() {
                self.push_ann(a);
            }
        }
        self.redraw();
    }

    fn start_text(&mut self, x: f32, y: f32) {
        self.editing = Some(self.new_ann(Tool::Text, x, y));
    }

    pub fn set_color(&mut self, c: usize) {
        self.color = c.min(COLORS.len() - 1);
        if let Some(e) = &mut self.editing {
            e.color = self.color;
        }
        self.redraw();
    }

    pub fn set_width(&mut self, w: i32) {
        self.width = w.clamp(1, 40);
        if let Some(e) = &mut self.editing {
            e.width = self.width;
        }
        self.redraw();
    }

    // -- resultado
    pub fn confirm(&mut self) {
        self.finish_with(Exit::Confirm);
    }

    /// Como `confirm`, mas o recorte vai para a nuvem e o link para o clipboard.
    pub fn upload(&mut self) {
        self.finish_with(Exit::Upload);
    }

    fn finish_with(&mut self, exit: Exit) {
        if self.editing.is_some() {
            self.commit_text();
        }
        if self.output_rect().is_some() {
            self.exit = Some(exit);
        }
    }

    pub fn close(&mut self) {
        self.exit = Some(Exit::Cancel);
    }

    /// Seleção recortada aos limites do screenshot, em px inteiros.
    fn output_rect(&self) -> Option<IntRect> {
        let r = self.sel.rect?;
        let x0 = r.x.round().max(0.0) as i32;
        let y0 = r.y.round().max(0.0) as i32;
        let x1 = (r.right().round() as i32).min(self.base.width() as i32);
        let y1 = (r.bottom().round() as i32).min(self.base.height() as i32);
        IntRect::from_ltrb(x0, y0, x1, y1).filter(|r| r.width() > 0 && r.height() > 0)
    }

    /// Imagem final (screenshot + anotações) da área selecionada.
    pub fn render_output(&mut self) -> Option<Pixmap> {
        if self.editing.is_some() {
            self.commit_text();
        }
        let r = self.output_rect()?;
        Some(self.paint_scene(r, false))
    }

    // -- desenho da cena
    fn paint_scene(&self, r: IntRect, caret: bool) -> Pixmap {
        let mut pm = Pixmap::new(r.width(), r.height()).expect("área não vazia");
        copy_region(
            &self.base,
            r.x(),
            r.y(),
            &mut pm,
            0,
            0,
            r.width(),
            r.height(),
        );
        for a in &self.anns {
            self.draw_ann(&mut pm, r, a, false);
        }
        if let Some(a) = &self.drawing {
            self.draw_ann(&mut pm, r, a, false);
        }
        if let Some(a) = &self.editing {
            self.draw_ann(&mut pm, r, a, caret);
        }
        pm
    }

    fn draw_ann(&self, pm: &mut Pixmap, org: IntRect, a: &Ann, caret: bool) {
        let ts = Transform::from_translate(-org.x() as f32, -org.y() as f32);
        let u = a.ui;
        let lw = a.width as f32 * u;
        let c = a.rgb();
        let pts = &a.pts;
        let line = Line::round(lw);
        match a.tool {
            Tool::Pen => {
                let mut p = Pb::new();
                p.move_to(pts[0].0, pts[0].1);
                for &(x, y) in &pts[1..] {
                    p.line_to(x, y);
                }
                if pts.len() == 1 {
                    p.line_to(pts[0].0, pts[0].1);
                }
                draw::stroke(pm, p.finish(), c, 1.0, &line, ts);
            }
            Tool::Line | Tool::Arrow => {
                let ((x1, y1), (x2, y2)) = (pts[0], pts[1]);
                let mut p = Pb::new();
                p.move_to(x1, y1);
                if a.tool == Tool::Arrow {
                    let head = (10.0 + a.width as f32 * 3.0) * u;
                    let ang = (y2 - y1).atan2(x2 - x1);
                    let back = (head * 0.75).min((x2 - x1).hypot(y2 - y1));
                    p.line_to(x2 - back * ang.cos(), y2 - back * ang.sin());
                    draw::stroke(pm, p.finish(), c, 1.0, &line, ts);
                    draw::fill(pm, draw::arrow_head(x1, y1, x2, y2, head), c, 1.0, ts);
                } else {
                    p.line_to(x2, y2);
                    draw::stroke(pm, p.finish(), c, 1.0, &line, ts);
                }
            }
            Tool::Rect => {
                let r = norm_rect(pts[0], pts[1]);
                let l = Line {
                    width: lw,
                    cap: LineCap::Round,
                    join: LineJoin::Miter,
                    dash: None,
                };
                draw::stroke(pm, draw::rect_path(r.x, r.y, r.w, r.h), c, 1.0, &l, ts);
            }
            Tool::Ellipse => {
                let r = norm_rect(pts[0], pts[1]);
                if r.w > 0.0 && r.h > 0.0 {
                    let mut p = Pb::new();
                    p.oval(r.x, r.y, r.w, r.h);
                    draw::stroke(pm, p.finish(), c, 1.0, &line, ts);
                }
            }
            Tool::Marker => {
                let r = norm_rect(pts[0], pts[1]);
                draw::fill(pm, draw::rect_path(r.x, r.y, r.w, r.h), c, 0.38, ts);
            }
            Tool::Pixelate => self.draw_pixelate(pm, org, a),
            Tool::Text => {
                let fs = (12.0 + a.width as f32 * 3.0) * u;
                let m = text::metrics(Weight::Bold, fs);
                let (x, y) = pts[0];
                let lines: Vec<&str> = a.text.split('\n').collect();
                for (i, l) in lines.iter().enumerate() {
                    draw::text(
                        pm,
                        Weight::Bold,
                        fs,
                        l,
                        x,
                        y + m.ascent + i as f32 * m.height,
                        c,
                        1.0,
                        ts,
                    );
                }
                if caret {
                    let last = lines.last().copied().unwrap_or("");
                    let cx = x + text::extents(Weight::Bold, fs, last).x_advance + 2.0 * u;
                    let cy = y + (lines.len() - 1) as f32 * m.height;
                    let mut p = Pb::new();
                    p.move_to(cx, cy).line_to(cx, cy + m.height);
                    draw::stroke(pm, p.finish(), c, 1.0, &Line::round(2.0 * u), ts);
                    let widest = lines
                        .iter()
                        .map(|l| text::extents(Weight::Bold, fs, l).x_advance)
                        .fold(0.0, f32::max);
                    let mut dash = Line::butt(1.0 * u);
                    dash.dash = Some(vec![4.0 * u, 3.0 * u]);
                    let r = draw::rect_path(
                        x - 4.0 * u,
                        y - 3.0 * u,
                        widest + 12.0 * u,
                        m.height * lines.len() as f32 + 6.0 * u,
                    );
                    draw::stroke(pm, r, [1.0, 1.0, 1.0], 0.7, &dash, ts);
                }
            }
            Tool::Step => {
                let rad = (10.0 + a.width as f32 * 1.6) * u;
                let (x, y) = pts[0];
                let mut p = Pb::new();
                p.circle(x, y, rad);
                draw::fill(pm, p.finish(), c, 1.0, ts);
                let fg = if luminance(c) > 0.6 {
                    [0.0, 0.0, 0.0]
                } else {
                    [1.0, 1.0, 1.0]
                };
                draw::text_centered(pm, Weight::Bold, rad * 1.1, &a.n.to_string(), x, y, fg, ts);
            }
            Tool::Move => {}
        }
    }

    /// Pixelização: média de blocos do screenshot original (como no ShareX).
    fn draw_pixelate(&self, pm: &mut Pixmap, org: IntRect, a: &Ann) {
        let r = norm_rect(a.pts[0], a.pts[1]);
        let (x0, y0) = (r.x.round() as i32, r.y.round() as i32);
        let (w, h) = (r.w.round() as i32, r.h.round() as i32);
        if w < 1 || h < 1 {
            return;
        }
        let block = 4 + a.width * 2; // tamanho do "pixel" em pixels reais
        let key = (x0, y0, w, h, block);
        let mut cache = a.cache.borrow_mut();
        if cache.as_ref().map(|c| c.key) != Some(key) {
            let cols = ((w + block - 1) / block) as usize;
            let rows = ((h + block - 1) / block) as usize;
            let (bw, bh) = (self.base.width() as i32, self.base.height() as i32);
            let src = self.base.data();
            let mut cells = vec![[0u8, 0, 0, 255]; cols * rows];
            for (ci, cell) in cells.iter_mut().enumerate() {
                let (cx, cy) = ((ci % cols) as i32, (ci / cols) as i32);
                let xa = (x0 + cx * block).max(0);
                let xb = (x0 + ((cx + 1) * block).min(w)).min(bw);
                let ya = (y0 + cy * block).max(0);
                let yb = (y0 + ((cy + 1) * block).min(h)).min(bh);
                let mut sum = [0u64; 3];
                let mut n = 0u64;
                for yy in ya..yb {
                    let row = (yy * bw) as usize * 4;
                    for xx in xa..xb {
                        let i = row + xx as usize * 4;
                        sum[0] += src[i] as u64;
                        sum[1] += src[i + 1] as u64;
                        sum[2] += src[i + 2] as u64;
                        n += 1;
                    }
                }
                if n == 0 {
                    continue; // célula fora da imagem: fica preta
                }
                *cell = [
                    (sum[0] / n) as u8,
                    (sum[1] / n) as u8,
                    (sum[2] / n) as u8,
                    255,
                ];
            }
            *cache = Some(PixCache { key, cols, cells });
        }
        let c = cache.as_ref().expect("preenchido acima");
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let data = pm.data_mut();
        let ya = (y0 - org.y()).max(0);
        let yb = (y0 + h - org.y()).min(ph);
        let xa = (x0 - org.x()).max(0);
        let xb = (x0 + w - org.x()).min(pw);
        for py in ya..yb {
            let cy = ((py + org.y() - y0) / block) as usize;
            let row = (py * pw) as usize * 4;
            for px in xa..xb {
                let cx = ((px + org.x() - x0) / block) as usize;
                let i = row + px as usize * 4;
                data[i..i + 4].copy_from_slice(&c.cells[cy * c.cols + cx]);
            }
        }
    }

    fn ensure_scene(&mut self) {
        if self.scene_valid {
            return;
        }
        self.scene = self.output_rect().map(|r| (r, self.paint_scene(r, true)));
        self.scene_valid = true;
    }

    // -- barra de ferramentas
    fn toolbar(&self) -> Option<Toolbar> {
        let r = self.sel.rect?;
        if self.sel.mode.is_some() {
            return None;
        }
        let mon = self.monitor_of(r.cx(), r.cy());
        let g = self.mons.get(mon)?.img;
        let u = self.mons[mon].ui;
        let slots = toolbar_slots();
        let tw = (PAD * 2.0
            + slots
                .iter()
                .map(|s| if matches!(s, Slot::Sep) { SEP } else { BTN })
                .sum::<f32>())
            * u;
        let th = (BTN + PAD * 2.0) * u;
        let x = (r.cx() - tw / 2.0)
            .max(g.x + 8.0 * u)
            .min(g.right() - tw - 8.0 * u);
        let y = if r.bottom() + 10.0 * u + th <= g.bottom() - 4.0 * u {
            r.bottom() + 10.0 * u
        } else if r.y - 10.0 * u - th >= g.y + 4.0 * u {
            r.y - 10.0 * u - th
        } else {
            r.bottom().min(g.bottom()) - th - 10.0 * u
        };
        let mut items = Vec::new();
        let mut seps = Vec::new();
        let mut cx = x + PAD * u;
        for s in slots {
            match s {
                Slot::Sep => {
                    seps.push(cx + SEP * u / 2.0);
                    cx += SEP * u;
                }
                Slot::It(it) => {
                    items.push((it, R::new(cx, y + PAD * u, BTN * u, BTN * u)));
                    cx += BTN * u;
                }
            }
        }
        Some(Toolbar {
            rect: R::new(x, y, tw, th),
            u,
            mon,
            items,
            seps,
        })
    }

    fn toolbar_hit(&self, x: f32, y: f32) -> Option<Item> {
        let tb = self.toolbar()?;
        if !tb.rect.contains(x, y) {
            return None;
        }
        Some(
            tb.items
                .iter()
                .find(|(_, r)| r.contains_half_open(x, y))
                .map_or(Item::Bar, |(it, _)| *it),
        )
    }

    fn tooltip(&self, it: Item) -> String {
        match it {
            Item::Tool(t) => format!("{} ({})", t.info().tip, t.info().key.to_ascii_uppercase()),
            Item::Color(_) => "Cor (teclas 1–7)".into(),
            Item::Width => format!("Espessura: {}  (roda do mouse ou + / −)", self.width),
            Item::Undo => "Desfazer (Ctrl+Z)".into(),
            Item::Redo => "Refazer (Ctrl+Y)".into(),
            Item::Upload => "Enviar para vkdl.link e copiar o link (Ctrl+U)".into(),
            Item::Ok => "Copiar e salvar (Enter / Ctrl+C / duplo clique)".into(),
            Item::Close => "Cancelar (Esc)".into(),
            Item::Bar => String::new(),
        }
    }

    fn draw_toolbar(&self, pm: &mut Pixmap, ts: Transform) {
        let Some(tb) = self.toolbar() else { return };
        let u = tb.u;
        let R {
            x: tx,
            y: ty,
            w: tw,
            h: th,
        } = tb.rect;
        draw::fill(
            pm,
            draw::rounded_rect(tx, ty, tw, th, 8.0 * u),
            [0.12, 0.12, 0.14],
            0.96,
            ts,
        );
        let color = COLORS[self.color];
        for &(it, b) in &tb.items {
            let (cx, cy) = (b.cx(), b.cy());
            let active = it == Item::Tool(self.tool);
            if active || self.hover == Some(it) {
                let (c, a) = if active {
                    ([0.20, 0.55, 1.0], 0.9)
                } else {
                    ([1.0, 1.0, 1.0], 0.12)
                };
                draw::fill(
                    pm,
                    draw::rounded_rect(b.x + u, b.y + u, b.w - 2.0 * u, b.h - 2.0 * u, 6.0 * u),
                    c,
                    a,
                    ts,
                );
            }
            match it {
                Item::Tool(t) => draw::icon(pm, t.id(), cx, cy, u, color, ts),
                Item::Color(i) => {
                    if i == self.color {
                        let mut p = Pb::new();
                        p.circle(cx, cy, 11.0 * u);
                        draw::fill(pm, p.finish(), [1.0, 1.0, 1.0], 1.0, ts);
                    }
                    let mut p = Pb::new();
                    p.circle(cx, cy, 8.0 * u);
                    let path = p.finish();
                    draw::fill(pm, path.clone(), COLORS[i], 1.0, ts);
                    draw::stroke(pm, path, [0.5, 0.5, 0.5], 0.8, &Line::butt(u), ts);
                }
                Item::Width => {
                    let mut p = Pb::new();
                    p.circle(
                        cx,
                        cy - 3.0 * u,
                        (self.width.min(18) as f32 / 2.0).max(1.5) * u,
                    );
                    draw::fill(pm, p.finish(), color, 1.0, ts);
                    let txt = self.width.to_string();
                    let e = text::extents(Weight::Regular, 9.0 * u, &txt);
                    draw::text(
                        pm,
                        Weight::Regular,
                        9.0 * u,
                        &txt,
                        cx - e.x_advance / 2.0,
                        b.bottom() - 3.0 * u,
                        [0.9, 0.9, 0.92],
                        1.0,
                        ts,
                    );
                }
                Item::Undo => draw::icon(pm, "undo", cx, cy, u, color, ts),
                Item::Redo => draw::icon(pm, "redo", cx, cy, u, color, ts),
                Item::Upload => draw::icon(pm, "upload", cx, cy, u, color, ts),
                Item::Ok => draw::icon(pm, "ok", cx, cy, u, color, ts),
                Item::Close => draw::icon(pm, "close", cx, cy, u, color, ts),
                Item::Bar => {}
            }
        }
        for &sx in &tb.seps {
            let mut p = Pb::new();
            p.move_to(sx, ty + 8.0 * u).line_to(sx, ty + th - 8.0 * u);
            draw::stroke(pm, p.finish(), [1.0, 1.0, 1.0], 0.18, &Line::butt(u), ts);
        }
        // dica
        if let Some(h) = self.hover.filter(|h| *h != Item::Bar) {
            let tip = self.tooltip(h);
            if !tip.is_empty() {
                let fs = 12.0 * u;
                let e = text::extents(Weight::Regular, fs, &tip);
                let g = self.mons[tb.mon].img;
                let lx = tx.max(g.x + 4.0 * u).min(g.right() - e.width - 20.0 * u);
                let mut ly = ty + th + 6.0 * u;
                if ly + 24.0 * u > g.bottom() {
                    ly = ty - 30.0 * u;
                }
                draw::fill(
                    pm,
                    draw::rounded_rect(lx, ly, e.x_advance + 16.0 * u, 24.0 * u, 5.0 * u),
                    [0.0, 0.0, 0.0],
                    0.85,
                    ts,
                );
                draw::text(
                    pm,
                    Weight::Regular,
                    fs,
                    &tip,
                    lx + 8.0 * u,
                    ly + 16.0 * u,
                    [1.0, 1.0, 1.0],
                    1.0,
                    ts,
                );
            }
        }
    }

    // -- desenho de uma janela
    /// Desenha o monitor `idx` em `frame` (tamanho da janela). `dim` é o fundo
    /// escurecido já na resolução da janela (ver [`make_dim_background`]).
    pub fn render_window(&mut self, idx: usize, frame: &mut Pixmap, dim: &Pixmap) {
        let Some(&m) = self.mons.get(idx) else { return };
        if dim.width() == frame.width() && dim.height() == frame.height() {
            frame.data_mut().copy_from_slice(dim.data());
        } else {
            frame.fill(tiny_skia::Color::BLACK);
        }
        let k = frame.width() as f32 / m.img.w.max(1.0);
        let ts = Transform::from_row(k, 0.0, 0.0, k, -m.img.x * k, -m.img.y * k);
        let u = m.ui;
        let Some(r) = self.sel.rect else {
            let msg = "Arraste para selecionar a área · Esc para cancelar";
            let fs = 18.0 * u;
            let e = text::extents(Weight::Bold, fs, msg);
            draw::text(
                frame,
                Weight::Bold,
                fs,
                msg,
                m.img.x + (m.img.w - e.width) / 2.0,
                m.img.y + m.img.h / 2.0,
                [1.0, 1.0, 1.0],
                1.0,
                ts,
            );
            return;
        };
        self.ensure_scene();
        if let Some((ir, scene)) = &self.scene {
            blit(frame, scene, ir.x() as f32, ir.y() as f32, ts);
        }
        let (x, y, w, h) = (r.x, r.y, r.w, r.h);
        let blue = [0.20, 0.60, 1.0];
        draw::stroke(
            frame,
            draw::rect_path(x, y, w, h),
            blue,
            1.0,
            &Line::butt(2.0 * u),
            ts,
        );
        let mut hp = Pb::new();
        for (hx, hy) in [
            (x, y),
            (x + w, y),
            (x, y + h),
            (x + w, y + h),
            (x + w / 2.0, y),
            (x + w / 2.0, y + h),
            (x, y + h / 2.0),
            (x + w, y + h / 2.0),
        ] {
            hp.rect(hx - 4.0 * u, hy - 4.0 * u, 8.0 * u, 8.0 * u);
        }
        draw::fill(frame, hp.finish(), blue, 1.0, ts);

        let label = format!("{} × {}", w.round() as i64, h.round() as i64);
        let fs = 13.0 * u;
        let e = text::extents(Weight::Bold, fs, &label);
        let tb_above = self.toolbar().is_some_and(|t| t.rect.y < y);
        let above_free = y - m.img.y > 30.0 * u && !tb_above;
        let ly = if above_free {
            y - 10.0 * u
        } else {
            y + e.height + 14.0 * u
        };
        let lx = x + if above_free { 2.0 } else { 8.0 } * u;
        let bg = draw::rect_path(
            lx - 6.0 * u,
            ly - e.height - 6.0 * u,
            e.width + 12.0 * u,
            e.height + 12.0 * u,
        );
        draw::fill(frame, bg, [0.0, 0.0, 0.0], 0.75, ts);
        draw::text(
            frame,
            Weight::Bold,
            fs,
            &label,
            lx,
            ly,
            [1.0, 1.0, 1.0],
            1.0,
            ts,
        );
        self.draw_toolbar(frame, ts);
    }

    // -- mouse
    pub fn pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer = (x, y);
        if self.sel.motion(x.round(), y.round()) {
            self.redraw();
        } else if self.drawing.is_some() {
            let shift = self.mods.shift_key();
            let a = self.drawing.as_mut().expect("verificado acima");
            if a.tool == Tool::Pen {
                a.pts.push((x, y));
            } else {
                let (x0, y0) = a.pts[0];
                let (mut x, mut y) = (x, y);
                if shift {
                    let (dx, dy) = (x - x0, y - y0);
                    if matches!(a.tool, Tool::Line | Tool::Arrow) {
                        // trava em múltiplos de 45°
                        let ang = (dy.atan2(dx) / (PI / 4.0)).round() * (PI / 4.0);
                        let ln = dx.hypot(dy);
                        (x, y) = (x0 + ln * ang.cos(), y0 + ln * ang.sin());
                    } else {
                        // quadrado / círculo
                        let d = dx.abs().max(dy.abs());
                        (x, y) = (x0 + d.copysign(dx), y0 + d.copysign(dy));
                    }
                }
                a.pts[1] = (x, y);
            }
            self.redraw();
        } else {
            self.update_cursor();
        }
    }

    pub fn press(&mut self, button: MouseButton) {
        let (x, y) = self.pointer;
        let now = Instant::now();
        if let Some(it) = self.toolbar_hit(x, y) {
            if button == MouseButton::Left {
                self.toolbar_click(it);
            }
            self.last_press = None;
            return;
        }
        let ui = self.sel_ui();

        if button == MouseButton::Left {
            let near = |px: f32, py: f32| (px - x).abs() <= 4.0 * ui && (py - y).abs() <= 4.0 * ui;
            let first = self
                .last_press
                .filter(|&(t, px, py)| now - t < DOUBLE_CLICK && near(px, py));
            self.last_press = if first.is_some() {
                None
            } else {
                Some((now, x, y))
            };
            if let Some((first_t, _, _)) = first.filter(|_| self.sel.contains(x, y)) {
                // desfaz o que o primeiro clique criou (passo, texto vazio...)
                self.drawing = None;
                let recent: Vec<u64> = self
                    .click_items
                    .iter()
                    .filter(|(t, _)| *t >= first_t)
                    .map(|(_, id)| *id)
                    .collect();
                self.anns.retain(|a| !recent.contains(&a.id));
                if self
                    .editing
                    .as_ref()
                    .is_some_and(|e| e.text.trim().is_empty())
                {
                    self.editing = None;
                }
                self.sel.mode = None;
                self.confirm();
                self.redraw();
                return;
            }
        }

        match button {
            MouseButton::Right => {
                if self.editing.is_some() {
                    self.commit_text();
                } else if self.drawing.is_some() {
                    self.drawing = None;
                } else if self.sel.rect.is_some() {
                    // recomeça a seleção do zero
                    self.sel = Selection::default();
                    self.anns.clear();
                    self.redo_stack.clear();
                } else {
                    self.close();
                    return;
                }
                self.redraw();
                return;
            }
            MouseButton::Left => {}
            _ => return,
        }

        if self.editing.is_some() {
            self.commit_text();
        }
        self.click_items.retain(|(t, _)| now - *t < CLICK_ITEM_TTL);
        let handle = HANDLE * ui;
        let (rx, ry) = (x.round(), y.round());

        if self.sel.rect.is_none() {
            self.sel.press(rx, ry, Some(Mode::New), handle);
        } else {
            let hit = self.sel.hit(x, y, handle);
            if matches!(hit, Some(Mode::Edge(_))) || self.tool == Tool::Move {
                // redimensionar pelas bordas funciona com qualquer ferramenta
                self.sel.press(rx, ry, None, handle);
            } else if self.tool == Tool::Text {
                self.start_text(x, y);
            } else if self.tool == Tool::Step {
                let mut a = self.new_ann(Tool::Step, x, y);
                a.n = self.next_step();
                self.click_items.push((now, a.id));
                self.push_ann(a);
            } else {
                self.drawing = Some(self.new_ann(self.tool, x, y));
            }
        }
        self.redraw();
    }

    pub fn release(&mut self, _button: MouseButton) {
        let min = 3.0 * self.sel_ui();
        if self.sel.mode.is_some() {
            self.sel.release(min);
        } else if let Some(a) = self.drawing.take() {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for &(x, y) in &a.pts {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
            if x1 - x0 >= min || y1 - y0 >= min {
                self.push_ann(a);
            }
        }
        self.update_cursor();
        self.redraw();
    }

    /// Roda do mouse; `lines` > 0 é para cima.
    pub fn wheel(&mut self, lines: f64) {
        self.scroll_accum += lines;
        while self.scroll_accum >= 1.0 {
            self.scroll_accum -= 1.0;
            self.set_width(self.width + 1);
        }
        while self.scroll_accum <= -1.0 {
            self.scroll_accum += 1.0;
            self.set_width(self.width - 1);
        }
    }

    fn update_cursor(&mut self) {
        let (x, y) = self.pointer;
        let tb = self.toolbar_hit(x, y);
        if tb != self.hover {
            self.hover = tb;
            self.redraw_ui();
        }
        let handle = HANDLE * self.sel_ui();
        self.cursor = if tb.is_some() {
            CursorIcon::Default
        } else if self.sel.rect.is_none() {
            CursorIcon::Crosshair
        } else {
            match self.sel.hit(x, y, handle) {
                Some(Mode::Edge(e)) => edge_cursor(e),
                h => match self.tool {
                    Tool::Move if h == Some(Mode::Move) => CursorIcon::Move,
                    Tool::Text => CursorIcon::Text,
                    _ => CursorIcon::Crosshair,
                },
            }
        };
    }

    fn toolbar_click(&mut self, it: Item) {
        match it {
            Item::Tool(t) => {
                if self.editing.is_some() {
                    self.commit_text();
                }
                self.tool = t;
            }
            Item::Color(c) => self.set_color(c),
            Item::Width => {
                let next = WIDTHS
                    .iter()
                    .copied()
                    .find(|&w| w > self.width)
                    .unwrap_or(WIDTHS[0]);
                self.set_width(next);
            }
            Item::Undo => self.undo(),
            Item::Redo => self.redo(),
            Item::Upload => return self.upload(),
            Item::Ok => return self.confirm(),
            Item::Close => return self.close(),
            Item::Bar => {}
        }
        self.update_cursor();
        self.redraw();
    }

    // -- teclado
    pub fn key(&mut self, k: &KeyInput) {
        let ctrl = self.mods.control_key();
        let shift = self.mods.shift_key();
        let named = k.named;
        let enter = named == Some(NamedKey::Enter);

        if let Some(ed) = &mut self.editing {
            match named {
                Some(NamedKey::Escape) => self.editing = None,
                _ if enter && shift => ed.text.push('\n'),
                _ if enter => self.commit_text(),
                _ if ctrl && k.ch == Some('c') => {
                    self.commit_text();
                    self.confirm();
                    return;
                }
                _ if ctrl && k.ch == Some('u') => {
                    self.commit_text();
                    self.upload();
                    return;
                }
                _ if ctrl && k.ch == Some('z') => self.editing = None,
                Some(NamedKey::Backspace) => {
                    ed.text.pop();
                }
                _ if !ctrl => {
                    if let Some(t) = &k.text {
                        ed.text.extend(t.chars().filter(|c| !c.is_control()));
                    }
                }
                _ => {}
            }
            self.redraw();
            return;
        }

        let arrow = matches!(
            named,
            Some(
                NamedKey::ArrowLeft
                    | NamedKey::ArrowRight
                    | NamedKey::ArrowUp
                    | NamedKey::ArrowDown
            )
        );
        if named == Some(NamedKey::Escape) {
            self.close();
        } else if enter || (ctrl && k.ch == Some('c')) {
            self.confirm();
        } else if ctrl && k.ch == Some('u') {
            self.upload();
        } else if ctrl && k.ch == Some('z') {
            if shift {
                self.redo()
            } else {
                self.undo()
            }
        } else if ctrl && k.ch == Some('y') {
            self.redo();
        } else if arrow && self.sel.rect.is_some() {
            let r = self.sel.rect.expect("verificado acima");
            let step = (if shift { 10.0 } else { 1.0 }) * self.sel_ui().round().max(1.0);
            let (dx, dy) = match named {
                Some(NamedKey::ArrowLeft) => (-step, 0.0),
                Some(NamedKey::ArrowRight) => (step, 0.0),
                Some(NamedKey::ArrowUp) => (0.0, -step),
                _ => (0.0, step),
            };
            self.sel.rect = Some(if ctrl {
                // Ctrl+setas redimensiona
                let min = 3.0 * self.sel_ui();
                R::new(r.x, r.y, (r.w + dx).max(min), (r.h + dy).max(min))
            } else {
                R::new(r.x + dx, r.y + dy, r.w, r.h)
            });
            self.redraw();
        } else if ctrl {
        } else if matches!(k.ch, Some('+' | '=')) {
            self.set_width(self.width + 1);
        } else if k.ch == Some('-') {
            self.set_width(self.width - 1);
        } else if let Some(d) =
            k.ch.and_then(|c| c.to_digit(10))
                .filter(|d| (1..=COLORS.len() as u32).contains(d))
        {
            self.set_color(d as usize - 1);
        } else if let Some(t) = k.ch.and_then(Tool::from_key) {
            self.tool = t;
            self.update_cursor();
            self.redraw();
        }
    }

    /// Pedido de captura enquanto a sobreposição já está aberta (atalho
    /// global): confirma como Enter, inclusive no meio de um texto.
    pub fn hotkey_pressed(&mut self) {
        self.confirm();
    }
}

// ---------------------------------------------------------------- pixels
/// Copia um retângulo `w×h` de `src` (em sx, sy) para `dst` (em dx, dy), recortando bordas.
#[allow(clippy::too_many_arguments)]
pub fn copy_region(
    src: &Pixmap,
    sx: i32,
    sy: i32,
    dst: &mut Pixmap,
    dx: i32,
    dy: i32,
    w: u32,
    h: u32,
) {
    let (sw, shh) = (src.width() as i32, src.height() as i32);
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    // intervalo válido de deslocamentos (i, j) dentro do retângulo
    let i0 = 0.max(-sx).max(-dx);
    let j0 = 0.max(-sy).max(-dy);
    let i1 = (w as i32).min(sw - sx).min(dw - dx);
    let j1 = (h as i32).min(shh - sy).min(dh - dy);
    if i1 <= i0 || j1 <= j0 {
        return;
    }
    let n = (i1 - i0) as usize * 4;
    let s = src.data();
    let d = dst.data_mut();
    for j in j0..j1 {
        let so = (((sy + j) * sw + sx + i0) * 4) as usize;
        let doff = (((dy + j) * dw + dx + i0) * 4) as usize;
        d[doff..doff + n].copy_from_slice(&s[so..so + n]);
    }
}

/// Desenha `src` (posicionado em x, y no espaço da imagem) em `dst` usando `ts`.
fn blit(dst: &mut Pixmap, src: &Pixmap, x: f32, y: f32, ts: Transform) {
    let unit =
        (ts.sx - 1.0).abs() < 1e-4 && (ts.sy - 1.0).abs() < 1e-4 && ts.kx == 0.0 && ts.ky == 0.0;
    if unit {
        let (dx, dy) = ((x + ts.tx).round() as i32, (y + ts.ty).round() as i32);
        copy_region(src, 0, 0, dst, dx, dy, src.width(), src.height());
    } else {
        let paint = PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..Default::default()
        };
        dst.draw_pixmap(0, 0, src.as_ref(), &paint, ts.pre_translate(x, y), None);
    }
}

/// Fundo escurecido (screenshot × 0,55) de um monitor, na resolução da janela.
pub fn make_dim_background(base: &Pixmap, img: R, w: u32, h: u32) -> Pixmap {
    let mut pm = Pixmap::new(w.max(1), h.max(1)).expect("tamanho válido");
    let k = w as f32 / img.w.max(1.0);
    let ts = Transform::from_row(k, 0.0, 0.0, k, -img.x * k, -img.y * k);
    blit(&mut pm, base, 0.0, 0.0, ts);
    for px in pm.data_mut().as_chunks_mut::<4>().0 {
        px[0] = ((px[0] as u32 * 141) >> 8) as u8;
        px[1] = ((px[1] as u32 * 141) >> 8) as u8;
        px[2] = ((px[2] as u32 * 141) >> 8) as u8;
        px[3] = 255;
    }
    pm
}

/// Converte RGBA (opaco) para o formato 0RGB que o softbuffer espera.
pub fn to_xrgb(src: &Pixmap, dst: &mut [u32]) {
    for (d, s) in dst.iter_mut().zip(src.data().as_chunks::<4>().0) {
        *d = (s[0] as u32) << 16 | (s[1] as u32) << 8 | s[2] as u32;
    }
}

// ---------------------------------------------------------------- testes
#[cfg(test)]
mod tests {
    use super::*;

    fn overlay(w: u32, h: u32) -> Overlay {
        let mut base = Pixmap::new(w, h).unwrap();
        // gradiente simples para que pixelização e recortes tenham conteúdo
        for (i, px) in base
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let (x, y) = (i as u32 % w, i as u32 / w);
            px.copy_from_slice(&[(x % 256) as u8, (y % 256) as u8, 128, 255]);
        }
        let mons = vec![Mon {
            img: R::new(0.0, 0.0, w as f32, h as f32),
            ui: 1.0,
        }];
        Overlay::new(base, mons, &Config::default())
    }

    fn drag(o: &mut Overlay, from: (f32, f32), to: (f32, f32)) {
        o.pointer_moved(from.0, from.1);
        o.press(MouseButton::Left);
        o.pointer_moved((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        o.pointer_moved(to.0, to.1);
        o.release(MouseButton::Left);
        o.last_press = None; // não deixa o próximo clique virar duplo clique
    }

    fn key_char(o: &mut Overlay, c: char) {
        o.key(&KeyInput {
            named: None,
            ch: Some(c),
            text: Some(c.to_string()),
        });
    }

    fn key_named(o: &mut Overlay, n: NamedKey) {
        o.key(&KeyInput {
            named: Some(n),
            ch: None,
            text: None,
        });
    }

    #[test]
    fn selection_drag_creates_rect() {
        let mut o = overlay(800, 600);
        drag(&mut o, (100.0, 100.0), (300.0, 250.0));
        assert_eq!(o.sel.rect, Some(R::new(100.0, 100.0, 200.0, 150.0)));
        assert!(o.sel.mode.is_none());
    }

    #[test]
    fn tiny_selection_is_discarded() {
        let mut o = overlay(800, 600);
        drag(&mut o, (100.0, 100.0), (101.0, 101.0));
        assert_eq!(o.sel.rect, None);
    }

    #[test]
    fn resize_from_edges_with_any_tool() {
        let mut o = overlay(800, 600);
        drag(&mut o, (100.0, 100.0), (300.0, 300.0));
        o.tool = Tool::Pen;
        drag(&mut o, (300.0, 300.0), (400.0, 350.0)); // canto SE
        assert_eq!(o.sel.rect, Some(R::new(100.0, 100.0, 300.0, 250.0)));
        drag(&mut o, (100.0, 200.0), (50.0, 200.0)); // borda W
        assert_eq!(o.sel.rect, Some(R::new(50.0, 100.0, 350.0, 250.0)));
        assert!(o.anns.is_empty(), "redimensionar não deve desenhar");
    }

    #[test]
    fn move_tool_moves_selection() {
        let mut o = overlay(800, 600);
        drag(&mut o, (100.0, 100.0), (300.0, 300.0));
        key_char(&mut o, 'v');
        assert_eq!(o.tool, Tool::Move);
        drag(&mut o, (200.0, 200.0), (250.0, 220.0));
        assert_eq!(o.sel.rect, Some(R::new(150.0, 120.0, 200.0, 200.0)));
    }

    #[test]
    fn hit_test_edges_and_inside() {
        let s = Selection {
            rect: Some(R::new(100.0, 100.0, 100.0, 100.0)),
            ..Default::default()
        };
        assert_eq!(s.hit(100.0, 100.0, 8.0), Some(Mode::Edge(N | W)));
        assert_eq!(s.hit(205.0, 150.0, 8.0), Some(Mode::Edge(E)));
        assert_eq!(s.hit(150.0, 198.0, 8.0), Some(Mode::Edge(S)));
        assert_eq!(s.hit(150.0, 150.0, 8.0), Some(Mode::Move));
        assert_eq!(s.hit(50.0, 50.0, 8.0), None);
    }

    #[test]
    fn drawing_undo_redo() {
        let mut o = overlay(800, 600);
        drag(&mut o, (0.0, 0.0), (500.0, 500.0));
        for t in ['p', 'l', 'a', 'r', 'e', 'h', 'b'] {
            key_char(&mut o, t);
            drag(&mut o, (100.0, 100.0), (200.0, 180.0));
        }
        assert_eq!(o.anns.len(), 7);
        o.mods = ModifiersState::CONTROL;
        key_char(&mut o, 'z');
        key_char(&mut o, 'z');
        assert_eq!(o.anns.len(), 5);
        key_char(&mut o, 'y');
        assert_eq!(o.anns.len(), 6);
        o.mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        key_char(&mut o, 'z');
        assert_eq!(o.anns.len(), 7);
        o.mods = ModifiersState::empty();
        // um clique sem arrastar não cria anotação
        key_char(&mut o, 'r');
        drag(&mut o, (100.0, 100.0), (101.0, 101.0));
        assert_eq!(o.anns.len(), 7);
        // nova anotação limpa o refazer
        o.undo();
        drag(&mut o, (100.0, 100.0), (150.0, 150.0));
        assert!(o.redo_stack.is_empty());
    }

    #[test]
    fn shift_constrains_line_and_square() {
        let mut o = overlay(800, 600);
        drag(&mut o, (0.0, 0.0), (700.0, 500.0));
        o.mods = ModifiersState::SHIFT;
        key_char(&mut o, 'l');
        drag(&mut o, (100.0, 100.0), (200.0, 110.0));
        let (x, y) = o.anns[0].pts[1];
        assert!((y - 100.0).abs() < 0.01 && (x - 100.0 - 10100f32.sqrt()).abs() < 0.1);
        key_char(&mut o, 'r');
        drag(&mut o, (100.0, 100.0), (200.0, 130.0));
        assert_eq!(o.anns[1].pts[1], (200.0, 200.0));
    }

    #[test]
    fn steps_number_and_double_click_confirms() {
        let mut o = overlay(800, 600);
        drag(&mut o, (0.0, 0.0), (500.0, 500.0));
        key_char(&mut o, 'n');
        for p in [(50.0, 50.0), (100.0, 100.0)] {
            o.pointer_moved(p.0, p.1);
            o.press(MouseButton::Left);
            o.release(MouseButton::Left);
            o.last_press = None;
        }
        assert_eq!(o.anns.iter().map(|a| a.n).collect::<Vec<_>>(), vec![1, 2]);
        // duplo clique: o passo criado pelo 1º clique é removido e confirma
        o.pointer_moved(300.0, 300.0);
        o.press(MouseButton::Left);
        o.release(MouseButton::Left);
        assert_eq!(o.anns.len(), 3);
        o.press(MouseButton::Left);
        assert_eq!(o.anns.len(), 2);
        assert_eq!(o.exit, Some(Exit::Confirm));
    }

    #[test]
    fn text_editing_keys() {
        let mut o = overlay(800, 600);
        drag(&mut o, (0.0, 0.0), (500.0, 500.0));
        key_char(&mut o, 't');
        o.pointer_moved(100.0, 100.0);
        o.press(MouseButton::Left);
        o.release(MouseButton::Left);
        for c in "Olá ".chars() {
            key_char(&mut o, c);
        }
        key_char(&mut o, '1'); // dígito vira texto, não troca a cor
        assert_eq!(o.color, 0);
        o.mods = ModifiersState::SHIFT;
        key_named(&mut o, NamedKey::Enter);
        o.mods = ModifiersState::empty();
        key_char(&mut o, 'x');
        key_named(&mut o, NamedKey::Backspace);
        key_char(&mut o, 'y');
        assert_eq!(o.editing.as_ref().unwrap().text, "Olá 1\ny");
        key_named(&mut o, NamedKey::Enter);
        assert!(o.editing.is_none());
        assert_eq!(o.anns.len(), 1);
        // texto vazio não é salvo
        o.last_press = None;
        o.pointer_moved(200.0, 200.0);
        o.press(MouseButton::Left);
        key_named(&mut o, NamedKey::Enter);
        assert_eq!(o.anns.len(), 1);
        // Esc durante a edição só cancela o texto
        o.last_press = None;
        o.press(MouseButton::Left);
        key_char(&mut o, 'z');
        key_named(&mut o, NamedKey::Escape);
        assert!(o.exit.is_none() && o.editing.is_none());
    }

    #[test]
    fn keys_change_color_width_and_tool() {
        let mut o = overlay(800, 600);
        key_char(&mut o, '3');
        assert_eq!(o.color, 2);
        key_char(&mut o, '9'); // fora da paleta
        assert_eq!(o.color, 2);
        let w = o.width;
        key_char(&mut o, '+');
        key_char(&mut o, '=');
        assert_eq!(o.width, w + 2);
        key_char(&mut o, '-');
        assert_eq!(o.width, w + 1);
        o.wheel(-1.0);
        o.wheel(-0.5);
        assert_eq!(o.width, w);
        o.wheel(-0.5);
        assert_eq!(o.width, w - 1);
        o.set_width(100);
        assert_eq!(o.width, 40);
        key_char(&mut o, 'e');
        assert_eq!(o.tool, Tool::Ellipse);
    }

    #[test]
    fn arrows_move_and_resize() {
        let mut o = overlay(800, 600);
        drag(&mut o, (100.0, 100.0), (200.0, 200.0));
        key_named(&mut o, NamedKey::ArrowRight);
        o.mods = ModifiersState::SHIFT;
        key_named(&mut o, NamedKey::ArrowDown);
        assert_eq!(o.sel.rect, Some(R::new(101.0, 110.0, 100.0, 100.0)));
        o.mods = ModifiersState::CONTROL;
        key_named(&mut o, NamedKey::ArrowLeft);
        assert_eq!(o.sel.rect, Some(R::new(101.0, 110.0, 99.0, 100.0)));
    }

    #[test]
    fn right_click_resets_then_closes() {
        let mut o = overlay(800, 600);
        drag(&mut o, (100.0, 100.0), (200.0, 200.0));
        drag(&mut o, (120.0, 120.0), (180.0, 180.0));
        assert_eq!(o.anns.len(), 1);
        o.press(MouseButton::Right);
        assert!(o.sel.rect.is_none() && o.anns.is_empty() && o.exit.is_none());
        o.press(MouseButton::Right);
        assert_eq!(o.exit, Some(Exit::Cancel));
    }

    #[test]
    fn escape_closes_and_enter_needs_selection() {
        let mut o = overlay(800, 600);
        key_named(&mut o, NamedKey::Enter);
        assert!(o.exit.is_none());
        key_named(&mut o, NamedKey::Escape);
        assert_eq!(o.exit, Some(Exit::Cancel));
    }

    #[test]
    fn toolbar_buttons_work() {
        let mut o = overlay(1000, 800);
        drag(&mut o, (300.0, 200.0), (700.0, 500.0));
        let tb = o.toolbar().expect("barra visível");
        assert!(tb.rect.y >= 500.0, "barra abaixo da seleção");
        let find = |it: Item| tb.items.iter().find(|(i, _)| *i == it).unwrap().1;
        let click = |o: &mut Overlay, r: R| {
            o.pointer_moved(r.cx(), r.cy());
            o.press(MouseButton::Left);
            o.release(MouseButton::Left);
        };
        click(&mut o, find(Item::Tool(Tool::Arrow)));
        assert_eq!(o.tool, Tool::Arrow);
        click(&mut o, find(Item::Color(4)));
        assert_eq!(o.color, 4);
        o.set_width(4);
        click(&mut o, find(Item::Width));
        assert_eq!(o.width, 6);
        o.set_width(16);
        click(&mut o, find(Item::Width));
        assert_eq!(o.width, 2);
        assert!(o.anns.is_empty(), "cliques na barra não desenham");
        assert_eq!(o.cursor, CursorIcon::Default);
        assert!(!o.tooltip(Item::Width).is_empty());
        assert!(!o.tooltip(Item::Upload).is_empty());
        click(&mut o, find(Item::Ok));
        assert_eq!(o.exit, Some(Exit::Confirm));
    }

    #[test]
    fn upload_button_and_shortcut() {
        let mut o = overlay(1000, 800);
        // sem seleção, Ctrl+U não faz nada
        o.mods = ModifiersState::CONTROL;
        key_char(&mut o, 'u');
        assert!(o.exit.is_none());
        o.mods = ModifiersState::empty();
        drag(&mut o, (300.0, 200.0), (700.0, 500.0));
        let tb = o.toolbar().expect("barra visível");
        // o botão da nuvem fica logo antes de confirmar e cancelar
        let pos = |it: Item| tb.items.iter().position(|(i, _)| *i == it).unwrap();
        assert_eq!(pos(Item::Upload) + 1, pos(Item::Ok));
        assert_eq!(pos(Item::Ok) + 1, pos(Item::Close));
        let r = tb.items[pos(Item::Upload)].1;
        o.pointer_moved(r.cx(), r.cy());
        o.press(MouseButton::Left);
        o.release(MouseButton::Left);
        assert_eq!(o.exit, Some(Exit::Upload));

        let mut o = overlay(1000, 800);
        drag(&mut o, (300.0, 200.0), (700.0, 500.0));
        o.mods = ModifiersState::CONTROL;
        key_char(&mut o, 'u');
        assert_eq!(o.exit, Some(Exit::Upload));
    }

    #[test]
    fn toolbar_goes_above_when_no_room_below() {
        let mut o = overlay(1000, 800);
        drag(&mut o, (300.0, 400.0), (700.0, 790.0));
        let tb = o.toolbar().unwrap();
        assert!(tb.rect.bottom() <= 400.0);
        assert!(tb.rect.x >= 8.0 && tb.rect.right() <= 992.0);
    }

    #[test]
    fn output_contains_annotations() {
        let mut o = overlay(400, 300);
        drag(&mut o, (50.0, 50.0), (250.0, 200.0));
        key_char(&mut o, 'h'); // marca-texto vermelho
        drag(&mut o, (60.0, 60.0), (240.0, 190.0));
        let out = o.render_output().unwrap();
        assert_eq!((out.width(), out.height()), (200, 150));
        let before = o.base.pixel(150, 120).unwrap();
        let after = out.pixel(100, 70).unwrap();
        assert!(
            after.red() > before.red(),
            "marca-texto vermelho deve avermelhar"
        );
        assert_eq!(after.alpha(), 255);
    }

    #[test]
    fn output_is_clipped_to_image() {
        let mut o = overlay(400, 300);
        o.sel.rect = Some(R::new(-50.0, 250.0, 200.0, 200.0));
        let out = o.render_output().unwrap();
        assert_eq!((out.width(), out.height()), (150, 50));
        o.sel.rect = Some(R::new(500.0, 500.0, 10.0, 10.0));
        assert!(o.render_output().is_none());
    }

    #[test]
    fn pixelate_averages_blocks() {
        let mut o = overlay(400, 300);
        drag(&mut o, (0.0, 0.0), (300.0, 200.0));
        o.set_width(2); // bloco de 8 px
        key_char(&mut o, 'b');
        drag(&mut o, (16.0, 16.0), (48.0, 48.0));
        let out = o.render_output().unwrap();
        let a = out.pixel(16, 16).unwrap();
        let b = out.pixel(23, 23).unwrap();
        assert_eq!(
            (a.red(), a.green()),
            (b.red(), b.green()),
            "mesmo bloco, mesma cor"
        );
        assert_eq!(a.red(), 19); // média de 16..24 = 19,5 → 19
        let c = out.pixel(24, 16).unwrap();
        assert_ne!(a.red(), c.red());
    }

    #[test]
    fn render_window_and_hidpi_mapping() {
        let mut o = overlay(400, 300);
        o.set_mons(vec![Mon {
            img: R::new(0.0, 0.0, 400.0, 300.0),
            ui: 2.0,
        }]);
        let dim = make_dim_background(&o.base, o.mons[0].img, 200, 150);
        assert_eq!(dim.width(), 200);
        let mut frame = Pixmap::new(200, 150).unwrap();
        o.render_window(0, &mut frame, &dim);
        drag(&mut o, (100.0, 100.0), (300.0, 200.0));
        o.render_window(0, &mut frame, &dim);
        let mut buf = vec![0u32; 200 * 150];
        to_xrgb(&frame, &mut buf);
        assert!(buf.iter().any(|&p| p != 0));
    }

    #[test]
    fn copy_region_clips() {
        let mut src = Pixmap::new(10, 10).unwrap();
        src.fill(tiny_skia::Color::WHITE);
        let mut dst = Pixmap::new(10, 10).unwrap();
        copy_region(&src, -5, -5, &mut dst, 8, 8, 10, 10);
        assert_eq!(dst.pixel(9, 9).unwrap().alpha(), 0, "fora da origem");
        copy_region(&src, 0, 0, &mut dst, 8, 8, 10, 10);
        assert_eq!(dst.pixel(9, 9).unwrap().alpha(), 255);
        assert_eq!(dst.pixel(7, 7).unwrap().alpha(), 0);
    }

    #[test]
    fn hotkey_confirms_only_with_selection() {
        let mut o = overlay(400, 300);
        o.hotkey_pressed();
        assert!(o.exit.is_none());
        drag(&mut o, (10.0, 10.0), (100.0, 100.0));
        // Insert era o atalho antigo; agora é uma tecla qualquer na sobreposição
        o.key(&KeyInput {
            named: Some(NamedKey::Insert),
            ..Default::default()
        });
        assert!(o.exit.is_none(), "Insert não confirma");
        o.hotkey_pressed();
        assert_eq!(o.exit, Some(Exit::Confirm));
    }
}
