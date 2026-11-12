#!/usr/bin/env python3
"""Recortei — captura de área de tela na bandeja do sistema (GNOME/Wayland).

- Fica na bandeja (AppIndicator).
- Atalho Insert (configurado como atalho do GNOME) chama a ação "capture".
- Arraste para selecionar a área; depois anote com as ferramentas da barra
  (caneta, linha, seta, retângulo, elipse, texto, marca-texto, pixelizar,
  numeração).
- Enter, Ctrl+C ou duplo clique: copia para a área de transferência e salva.
"""
import json
import math
import os
import random
import sys
import time

import cairo
import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("AyatanaAppIndicator3", "0.1")
gi.require_foreign("cairo")  # python3-gi-cairo: sem isso o overlay fica preto
from gi.repository import AyatanaAppIndicator3 as AppIndicator  # noqa: E402
from gi.repository import Gdk, GdkPixbuf, Gio, GLib, Gtk  # noqa: E402

APP_ID = "io.github.recortei"
CONFIG_DIR = os.path.join(GLib.get_user_config_dir(), "recortei")
CONFIG_FILE = os.path.join(CONFIG_DIR, "config.json")
PICTURES = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_PICTURES) or os.path.expanduser("~")
SAVE_DIR = os.path.join(PICTURES, "Recortei")
HANDLE = 8  # tamanho da zona de redimensionamento (px)

# (id, dica, tecla de atalho)
TOOLS = [
    ("move", "Mover / ajustar área", "v"),
    ("pen", "Caneta", "p"),
    ("line", "Linha", "l"),
    ("arrow", "Seta", "a"),
    ("rect", "Retângulo", "r"),
    ("ellipse", "Elipse", "e"),
    ("text", "Texto", "t"),
    ("marker", "Marca-texto", "h"),
    ("pixelate", "Pixelizar / censurar", "b"),
    ("step", "Numeração", "n"),
]
TOOL_KEYS = {k: t for t, _d, k in TOOLS}
COLORS = [
    (0.92, 0.20, 0.20),  # vermelho
    (1.00, 0.78, 0.05),  # amarelo
    (0.20, 0.78, 0.30),  # verde
    (0.20, 0.55, 1.00),  # azul
    (0.65, 0.35, 0.95),  # roxo
    (0.00, 0.00, 0.00),  # preto
    (1.00, 1.00, 1.00),  # branco
]
WIDTHS = [2, 4, 6, 10, 16]
BTN, SEP, PAD = 32, 9, 5


# ---------------------------------------------------------------- config
def load_config():
    try:
        with open(CONFIG_FILE) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_config(cfg):
    os.makedirs(CONFIG_DIR, exist_ok=True)
    with open(CONFIG_FILE, "w") as f:
        json.dump(cfg, f, indent=2)


# ---------------------------------------------------------------- screenshot via portal
def portal_screenshot(callback):
    """Tira um print da tela inteira via xdg-desktop-portal (funciona no Wayland).
    Chama callback(pixbuf) ou callback(None) em caso de erro."""
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    token = "recortei%d" % random.randint(0, 10**9)
    sender = bus.get_unique_name()[1:].replace(".", "_")
    path = f"/org/freedesktop/portal/desktop/request/{sender}/{token}"
    state = {}

    def on_response(_conn, _s, _p, _i, _sig, params):
        bus.signal_unsubscribe(state["sub"])
        code, results = params.unpack()
        pixbuf = None
        if code == 0 and "uri" in results:
            fpath = Gio.File.new_for_uri(results["uri"]).get_path()
            try:
                pixbuf = GdkPixbuf.Pixbuf.new_from_file(fpath)
            except GLib.Error as e:
                print("Erro lendo screenshot:", e, file=sys.stderr)
            finally:
                try:
                    os.remove(fpath)  # o portal salva em ~/Imagens; não queremos lixo
                except OSError:
                    pass
        callback(pixbuf)

    state["sub"] = bus.signal_subscribe(
        "org.freedesktop.portal.Desktop", "org.freedesktop.portal.Request", "Response",
        path, None, Gio.DBusSignalFlags.NONE, on_response)
    opts = {"handle_token": GLib.Variant("s", token), "interactive": GLib.Variant("b", False)}
    bus.call(
        "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot", "Screenshot",
        GLib.Variant("(sa{sv})", ("", opts)), None, Gio.DBusCallFlags.NONE, -1, None, None)


