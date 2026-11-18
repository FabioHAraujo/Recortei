//! A janela do winit das Configurações: cria, desenha com o [`view`] e
//! traduz eventos para o [`Model`].

use std::num::NonZeroU32;
use std::rc::Rc;

use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, OwnedDisplayHandle};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use super::view;
use super::{Action, Model, Nav};
use crate::hotkey::{self, Hotkey, Recorded, Status};
use crate::overlay::to_xrgb;

pub struct SettingsWindow {
    window: Rc<Window>,
    surface: Surface<OwnedDisplayHandle, Rc<Window>>,
    frame: Option<Pixmap>,
    pub model: Model,
    mods: ModifiersState,
    /// Último ponto do mouse (px lógicos).
    pointer: (f32, f32),
    cursor: CursorIcon,
    shown: bool,
}

impl SettingsWindow {
    /// `header`: desenha a própria barra de título (Wayland, onde o winit
    /// sem decorações do servidor mostraria uma moldura cinza sem título).
    pub fn open(
        el: &ActiveEventLoop,
        context: &Context<OwnedDisplayHandle>,
        model: Model,
    ) -> Result<Self, String> {
        let size = LogicalSize::new(view::WIDTH, view::window_height(model.header));
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title("Configurações do Recortei")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_resizable(false)
            .with_decorations(!model.header)
            .with_visible(!cfg!(windows))
            .with_window_icon(window_icon());
        if let Some(m) = el
            .primary_monitor()
            .or_else(|| el.available_monitors().next())
        {
            // centraliza (o Wayland ignora; lá o compositor decide)
            let sf = m.scale_factor();
            let (w, h) = (size.width as f64 * sf, size.height as f64 * sf);
            let (mw, mh) = (m.size().width as f64, m.size().height as f64);
            let pos = m.position();
            attrs = attrs.with_position(PhysicalPosition::new(
                pos.x + ((mw - w) / 2.0).max(0.0) as i32,
                pos.y + ((mh - h) / 2.0).max(0.0) as i32,
            ));
        }
        #[cfg(target_os = "linux")]
        {
            attrs = if model.header {
                winit::platform::wayland::WindowAttributesExtWayland::with_name(
                    attrs,
                    crate::app::APP_ID,
                    "recortei",
                )
            } else {
                winit::platform::x11::WindowAttributesExtX11::with_name(
                    attrs, "recortei", "Recortei",
                )
            };
        }
        let window = Rc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        let surface = Surface::new(context, window.clone()).map_err(|e| e.to_string())?;
        window.request_redraw();
        Ok(SettingsWindow {
            window,
            surface,
            frame: None,
            model,
            mods: ModifiersState::empty(),
            pointer: (-1.0, -1.0),
            cursor: CursorIcon::Default,
            shown: false,
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.set_minimized(false);
        self.window.focus_window();
    }

    pub fn recording(&self) -> bool {
        self.model.recording
    }

    pub fn set_status(&mut self, status: Status) {
        self.model.status = status;
        self.refresh_hover();
        self.window.request_redraw();
    }

    pub fn set_light(&mut self, light: bool) {
        if self.model.light != light {
            self.model.light = light;
            self.window.request_redraw();
        }
    }

    pub fn set_snipping(&mut self, owned: bool, fixed: bool) {
        self.model.snipping = owned;
        self.model.snipping_fixed = fixed;
        self.refresh_hover();
        self.window.request_redraw();
    }

    pub fn hotkey(&self) -> Hotkey {
        self.model.hotkey
    }

    fn scale(&self) -> f32 {
        let w = self.window.inner_size().width as f32;
        if w > 0.0 {
            w / view::WIDTH
        } else {
            self.window.scale_factor() as f32
        }
    }

    fn render(&mut self) {
        let size = self.window.inner_size();
        let (Some(nw), Some(nh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        if self
            .frame
            .as_ref()
            .is_none_or(|f| f.width() != size.width || f.height() != size.height)
        {
            self.frame = Pixmap::new(size.width, size.height);
        }
        let scale = self.scale();
        let v = self.model.view();
        let Some(frame) = self.frame.as_mut() else {
            return;
        };
        view::render(&v, frame, scale);
        if let Err(e) = self.surface.resize(nw, nh) {
            eprintln!("Erro redimensionando superfície: {e}");
            return;
        }
        match self.surface.buffer_mut() {
            Ok(mut buf) => {
                to_xrgb(frame, &mut buf);
                if let Err(e) = buf.present() {
                    eprintln!("Erro apresentando quadro: {e}");
                }
            }
            Err(e) => eprintln!("Erro obtendo buffer: {e}"),
        }
        if !self.shown {
            self.shown = true;
            self.window.set_visible(true);
            self.window.focus_window();
        }
    }

    fn update_cursor(&mut self) {
        let c = if self.model.hover.is_some() {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        };
        if c != self.cursor {
            self.cursor = c;
            self.window.set_cursor(c);
        }
    }

    /// O layout pode mudar sob o mouse parado (o botão de gravar encolhe
    /// durante a gravação, o "Desativar no Windows" some): recalcula o hover
    /// e o cursor sem esperar o mouse mexer.
    fn refresh_hover(&mut self) {
        if self.pointer.0 >= 0.0 {
            self.model.pointer_moved(self.pointer.0, self.pointer.1);
            self.update_cursor();
        }
    }

    /// Trata um evento da janela; devolve o que o app deve fazer.
    pub fn event(&mut self, event: WindowEvent) -> Option<Action> {
        let action = match event {
            WindowEvent::RedrawRequested => {
                self.render();
                return None;
            }
            WindowEvent::CloseRequested => Some(Action::Close),
            WindowEvent::ThemeChanged(t) => {
                self.set_light(t == winit::window::Theme::Light);
                None
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                None
            }
            WindowEvent::Focused(false) => self.model.focus_lost(),
            WindowEvent::CursorMoved { position, .. } => {
                let s = self.scale();
                self.pointer = (position.x as f32 / s, position.y as f32 / s);
                self.model.pointer_moved(self.pointer.0, self.pointer.1);
                self.update_cursor();
                None
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = (-1.0, -1.0);
                self.model.pointer_left();
                self.update_cursor();
                None
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.pointer;
                match state {
                    ElementState::Pressed if self.model.on_header(x, y) => {
                        let _ = self.window.drag_window();
                        None
                    }
                    ElementState::Pressed => {
                        self.model.press(x, y);
                        None
                    }
                    ElementState::Released => self.model.release(x, y),
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if self.model.recording && event.repeat {
                    None // tecla segurada (ex.: o Espaço que começou a gravação)
                } else if self.model.recording {
                    // o Windows não entrega o "aperto" da Print Screen, só a soltura
                    let print = event.logical_key == Key::Named(NamedKey::PrintScreen);
                    if pressed || print {
                        let r = hotkey::record(event.physical_key, &event.logical_key, self.mods);
                        let r = match r {
                            Recorded::Invalid(_) if !pressed => Recorded::Waiting,
                            r => r,
                        };
                        self.model.recorded(r)
                    } else {
                        None
                    }
                } else if pressed {
                    let shift = self.mods.shift_key();
                    match event.logical_key {
                        Key::Named(NamedKey::Tab) => self.model.nav(Nav::Tab { back: shift }),
                        Key::Named(NamedKey::Enter | NamedKey::Space) if !event.repeat => {
                            self.model.nav(Nav::Activate)
                        }
                        Key::Named(NamedKey::Escape) => self.model.nav(Nav::Escape),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => return None,
        };
        self.refresh_hover();
        // foco, hover, gravação: qualquer um desses muda o desenho
        self.window.request_redraw();
        action
    }
}

fn window_icon() -> Option<winit::window::Icon> {
    let size = 32;
    winit::window::Icon::from_rgba(crate::draw::app_icon_rgba(size, true), size, size).ok()
}
