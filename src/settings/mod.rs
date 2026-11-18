//! Janela "Configurações do Recortei": escolher o atalho de captura.
//!
//! - `mod.rs` (este): estado e regras (teclado, botões, textos de situação),
//!   sem winit, testável;
//! - `view.rs`: layout e desenho puros num `Pixmap` (com imagens de referência);
//! - `window.rs`: a janela do winit que liga as duas coisas.

mod view;
mod window;

pub use self::window::SettingsWindow;

use self::view::{Btn, Tone, View};
use crate::hotkey::{self, Hotkey, Recorded, Status};

/// O que a janela pede ao app.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Salvar e aplicar já (Windows registra de novo; GNOME troca a tecla).
    Apply(Hotkey),
    /// Começou a gravar: no Windows o atalho global é solto enquanto isso.
    StartRecording,
    /// Gravação cancelada: volta o atalho atual.
    StopRecording,
    /// "Desativar no Windows" (Ferramenta de Captura na tecla Print Screen).
    FixSnipping,
    Close,
}

/// Teclas de navegação fora da gravação.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    Tab { back: bool },
    Activate,
    Escape,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub hotkey: Hotkey,
    pub status: Status,
    /// Windows: a Ferramenta de Captura está com a tecla Print Screen.
    pub snipping: bool,
    /// O usuário clicou em "Desativar no Windows" nesta sessão.
    pub snipping_fixed: bool,
    /// Linux: mostra a nota sobre teclas que o GNOME segura (some quando a
    /// situação é [`Status::Manual`], fora do GNOME).
    pub gnome: bool,
    pub recording: bool,
    /// Por que a última tecla gravada foi recusada.
    pub record_hint: Option<String>,
    pub light: bool,
    /// Barra de título desenhada pela própria janela (Wayland).
    pub header: bool,
    pub focus: Option<Btn>,
    /// O anel de foco só aparece depois de usar o teclado (como `:focus-visible`).
    pub focus_visible: bool,
    pub hover: Option<Btn>,
    pub pressed: Option<Btn>,
}

impl Model {
    pub fn new(hotkey: Hotkey, status: Status) -> Self {
        Model {
            hotkey,
            status,
            snipping: false,
            snipping_fixed: false,
            gnome: false,
            recording: false,
            record_hint: None,
            light: false,
            header: false,
            focus: None,
            focus_visible: false,
            hover: None,
            pressed: None,
        }
    }

    /// Botão "Desativar no Windows": aparece com o atalho ativo ou recusado
    /// (com a opção ligada, o Windows costuma recusar a tecla ao Recortei).
    fn show_fix(&self) -> bool {
        matches!(self.status, Status::Active | Status::Taken)
            && self.snipping
            && !self.snipping_fixed
            && self.hotkey.uses_print_screen()
    }

    /// Botões na ordem do Tab.
    fn focus_order(&self) -> Vec<Btn> {
        let mut v: Vec<Btn> = (0..hotkey::PRESETS.len()).map(Btn::Preset).collect();
        v.push(Btn::Record);
        if self.show_fix() {
            v.push(Btn::FixSnipping);
        }
        v.push(Btn::Close);
        v
    }