# ---------------------------------------------------------------- seleção (compartilhada entre monitores)
class Selection:
    """Retângulo em coordenadas lógicas globais (layout do GNOME)."""

    def __init__(self):
        self.rect = None  # (x, y, w, h) ou None
        self.mode = None  # 'new' | 'move' | edge string como 'nw', 'e'...
        self.anchor = None
        self.orig = None

    def contains(self, x, y):
        if not self.rect:
            return False
        rx, ry, rw, rh = self.rect
        return rx <= x <= rx + rw and ry <= y <= ry + rh

    def hit(self, x, y):
        if not self.rect:
            return None
        rx, ry, rw, rh = self.rect
        if not (rx - HANDLE <= x <= rx + rw + HANDLE and ry - HANDLE <= y <= ry + rh + HANDLE):
            return None
        edge = ""
        if abs(y - ry) <= HANDLE:
            edge += "n"
        elif abs(y - (ry + rh)) <= HANDLE:
            edge += "s"
        if abs(x - rx) <= HANDLE:
            edge += "w"
        elif abs(x - (rx + rw)) <= HANDLE:
            edge += "e"
        if edge:
            return edge
        if rx < x < rx + rw and ry < y < ry + rh:
            return "move"
        return None

    def press(self, x, y, mode=None):
        self.anchor = (x, y)
        self.orig = self.rect
        self.mode = mode or self.hit(x, y) or "new"
        if self.mode == "new":
            self.rect = (x, y, 0, 0)

    def motion(self, x, y):
        if not self.mode:
            return False
        ax, ay = self.anchor
        if self.mode == "new":
            self.rect = (min(ax, x), min(ay, y), abs(x - ax), abs(y - ay))
        elif self.mode == "move":
            ox, oy, ow, oh = self.orig
            self.rect = (ox + x - ax, oy + y - ay, ow, oh)
        else:
            ox, oy, ow, oh = self.orig
            x1, y1, x2, y2 = ox, oy, ox + ow, oy + oh
            if "w" in self.mode:
                x1 = x
            if "e" in self.mode:
                x2 = x
            if "n" in self.mode:
                y1 = y
            if "s" in self.mode:
                y2 = y
            self.rect = (min(x1, x2), min(y1, y2), abs(x2 - x1), abs(y2 - y1))
        return True

    def release(self):
        self.mode = None
        if self.rect and (self.rect[2] < 3 or self.rect[3] < 3):
            self.rect = self.orig if self.orig and self.orig[2] >= 3 else None


CURSORS = {
    "n": "n-resize", "s": "s-resize", "e": "e-resize", "w": "w-resize",
    "ne": "ne-resize", "nw": "nw-resize", "se": "se-resize", "sw": "sw-resize",
    "move": "move",
}


# ---------------------------------------------------------------- helpers de desenho
def rounded_rect(cr, x, y, w, h, r):
    cr.new_sub_path()
    cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
    cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
    cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
    cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
    cr.close_path()


def norm_rect(p0, p1):
    return (min(p0[0], p1[0]), min(p0[1], p1[1]), abs(p1[0] - p0[0]), abs(p1[1] - p0[1]))


def arrow_head(cr, x1, y1, x2, y2, head):
    ang = math.atan2(y2 - y1, x2 - x1)
    cr.move_to(x2, y2)
    cr.line_to(x2 - head * math.cos(ang - 0.42), y2 - head * math.sin(ang - 0.42))
    cr.line_to(x2 - head * math.cos(ang + 0.42), y2 - head * math.sin(ang + 0.42))
    cr.close_path()
    cr.fill()