    /// Linha de situação: tom e texto.
    pub fn status_line(&self) -> (Tone, String) {
        let label = self.hotkey.label();
        match &self.status {
            Status::Pending => (Tone::Busy, "Aplicando…".into()),
            Status::Taken if self.show_fix() => (
                Tone::Warn,
                "A Ferramenta de Captura do Windows está usando a tecla Print Screen.".into(),
            ),
            Status::Taken if self.snipping_fixed && self.hotkey.uses_print_screen() => (
                Tone::Warn,
                "A tecla Print Screen ainda está com a Ferramenta de Captura. Saia e entre de \
                 novo no Windows."
                    .into(),
            ),
            Status::Taken => (
                Tone::Error,
                format!("{label} já é usado por outro programa. Escolha outro atalho."),
            ),
            Status::Error(e) => (Tone::Error, e.clone()),
            Status::Manual => (
                Tone::Info,
                format!(
                    "Crie um atalho global no seu ambiente: {label} →\u{a0}recortei\u{a0}--capture"
                ),
            ),
            Status::Active if self.show_fix() => (
                Tone::Warn,
                "A Ferramenta de Captura do Windows está usando a tecla Print Screen.".into(),
            ),
            Status::Active if self.snipping_fixed && self.hotkey.uses_print_screen() => (
                Tone::Ok,
                "Ativo. Se a tecla ainda abrir a Ferramenta de Captura, saia e entre de novo \
                 no Windows."
                    .into(),
            ),
            Status::Active => (
                Tone::Ok,
                format!("Ativo: aperte {label} em qualquer lugar para capturar."),
            ),
        }
    }

    pub fn view(&self) -> View {
        let labels: Vec<String> = hotkey::PRESETS.iter().map(|p| p.to_string()).collect();
        let active_preset = labels.iter().position(|p| *p == self.hotkey.label());
        let (tone, text) = self.status_line();
        View {
            light: self.light,
            header: self.header,
            keys: self.hotkey.parts(),
            presets: labels,
            active_preset,
            recording: self.recording,
            record_hint: self.record_hint.clone(),
            status: (tone, text),
            fix_button: self.show_fix(),
            note: (self.gnome && self.status != Status::Manual).then(|| {
                "No GNOME, algumas teclas ficam com o sistema; prefira as sugestões.".into()
            }),
            focus: self.focus.filter(|_| self.focus_visible),
            hover: self.hover,
            pressed: self.pressed,
        }
    }

    /// Clique ou Enter/Espaço num botão.
    pub fn activate(&mut self, b: Btn) -> Option<Action> {
        match b {
            Btn::Preset(i) => {
                let hk = Hotkey::parse(hotkey::PRESETS.get(i)?).ok()?;
                self.recording = false;
                self.record_hint = None;
                Some(self.choose(hk))
            }
            Btn::Record if self.recording => {
                self.recording = false;
                self.record_hint = None;
                Some(Action::StopRecording)
            }
            Btn::Record => {
                self.recording = true;
                self.record_hint = None;
                Some(Action::StartRecording)
            }
            Btn::FixSnipping => Some(Action::FixSnipping),
            Btn::Close | Btn::HeaderClose => Some(Action::Close),
        }
    }

    fn choose(&mut self, hk: Hotkey) -> Action {
        self.hotkey = hk;
        self.status = Status::Pending;
        if !self
            .focus_order()
            .contains(&self.focus.unwrap_or(Btn::Close))
        {
            self.focus = Some(Btn::Record);
        }
        Action::Apply(hk)
    }

    /// Tecla durante a gravação (já interpretada por [`hotkey::record`]).
    pub fn recorded(&mut self, r: Recorded) -> Option<Action> {
        if !self.recording {
            return None;
        }
        match r {
            Recorded::Waiting => None,
            Recorded::Cancel => self.activate(Btn::Record),
            Recorded::Invalid(why) => {
                self.record_hint = Some(why);
                None
            }
            Recorded::Hotkey(hk) => {
                self.recording = false;
                self.record_hint = None;
                Some(self.choose(hk))
            }
        }
    }

    pub fn nav(&mut self, n: Nav) -> Option<Action> {
        match n {
            Nav::Escape if self.recording => self.activate(Btn::Record),
            Nav::Escape => Some(Action::Close),
            // um botão que sumiu (ex.: "Desativar no Windows" depois do clique) não fica com o foco
            Nav::Activate => self.visible_focus().and_then(|b| self.activate(b)),
            Nav::Tab { back } => {
                let order = self.focus_order();
                let i = self
                    .visible_focus()
                    .and_then(|f| order.iter().position(|&b| b == f));
                let n = order.len();
                let next = match (i, back) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                };
                self.focus = Some(order[next]);
                self.focus_visible = true;
                None
            }
        }
    }

    /// Botão focado, se ainda está na tela.
    fn visible_focus(&self) -> Option<Btn> {
        self.focus.filter(|f| self.focus_order().contains(f))
    }

    /// A janela perdeu o foco: sem foco ela não recebe teclas, então a
    /// gravação é cancelada (e o atalho global volta).
    pub fn focus_lost(&mut self) -> Option<Action> {
        self.pressed = None;
        if self.recording {
            self.activate(Btn::Record)
        } else {
            None
        }
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32) {
        self.hover = view::hit(&self.view(), x, y);
    }

    pub fn pointer_left(&mut self) {
        self.hover = None;
        self.pressed = None;
    }

    pub fn press(&mut self, x: f32, y: f32) {
        self.pressed = view::hit(&self.view(), x, y);
    }

    /// Solta o botão do mouse: ativa se ainda estiver sobre o mesmo botão.
    pub fn release(&mut self, x: f32, y: f32) -> Option<Action> {
        let pressed = self.pressed.take()?;
        if view::hit(&self.view(), x, y) != Some(pressed) {
            return None;
        }
        if pressed != Btn::HeaderClose {
            self.focus = Some(pressed);
            self.focus_visible = false;
        }
        self.activate(pressed)
    }

    /// Está sobre a barra de título própria (fora do botão de fechar)?
    pub fn on_header(&self, x: f32, y: f32) -> bool {
        self.header && y < view::HEADER && view::hit(&self.view(), x, y).is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model::new(hotkey::DEFAULT, Status::Active)
    }

    #[test]
    fn presets_apply_immediately() {
        let mut m = model();
        let a = m.activate(Btn::Preset(2));
        let hk = Hotkey::parse("Ctrl+Shift+S").unwrap();
        assert_eq!(a, Some(Action::Apply(hk)));
        assert_eq!((m.hotkey, &m.status), (hk, &Status::Pending));
        assert_eq!(m.view().active_preset, Some(2));
        assert_eq!(m.status_line().0, Tone::Busy);
    }

    #[test]
    fn recording_flow() {
        let mut m = model();
        assert_eq!(m.activate(Btn::Record), Some(Action::StartRecording));
        assert!(m.recording);
        assert_eq!(m.recorded(Recorded::Waiting), None);
        assert_eq!(m.recorded(Recorded::Invalid("x".into())), None);
        assert_eq!(m.view().record_hint.as_deref(), Some("x"));
        assert_eq!(
            m.nav(Nav::Escape),
            Some(Action::StopRecording),
            "Esc cancela"
        );
        assert!(!m.recording);
        assert_eq!(m.nav(Nav::Escape), Some(Action::Close), "Esc fecha");
        m.activate(Btn::Record);
        let hk = Hotkey::parse("Alt+F9").unwrap();
        assert_eq!(m.recorded(Recorded::Hotkey(hk)), Some(Action::Apply(hk)));
        assert!(!m.recording && m.view().active_preset.is_none());
        assert_eq!(m.recorded(Recorded::Hotkey(hk)), None, "fora da gravação");
    }

    #[test]
    fn keyboard_focus_cycles() {
        let mut m = model();
        m.focus = None;
        m.nav(Nav::Tab { back: false });
        assert_eq!(m.focus, Some(Btn::Preset(0)));
        m.nav(Nav::Tab { back: true });
        assert_eq!(m.focus, Some(Btn::Close));
        m.nav(Nav::Tab { back: true });
        assert_eq!(m.focus, Some(Btn::Record));
        assert_eq!(m.nav(Nav::Activate), Some(Action::StartRecording));
        // o botão do Windows entra na ordem só quando aparece
        let mut m = model();
        m.snipping = true;
        m.focus = Some(Btn::Record);
        m.nav(Nav::Tab { back: false });
        assert_eq!(m.focus, Some(Btn::FixSnipping));
        assert_eq!(m.nav(Nav::Activate), Some(Action::FixSnipping));
    }

    #[test]
    fn hidden_fix_button_loses_focus() {
        let mut m = model();
        m.snipping = true;
        m.focus = Some(Btn::FixSnipping);
        assert_eq!(m.nav(Nav::Activate), Some(Action::FixSnipping));
        // clicou: o botão some
        m.snipping_fixed = true;
        assert_eq!(
            m.nav(Nav::Activate),
            None,
            "Enter não ativa botão escondido"
        );
        m.nav(Nav::Tab { back: false });
        assert_eq!(m.focus, Some(Btn::Preset(0)));
        m.focus = Some(Btn::Record);
        m.nav(Nav::Tab { back: false });
        assert_eq!(m.focus, Some(Btn::Close), "pula o botão escondido");
        // mesmo com erro depois do clique
        let mut m = model();
        m.snipping = true;
        m.focus = Some(Btn::FixSnipping);
        m.status = Status::Error("x".into());
        assert_eq!(m.nav(Nav::Activate), None);
        m.nav(Nav::Tab { back: true });
        assert_eq!(m.focus, Some(Btn::Close));
    }

    #[test]
    fn focus_loss_cancels_recording() {
        let mut m = model();
        assert_eq!(m.focus_lost(), None);
        m.activate(Btn::Record);
        m.pressed = Some(Btn::Record);
        assert_eq!(m.focus_lost(), Some(Action::StopRecording));
        assert!(!m.recording && m.pressed.is_none());
        assert_eq!(m.focus_lost(), None);
    }

    #[test]
    fn status_texts() {
        let mut m = model();
        assert_eq!(
            m.status_line(),
            (
                Tone::Ok,
                "Ativo: aperte PrintScreen em qualquer lugar para capturar.".into()
            )
        );
        m.snipping = true;
        assert_eq!(m.status_line().0, Tone::Warn);
        assert!(m.view().fix_button);
        m.snipping_fixed = true;
        assert_eq!(m.status_line().0, Tone::Ok);
        assert!(!m.view().fix_button);
        // com a opção ligada o Windows costuma recusar a tecla ao Recortei
        m.snipping_fixed = false;
        m.status = Status::Taken;
        assert_eq!(
            m.status_line(),
            (
                Tone::Warn,
                "A Ferramenta de Captura do Windows está usando a tecla Print Screen.".into()
            )
        );
        assert!(m.view().fix_button);
        m.snipping_fixed = true;
        assert_eq!(m.status_line().0, Tone::Warn);
        assert!(m.status_line().1.contains("Saia e entre de novo"));
        assert!(!m.view().fix_button);
        m.status = Status::Active;
        m.snipping_fixed = false;
        m.hotkey = Hotkey::parse("Insert").unwrap();
        assert_eq!(
            m.status_line().0,
            Tone::Ok,
            "Insert não sofre com a Ferramenta de Captura"
        );
        m.status = Status::Taken;
        assert_eq!(
            m.status_line().1,
            "Insert já é usado por outro programa. Escolha outro atalho."
        );
        m.status = Status::Manual;
        assert_eq!(
            m.status_line().1,
            "Crie um atalho global no seu ambiente: Insert →\u{a0}recortei\u{a0}--capture"
        );
    }

    #[test]
    fn mouse_click_needs_press_and_release_on_same_button() {
        let mut m = model();
        let v = m.view();
        let r = view::layout(&v).rect(Btn::Preset(1)).unwrap();
        let (x, y) = (r.cx(), r.cy());
        m.pointer_moved(x, y);
        assert_eq!(m.hover, Some(Btn::Preset(1)));
        m.press(x, y);
        assert_eq!(m.release(x + 500.0, y), None, "soltou fora");
        m.press(x, y);
        assert!(matches!(m.release(x, y), Some(Action::Apply(_))));
        assert_eq!(m.focus, Some(Btn::Preset(1)));
    }
}