def luminance(c):
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def draw_icon(cr, kind, cx, cy, color):
    """Ícones da barra, desenhados em ~18px centrados em (cx, cy)."""
    cr.save()
    cr.new_path()
    cr.set_source_rgb(0.93, 0.93, 0.95)
    cr.set_line_width(1.8)
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    cr.set_line_join(cairo.LINE_JOIN_ROUND)
    if kind == "move":
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            cr.move_to(cx, cy)
            cr.line_to(cx + dx * 8, cy + dy * 8)
            cr.stroke()
            arrow_head(cr, cx, cy, cx + dx * 9, cy + dy * 9, 5)
    elif kind == "pen":
        cr.move_to(cx - 8, cy + 5)
        cr.curve_to(cx - 4, cy - 8, cx, cy + 8, cx + 3, cy - 2)
        cr.curve_to(cx + 5, cy - 7, cx + 7, cy - 4, cx + 8, cy - 6)
        cr.stroke()
    elif kind == "line":
        cr.move_to(cx - 7, cy + 7)
        cr.line_to(cx + 7, cy - 7)
        cr.stroke()
    elif kind == "arrow":
        cr.move_to(cx - 7, cy + 7)
        cr.line_to(cx + 4, cy - 4)
        cr.stroke()
        arrow_head(cr, cx - 7, cy + 7, cx + 8, cy - 8, 8)
    elif kind == "rect":
        cr.rectangle(cx - 8, cy - 6, 16, 12)
        cr.stroke()
    elif kind == "ellipse":
        cr.save()
        cr.translate(cx, cy)
        cr.scale(9, 7)
        cr.arc(0, 0, 1, 0, 2 * math.pi)
        cr.restore()
        cr.stroke()
    elif kind == "text":
        cr.select_font_face("Serif", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(19)
        ext = cr.text_extents("T")
        cr.move_to(cx - ext.width / 2 - ext.x_bearing, cy - ext.height / 2 - ext.y_bearing)
        cr.show_text("T")
    elif kind == "marker":
        cr.set_source_rgba(color[0], color[1], color[2], 0.55)
        cr.rectangle(cx - 9, cy - 4, 18, 8)
        cr.fill()
        cr.set_source_rgb(0.93, 0.93, 0.95)
        cr.move_to(cx - 9, cy + 7)
        cr.line_to(cx + 9, cy + 7)
        cr.stroke()
    elif kind == "pixelate":
        for i in range(3):
            for j in range(3):
                shade = 0.35 + 0.6 * ((i + j) % 2)
                cr.set_source_rgb(shade, shade, shade + 0.02)
                cr.rectangle(cx - 9 + i * 6, cy - 9 + j * 6, 6, 6)
                cr.fill()
    elif kind == "step":
        cr.arc(cx, cy, 9, 0, 2 * math.pi)
        cr.fill()
        cr.set_source_rgb(0.13, 0.13, 0.15)
        cr.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(12)
        ext = cr.text_extents("1")
        cr.move_to(cx - ext.width / 2 - ext.x_bearing, cy - ext.height / 2 - ext.y_bearing)
        cr.show_text("1")
    elif kind in ("undo", "redo"):
        d = -1 if kind == "undo" else 1
        cr.arc(cx, cy + 3, 6, math.pi, 2 * math.pi)  # meia-volta por cima
        cr.stroke()
        tipx = cx + 6 * d
        arrow_head(cr, tipx, cy, tipx, cy + 8, 6)
    elif kind == "ok":
        cr.set_source_rgb(0.35, 0.85, 0.45)
        cr.set_line_width(2.6)
        cr.move_to(cx - 7, cy)
        cr.line_to(cx - 2, cy + 6)
        cr.line_to(cx + 8, cy - 6)
        cr.stroke()
    elif kind == "close":
        cr.set_source_rgb(0.95, 0.45, 0.45)
        cr.set_line_width(2.4)
        cr.move_to(cx - 6, cy - 6)
        cr.line_to(cx + 6, cy + 6)
        cr.move_to(cx + 6, cy - 6)
        cr.line_to(cx - 6, cy + 6)
        cr.stroke()
    cr.restore()


# ---------------------------------------------------------------- overlay
class Overlay:
    """Uma janela fullscreen por monitor, mostrando o screenshot congelado."""

    def __init__(self, app, pixbuf):
        self.app = app
        self.pixbuf = pixbuf
        self.base = Gdk.cairo_surface_create_from_pixbuf(pixbuf, 1, None)
        display = Gdk.Display.get_default()
        self.monitors = [display.get_monitor(i).get_geometry() for i in range(display.get_n_monitors())]
        self.ox = min(g.x for g in self.monitors)
        self.oy = min(g.y for g in self.monitors)
        total_w = max(g.x + g.width for g in self.monitors) - self.ox
        # escala pixels do screenshot / coordenadas lógicas (HiDPI)
        self.scale = pixbuf.get_width() / total_w
        self.sel = Selection()

        cfg = app.cfg
        self.tool = cfg.get("tool", "rect") if cfg.get("tool") in TOOL_KEYS.values() else "rect"
        ci = cfg.get("color", 0)
        self.color = COLORS[ci] if isinstance(ci, int) and 0 <= ci < len(COLORS) else COLORS[0]
        w = cfg.get("width", 4)
        self.width = w if isinstance(w, int) and 1 <= w <= 40 else 4

        self.anns = []  # anotações confirmadas
        self.redo_stack = []
        self.drawing = None  # anotação sendo arrastada
        self.editing = None  # anotação de texto sendo digitada
        self.click_items = []  # (time, ann) criados por clique simples (p/ desfazer no duplo clique)
        self.hover = None
        self.cursor_name = None

        self.im = Gtk.IMContextSimple()  # trata teclas mortas (´ ~ ^) no texto
        self.im.connect("commit", self.on_im_commit)

        self.windows = []
        for i, geo in enumerate(self.monitors):
            win = Gtk.Window(type=Gtk.WindowType.TOPLEVEL)
            win.set_app_paintable(True)
            win.set_decorated(False)
            win.set_keep_above(True)
            win.set_title("Recortei")
            area = Gtk.DrawingArea()
            area.set_can_focus(True)
            area.add_events(Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.BUTTON_RELEASE_MASK
                            | Gdk.EventMask.POINTER_MOTION_MASK | Gdk.EventMask.KEY_PRESS_MASK
                            | Gdk.EventMask.SCROLL_MASK)
            area.connect("draw", self.on_draw, geo)
            area.connect("button-press-event", self.on_press, geo)
            area.connect("button-release-event", self.on_release, geo)
            area.connect("motion-notify-event", self.on_motion, geo)
            area.connect("scroll-event", self.on_scroll)
            win.connect("key-press-event", self.on_key)
            win.add(area)
            win.show_all()
            win.fullscreen_on_monitor(win.get_screen(), i)
            self.windows.append((win, area))
        GLib.idle_add(lambda: (self.windows[0][0].present(), False)[1])

    # -- coordenadas
    def to_global(self, ev, geo):
        return ev.x + geo.x, ev.y + geo.y

    def redraw(self):
        for _w, a in self.windows:
            a.queue_draw()

    def monitor_of(self, x, y):
        for g in self.monitors:
            if g.x <= x < g.x + g.width and g.y <= y < g.y + g.height:
                return g
        return min(self.monitors, key=lambda g: abs(g.x + g.width / 2 - x) + abs(g.y + g.height / 2 - y))

    # -- anotações
    def new_ann(self, tool, x, y):
        return {"tool": tool, "color": self.color, "width": self.width,
                "pts": [(x, y)] if tool in ("pen", "text", "step") else [(x, y), (x, y)],
                "text": "", "n": 0}

    def next_step(self):
        return 1 + sum(1 for a in self.anns if a["tool"] == "step")

    def push_ann(self, a):
        self.anns.append(a)
        self.redo_stack.clear()

    def undo(self):
        if self.editing:
            self.editing = None
        elif self.anns:
            self.redo_stack.append(self.anns.pop())
        self.redraw()

    def redo(self):
        if self.redo_stack:
            self.anns.append(self.redo_stack.pop())
        self.redraw()

    def commit_text(self):
        a, self.editing = self.editing, None
        if a and a["text"].strip():
            self.push_ann(a)
        self.im.focus_out()
        self.redraw()

    def start_text(self, x, y, widget):
        self.editing = self.new_ann("text", x, y)
        self.im.set_client_window(widget.get_window())
        self.im.focus_in()

    def on_im_commit(self, _im, s):
        if self.editing is not None:
            self.editing["text"] += s
            self.redraw()

    def set_color(self, c):
        self.color = c
        if self.editing:
            self.editing["color"] = c
        self.redraw()

    def set_width(self, w):
        self.width = max(1, min(40, w))
        if self.editing:
            self.editing["width"] = self.width
        self.redraw()

    def pixel_rect(self, x, y, w, h):
        s = self.scale
        return (x - self.ox) * s, (y - self.oy) * s, w * s, h * s

    def draw_ann(self, cr, a, caret=False):
        t, (r, g, b), w, pts = a["tool"], a["color"], a["width"], a["pts"]
        cr.save()
        cr.new_path()
        cr.set_line_cap(cairo.LINE_CAP_ROUND)
        cr.set_line_join(cairo.LINE_JOIN_ROUND)
        cr.set_line_width(w)
        cr.set_source_rgb(r, g, b)
        if t == "pen":
            cr.move_to(*pts[0])
            for p in pts[1:]:
                cr.line_to(*p)
            cr.stroke()
        elif t in ("line", "arrow"):
            (x1, y1), (x2, y2) = pts
            if t == "arrow":
                head = 10 + w * 3
                ang = math.atan2(y2 - y1, x2 - x1)
                ln = math.hypot(x2 - x1, y2 - y1)
                back = min(head * 0.75, ln)
                cr.move_to(x1, y1)
                cr.line_to(x2 - back * math.cos(ang), y2 - back * math.sin(ang))
                cr.stroke()
                arrow_head(cr, x1, y1, x2, y2, head)
            else:
                cr.move_to(x1, y1)
                cr.line_to(x2, y2)
                cr.stroke()
        elif t == "rect":
            cr.set_line_join(cairo.LINE_JOIN_MITER)
            cr.rectangle(*norm_rect(*pts))
            cr.stroke()
        elif t == "ellipse":
            x, y, ew, eh = norm_rect(*pts)
            if ew > 0 and eh > 0:
                cr.save()
                cr.translate(x + ew / 2, y + eh / 2)
                cr.scale(ew / 2, eh / 2)
                cr.arc(0, 0, 1, 0, 2 * math.pi)
                cr.restore()
                cr.stroke()
        elif t == "marker":
            cr.set_source_rgba(r, g, b, 0.38)
            cr.rectangle(*norm_rect(*pts))
            cr.fill()
        elif t == "pixelate":
            self.draw_pixelate(cr, a)
        elif t == "text":
            fs = 12 + w * 3
            cr.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
            cr.set_font_size(fs)
            asc, _desc, lh = cr.font_extents()[:3]
            x, y = pts[0]
            lines = a["text"].split("\n")
            for i, line in enumerate(lines):
                cr.move_to(x, y + asc + i * lh)
                cr.show_text(line)
            if caret:
                cx = x + cr.text_extents(lines[-1]).x_advance + 2
                cy = y + (len(lines) - 1) * lh
                cr.set_line_width(2)
                cr.move_to(cx, cy)
                cr.line_to(cx, cy + lh)
                cr.stroke()
                widest = max(cr.text_extents(ln).x_advance for ln in lines)
                cr.set_source_rgba(1, 1, 1, 0.7)
                cr.set_line_width(1)
                cr.set_dash([4, 3])
                cr.rectangle(x - 4, y - 3, widest + 12, lh * len(lines) + 6)
                cr.stroke()
        elif t == "step":
            rad = 10 + w * 1.6
            x, y = pts[0]
            cr.arc(x, y, rad, 0, 2 * math.pi)
            cr.fill()
            txt = str(a["n"])
            fg = (0, 0, 0) if luminance(a["color"]) > 0.6 else (1, 1, 1)
            cr.set_source_rgb(*fg)
            cr.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
            cr.set_font_size(rad * 1.1)
            ext = cr.text_extents(txt)
            cr.move_to(x - ext.width / 2 - ext.x_bearing, y - ext.height / 2 - ext.y_bearing)
            cr.show_text(txt)
        cr.restore()

    def draw_pixelate(self, cr, a):
        x, y, w, h = norm_rect(*a["pts"])
        if w < 1 or h < 1:
            return
        block = 4 + a["width"] * 2  # tamanho do "pixel" em pixels reais
        key = (x, y, w, h, block)
        if a.get("cache_key") != key:
            px, py, pw, ph = self.pixel_rect(x, y, w, h)
            small = cairo.ImageSurface(cairo.FORMAT_ARGB32,
                                       max(1, math.ceil(pw / block)), max(1, math.ceil(ph / block)))
            c2 = cairo.Context(small)
            c2.scale(1 / block, 1 / block)
            c2.set_source_surface(self.base, -px, -py)
            c2.paint()
            a["cache"], a["cache_key"] = small, key
        cr.save()
        cr.rectangle(x, y, w, h)
        cr.clip()
        cr.translate(x, y)
        cr.scale(block / self.scale, block / self.scale)
        cr.set_source_surface(a["cache"], 0, 0)
        cr.get_source().set_filter(cairo.FILTER_NEAREST)
        cr.paint()
        cr.restore()

    def paint_scene(self, cr):
        """Desenha screenshot + anotações; cr em coordenadas lógicas globais."""
        cr.save()
        cr.translate(self.ox, self.oy)
        cr.scale(1 / self.scale, 1 / self.scale)
        cr.set_source_surface(self.base, 0, 0)
        cr.paint()
        cr.restore()
        for a in self.anns:
            self.draw_ann(cr, a)
        if self.drawing:
            self.draw_ann(cr, self.drawing)
        if self.editing:
            self.draw_ann(cr, self.editing, caret=True)

    # -- barra de ferramentas
    def toolbar_items(self):
        items = [("tool", t) for t, _d, _k in TOOLS] + [("sep", None)]
        items += [("color", c) for c in COLORS] + [("width", None), ("sep", None)]
        items += [("undo", None), ("redo", None), ("sep", None), ("ok", None), ("close", None)]
        return items

    def toolbar_layout(self):
        """Retorna (x, y, w, h, [(item, bx, by, bw, bh)]) ou None."""
        r = self.sel.rect
        if not r or self.sel.mode:
            return None
        items = self.toolbar_items()
        tw = PAD * 2 + sum(SEP if k == "sep" else BTN for k, _v in items)
        th = BTN + PAD * 2
        sx, sy, sw, sh = r
        g = self.monitor_of(sx + sw / 2, sy + sh / 2)
        x = min(max(sx + sw / 2 - tw / 2, g.x + 8), g.x + g.width - tw - 8)
        if sy + sh + 10 + th <= g.y + g.height - 4:
            y = sy + sh + 10
        elif sy - 10 - th >= g.y + 4:
            y = sy - 10 - th
        else:
            y = min(sy + sh, g.y + g.height) - th - 10
        out, cx = [], x + PAD
        for it in items:
            w = SEP if it[0] == "sep" else BTN
            if it[0] != "sep":
                out.append((it, cx, y + PAD, w, BTN))
            cx += w
        return x, y, tw, th, out

    def toolbar_hit(self, x, y):
        lay = self.toolbar_layout()
        if not lay:
            return None
        tx, ty, tw, th, items = lay
        if not (tx <= x <= tx + tw and ty <= y <= ty + th):
            return None
        for it, bx, by, bw, bh in items:
            if bx <= x < bx + bw and by <= y < by + bh:
                return it
        return ("bar", None)

    def tooltip(self, it):
        kind, v = it
        if kind == "tool":
            for t, d, k in TOOLS:
                if t == v:
                    return f"{d} ({k.upper()})"
        return {
            "color": "Cor (teclas 1–7)",
            "width": f"Espessura: {self.width}  (roda do mouse ou + / −)",
            "undo": "Desfazer (Ctrl+Z)",
            "redo": "Refazer (Ctrl+Y)",
            "ok": "Copiar e salvar (Enter / Ctrl+C / duplo clique)",
            "close": "Cancelar (Esc)",
        }.get(kind, "")

    def draw_toolbar(self, cr):
        lay = self.toolbar_layout()
        if not lay:
            return
        tx, ty, tw, th, items = lay
        cr.set_source_rgba(0.12, 0.12, 0.14, 0.96)
        rounded_rect(cr, tx, ty, tw, th, 8)
        cr.fill()
        for it, bx, by, bw, bh in items:
            kind, v = it
            cr.new_path()
            cx, cy = bx + bw / 2, by + bh / 2
            active =(kind == "tool" and v == self.tool)
            if active or self.hover == it:
                cr.set_source_rgba(0.20, 0.55, 1.0, 0.9) if active else cr.set_source_rgba(1, 1, 1, 0.12)
                rounded_rect(cr, bx + 1, by + 1, bw - 2, bh - 2, 6)
                cr.fill()
            if kind == "tool":
                draw_icon(cr, v, cx, cy, self.color)
            elif kind == "color":
                if v == self.color:
                    cr.set_source_rgb(1, 1, 1)
                    cr.arc(cx, cy, 11, 0, 2 * math.pi)
                    cr.fill()
                cr.set_source_rgb(*v)
                cr.arc(cx, cy, 8, 0, 2 * math.pi)
                cr.fill_preserve()
                cr.set_source_rgba(0.5, 0.5, 0.5, 0.8)
                cr.set_line_width(1)
                cr.stroke()
            elif kind == "width":
                cr.set_source_rgb(*self.color)
                cr.arc(cx, cy - 3, max(1.5, min(self.width, 18) / 2), 0, 2 * math.pi)
                cr.fill()
                cr.set_source_rgb(0.9, 0.9, 0.92)
                cr.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
                cr.set_font_size(9)
                txt = str(self.width)
                ext = cr.text_extents(txt)
                cr.move_to(cx - ext.x_advance / 2, by + bh - 3)
                cr.show_text(txt)
            else:
                draw_icon(cr, kind, cx, cy, self.color)
        # separadores
        cxs = tx + PAD
        cr.set_source_rgba(1, 1, 1, 0.18)
        cr.set_line_width(1)
        for kind, _v in self.toolbar_items():
            if kind == "sep":
                cr.move_to(cxs + SEP / 2, ty + 8)
                cr.line_to(cxs + SEP / 2, ty + th - 8)
                cr.stroke()
                cxs += SEP
            else:
                cxs += BTN
        # dica
        if self.hover and self.hover[0] != "bar":
            tip = self.tooltip(self.hover)
            if tip:
                cr.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
                cr.set_font_size(12)
                ext = cr.text_extents(tip)
                g = self.monitor_of(tx + tw / 2, ty + th / 2)
                lx = min(max(tx, g.x + 4), g.x + g.width - ext.width - 20)
                ly = ty + th + 6
                if ly + 24 > g.y + g.height:
                    ly = ty - 30
                cr.set_source_rgba(0, 0, 0, 0.85)
                rounded_rect(cr, lx, ly, ext.x_advance + 16, 24, 5)
                cr.fill()
                cr.set_source_rgb(1, 1, 1)
                cr.move_to(lx + 8, ly + 16)
                cr.show_text(tip)

    # -- desenho
    def on_draw(self, widget, cr, geo):
        cr.translate(-geo.x, -geo.y)  # passa a desenhar em coordenadas globais
        self.paint_scene(cr)
        cr.set_source_rgba(0, 0, 0, 0.45)
        r = self.sel.rect
        if r:
            x, y, w, h = r
            cr.set_fill_rule(cairo.FILL_RULE_EVEN_ODD)
            cr.rectangle(geo.x, geo.y, geo.width, geo.height)
            cr.rectangle(x, y, w, h)
            cr.fill()
            cr.set_fill_rule(cairo.FILL_RULE_WINDING)
            cr.set_source_rgb(0.20, 0.60, 1.0)
            cr.set_line_width(2)
            cr.rectangle(x, y, w, h)
            cr.stroke()
            for hx, hy in ((x, y), (x + w, y), (x, y + h), (x + w, y + h),
                           (x + w / 2, y), (x + w / 2, y + h), (x, y + h / 2), (x + w, y + h / 2)):
                cr.rectangle(hx - 4, hy - 4, 8, 8)
            cr.fill()
            s = self.scale
            label = f"{int(w * s)} × {int(h * s)}"
            cr.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
            cr.set_font_size(13)
            ext = cr.text_extents(label)
            lay = self.toolbar_layout()
            above_free = y - geo.y > 30 and not (lay and lay[1] < y)
            ly = y - 10 if above_free else y + ext.height + 14
            lx = x + (2 if above_free else 8)
            cr.set_source_rgba(0, 0, 0, 0.75)
            cr.rectangle(lx - 6, ly - ext.height - 6, ext.width + 12, ext.height + 12)
            cr.fill()
            cr.set_source_rgb(1, 1, 1)
            cr.move_to(lx, ly)
            cr.show_text(label)
            self.draw_toolbar(cr)
        else:
            cr.paint()
            cr.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
            cr.set_font_size(18)
            msg = "Arraste para selecionar a área · Esc para cancelar"
            ext = cr.text_extents(msg)
            cr.set_source_rgb(1, 1, 1)
            cr.move_to(geo.x + (geo.width - ext.width) / 2, geo.y + geo.height / 2)
            cr.show_text(msg)
        return False

    # -- mouse
    def on_press(self, widget, ev, geo):
        widget.grab_focus()
        x, y = self.to_global(ev, geo)
        tb = self.toolbar_hit(x, y)
        if tb:
            if ev.type == Gdk.EventType.BUTTON_PRESS and ev.button == 1:
                self.toolbar_click(tb)
            return True

        if ev.type == Gdk.EventType._2BUTTON_PRESS:
            if ev.button == 1 and self.sel.contains(x, y):
                # desfaz o que os dois cliques simples criaram (passo, texto vazio...)
                self.drawing = None
                for t, a in self.click_items:
                    if ev.time - t < 1000 and a in self.anns:
                        self.anns.remove(a)
                if self.editing and not self.editing["text"].strip():
                    self.editing = None
                self.sel.mode = None
                self.confirm()
            return True
        if ev.type != Gdk.EventType.BUTTON_PRESS:
            return True

        if ev.button == 3:
            if self.editing:
                self.commit_text()
            elif self.drawing:
                self.drawing = None
            elif self.sel.rect:  # recomeça a seleção do zero
                self.sel = Selection()
                self.anns.clear()
                self.redo_stack.clear()
            else:
                self.close()
                return True
            self.redraw()
            return True
        if ev.button != 1:
            return True

        if self.editing:
            self.commit_text()
        self.click_items = [(t, a) for t, a in self.click_items if ev.time - t < 1000]

        if not self.sel.rect:
            self.sel.press(x, y, "new")
        else:
            h = self.sel.hit(x, y)
            if h and h != "move":
                self.sel.press(x, y)  # redimensionar pelas bordas, com qualquer ferramenta
            elif self.tool == "move":
                self.sel.press(x, y)
            elif self.tool == "text":
                self.start_text(x, y, widget)
            elif self.tool == "step":
                a = self.new_ann("step", x, y)
                a["n"] = self.next_step()
                self.push_ann(a)
                self.click_items.append((ev.time, a))
            else:
                self.drawing = self.new_ann(self.tool, x, y)
        self.redraw()
        return True

    def on_release(self, widget, ev, geo):
        if self.sel.mode:
            self.sel.release()
        elif self.drawing:
            a, self.drawing = self.drawing, None
            xs = [p[0] for p in a["pts"]]
            ys = [p[1] for p in a["pts"]]
            if max(xs) - min(xs) >= 3 or max(ys) - min(ys) >= 3:
                self.push_ann(a)
        x, y = self.to_global(ev, geo)
        self.update_cursor(widget, x, y)
        self.redraw()
        return True

    def on_motion(self, widget, ev, geo):
        x, y = self.to_global(ev, geo)
        if self.sel.motion(x, y):
            self.redraw()
        elif self.drawing:
            a = self.drawing
            if a["tool"] == "pen":
                a["pts"].append((x, y))
            else:
                x0, y0 = a["pts"][0]
                if ev.state & Gdk.ModifierType.SHIFT_MASK:
                    dx, dy = x - x0, y - y0
                    if a["tool"] in ("line", "arrow"):  # trava em múltiplos de 45°
                        ang = round(math.atan2(dy, dx) / (math.pi / 4)) * (math.pi / 4)
                        ln = math.hypot(dx, dy)
                        x, y = x0 + ln * math.cos(ang), y0 + ln * math.sin(ang)
                    else:  # quadrado / círculo
                        d = max(abs(dx), abs(dy))
                        x, y = x0 + math.copysign(d, dx), y0 + math.copysign(d, dy)
                a["pts"][1] = (x, y)
            self.redraw()
        else:
            self.update_cursor(widget, x, y)
        return True

    def update_cursor(self, widget, x, y):
        tb = self.toolbar_hit(x, y)
        if tb != self.hover:
            self.hover = tb
            self.redraw()
        if tb:
            name = "default"
        elif not self.sel.rect:
            name = "crosshair"
        else:
            h = self.sel.hit(x, y)
            if h and h != "move":
                name = CURSORS[h]
            elif self.tool == "move":
                name = "move" if h == "move" else "crosshair"
            elif self.tool == "text":
                name = "text"
            else:
                name = "crosshair"
        if name != self.cursor_name:
            self.cursor_name = name
            for _w, a in self.windows:
                gw = a.get_window()
                if gw:
                    gw.set_cursor(Gdk.Cursor.new_from_name(a.get_display(), name))

    def on_scroll(self, widget, ev):
        if ev.direction == Gdk.ScrollDirection.UP:
            self.set_width(self.width + 1)
        elif ev.direction == Gdk.ScrollDirection.DOWN:
            self.set_width(self.width - 1)
        return True

    def toolbar_click(self, it):
        kind, v = it
        if kind == "tool":
            if self.editing:
                self.commit_text()
            self.tool = v
        elif kind == "color":
            self.set_color(v)
        elif kind == "width":
            nxt = [w for w in WIDTHS if w > self.width]
            self.set_width(nxt[0] if nxt else WIDTHS[0])
        elif kind == "undo":
            self.undo()
        elif kind == "redo":
            self.redo()
        elif kind == "ok":
            self.confirm()
            return
        elif kind == "close":
            self.close()
            return
        self.cursor_name = None
        self.redraw()

    # -- teclado
    def on_key(self, widget, ev):
        key = ev.keyval
        low = Gdk.keyval_to_lower(key)
        ctrl = bool(ev.state & Gdk.ModifierType.CONTROL_MASK)
        shift = bool(ev.state & Gdk.ModifierType.SHIFT_MASK)
        enter = key in (Gdk.KEY_Return, Gdk.KEY_KP_Enter)

        if self.editing:
            if key == Gdk.KEY_Escape:
                self.editing = None
                self.im.reset()
            elif enter and shift:
                self.editing["text"] += "\n"
            elif enter:
                self.commit_text()
            elif ctrl and low == Gdk.KEY_c:
                self.commit_text()
                self.confirm()
                return True
            elif ctrl and low == Gdk.KEY_z:
                self.editing = None
            elif key == Gdk.KEY_BackSpace:
                self.editing["text"] = self.editing["text"][:-1]
            elif not ctrl and not self.im.filter_keypress(ev):
                ch = Gdk.keyval_to_unicode(key)
                if ch >= 32:
                    self.editing["text"] += chr(ch)
            self.redraw()
            return True

        if key == Gdk.KEY_Escape:
            self.close()
        elif enter or key == Gdk.KEY_Insert or (ctrl and low == Gdk.KEY_c):
            self.confirm()
        elif ctrl and low == Gdk.KEY_z:
            self.redo() if shift else self.undo()
        elif ctrl and low == Gdk.KEY_y:
            self.redo()
        elif self.sel.rect and key in (Gdk.KEY_Left, Gdk.KEY_Right, Gdk.KEY_Up, Gdk.KEY_Down):
            step = 10 if shift else 1
            dx = {Gdk.KEY_Left: -step, Gdk.KEY_Right: step}.get(key, 0)
            dy = {Gdk.KEY_Up: -step, Gdk.KEY_Down: step}.get(key, 0)
            x, y, w, h = self.sel.rect
            if ctrl:  # Ctrl+setas redimensiona
                self.sel.rect = (x, y, max(3, w + dx), max(3, h + dy))
            else:
                self.sel.rect = (x + dx, y + dy, w, h)
            self.redraw()
        elif ctrl:
            pass
        elif key in (Gdk.KEY_plus, Gdk.KEY_KP_Add, Gdk.KEY_equal):
            self.set_width(self.width + 1)
        elif key in (Gdk.KEY_minus, Gdk.KEY_KP_Subtract):
            self.set_width(self.width - 1)
        elif Gdk.KEY_1 <= key <= Gdk.KEY_1 + len(COLORS) - 1:
            self.set_color(COLORS[key - Gdk.KEY_1])
        else:
            name = Gdk.keyval_name(low) or ""
            if name in TOOL_KEYS:
                self.tool = TOOL_KEYS[name]
                self.cursor_name = None
                self.redraw()
        return True

    # -- resultado
    def confirm(self):
        if self.editing:
            self.commit_text()
        r = self.sel.rect
        if not r:
            return
        s = self.scale
        x, y, w, h = r
        px = max(0, int(round((x - self.ox) * s)))
        py = max(0, int(round((y - self.oy) * s)))
        pw = min(int(round(w * s)), self.pixbuf.get_width() - px)
        ph = min(int(round(h * s)), self.pixbuf.get_height() - py)
        if pw <= 0 or ph <= 0:
            return
        surf = cairo.ImageSurface(cairo.FORMAT_ARGB32, pw, ph)
        cr = cairo.Context(surf)
        cr.translate(-px, -py)
        cr.scale(s, s)
        cr.translate(-self.ox, -self.oy)
        self.paint_scene(cr)
        surf.flush()
        crop = Gdk.pixbuf_get_from_surface(surf, 0, 0, pw, ph)
        # Clipboard precisa ser setado enquanto temos foco (Wayland)
        Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_image(crop)
        os.makedirs(SAVE_DIR, exist_ok=True)
        fname = os.path.join(SAVE_DIR, time.strftime("Recorte_%Y-%m-%d_%H-%M-%S.png"))
        crop.savev(fname, "png", [], [])
        self.close()
        self.app.notify_saved(fname, pw, ph)

    def close(self):
        self.app.remember_prefs(self.tool, COLORS.index(self.color), self.width)
        for w, _a in self.windows:
            w.destroy()
        self.windows = []
        self.app.overlay = None


# ---------------------------------------------------------------- app
class Recortei(Gtk.Application):
    def __init__(self):
        super().__init__(application_id=APP_ID, flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE)
        self.overlay = None
        self.busy = False
        self.cfg = load_config()
        self.cfg.pop("last_rect", None)  # versões antigas lembravam a área
        self.indicator = None

    def do_startup(self):
        Gtk.Application.do_startup(self)
        act = Gio.SimpleAction.new("capture", None)
        act.connect("activate", lambda *_: self.capture())
        self.add_action(act)
        self.build_tray()
        self.hold()  # continua vivo sem janelas

    def do_command_line(self, cmdline):
        args = cmdline.get_arguments()[1:]
        if "--capture" in args:
            self.capture()
        if "--quit" in args:
            self.quit()
        return 0

    # -- bandeja
    def build_tray(self):
        here = os.path.dirname(os.path.abspath(__file__))
        icon = os.path.join(here, "recortei.svg")
        self.indicator = AppIndicator.Indicator.new(
            "recortei", icon if os.path.exists(icon) else "applets-screenshooter-symbolic",
            AppIndicator.IndicatorCategory.APPLICATION_STATUS)
        self.indicator.set_status(AppIndicator.IndicatorStatus.ACTIVE)
        self.indicator.set_title("Recortei")
        menu = Gtk.Menu()

        def item(label, cb):
            mi = Gtk.MenuItem(label=label)
            mi.connect("activate", lambda *_: cb())
            menu.append(mi)
            return mi

        cap = item("Capturar área (Insert)", lambda: GLib.timeout_add(300, lambda: (self.capture(), False)[1]))
        self.indicator.set_secondary_activate_target(cap)
        item("Abrir pasta de recortes", self.open_folder)
        menu.append(Gtk.SeparatorMenuItem())
        item("Sair", self.quit)
        menu.show_all()
        self.indicator.set_menu(menu)

    def open_folder(self):
        os.makedirs(SAVE_DIR, exist_ok=True)
        Gio.AppInfo.launch_default_for_uri(GLib.filename_to_uri(SAVE_DIR), None)

    def remember_prefs(self, tool, color, width):
        self.cfg.update(tool=tool, color=color, width=width)
        save_config(self.cfg)

    # -- captura
    def capture(self):
        if self.overlay:
            self.overlay.windows[0][0].present()
            return
        if self.busy:
            return
        self.busy = True

        def got(pixbuf):
            self.busy = False
            if pixbuf is None:
                self.notify("Falha ao capturar a tela", "O portal de screenshot não retornou imagem.")
                return
            self.overlay = Overlay(self, pixbuf)

        portal_screenshot(got)

    def notify(self, title, body, fname=None):
        n = Gio.Notification.new(title)
        n.set_body(body)
        self.send_notification("recortei", n)

    def notify_saved(self, fname, w, h):
        self.notify("Recorte copiado", f"{w}×{h} — salvo em {fname}")


if __name__ == "__main__":
    sys.exit(Recortei().run(sys.argv))
