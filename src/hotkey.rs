//! Atalho global de captura (chave `hotkey` do config.json).
//!
//! Forma canônica: modificadores na ordem Ctrl+Alt+Shift+Super seguidos da
//! tecla (`PrintScreen`, `Insert`, `Ctrl+Shift+S`, `Alt+PrintScreen`, `F9`…).
//! Converte para o Windows (`RegisterHotKey`), para aceleradores do GNOME e a
//! partir das teclas do winit (gravação na janela de Configurações).

use std::fmt;

use winit::keyboard::{Key as WKey, KeyCode, ModifiersState, NamedKey, PhysicalKey};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub sup: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        sup: false,
    };

    fn any(self) -> bool {
        self.ctrl || self.alt || self.shift || self.sup
    }

    pub fn from_winit(m: ModifiersState) -> Self {
        Mods {
            ctrl: m.control_key(),
            alt: m.alt_key(),
            shift: m.shift_key(),
            sup: m.super_key(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    PrintScreen,
    Insert,
    Pause,
    ScrollLock,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    /// F1–F24.
    F(u8),
    /// A–Z (sempre maiúscula).
    Letter(char),
    /// 0–9.
    Digit(u8),
}

/// Teclas com nome: (nome canônico, apelidos aceitos na leitura, keysym do GNOME, VK do Windows).
const NAMED: [(Key, &str, &[&str], &str, u32); 9] = [
    (
        Key::PrintScreen,
        "PrintScreen",
        &["print", "prtsc", "prtscn", "prtscr", "printscr"],
        "Print",
        0x2C,
    ),
    (Key::Insert, "Insert", &["ins"], "Insert", 0x2D),
    (Key::Pause, "Pause", &["break"], "Pause", 0x13),
    (
        Key::ScrollLock,
        "ScrollLock",
        &["scroll", "scroll_lock"],
        "Scroll_Lock",
        0x91,
    ),
    (Key::Home, "Home", &[], "Home", 0x24),
    (Key::End, "End", &[], "End", 0x23),
    (
        Key::PageUp,
        "PageUp",
        &["pgup", "page_up", "prior"],
        "Page_Up",
        0x21,
    ),
    (
        Key::PageDown,
        "PageDown",
        &["pgdn", "page_down", "next"],
        "Page_Down",
        0x22,
    ),
    (Key::Delete, "Delete", &["del"], "Delete", 0x2E),
];

// Modificadores do `RegisterHotKey` (valores de `MOD_*` do Win32).
#[cfg_attr(not(windows), allow(dead_code))]
pub const MOD_ALT: u32 = 0x0001;
#[cfg_attr(not(windows), allow(dead_code))]
pub const MOD_CONTROL: u32 = 0x0002;
#[cfg_attr(not(windows), allow(dead_code))]
pub const MOD_SHIFT: u32 = 0x0004;
#[cfg_attr(not(windows), allow(dead_code))]
pub const MOD_WIN: u32 = 0x0008;
#[cfg_attr(not(windows), allow(dead_code))]
pub const MOD_NOREPEAT: u32 = 0x4000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub mods: Mods,
    pub key: Key,
}

pub const DEFAULT: Hotkey = Hotkey {
    mods: Mods::NONE,
    key: Key::PrintScreen,
};

/// Atalhos sugeridos na janela de Configurações.
pub const PRESETS: [&str; 6] = [
    "PrintScreen",
    "Insert",
    "Ctrl+Shift+S",
    "Alt+PrintScreen",
    "Ctrl+PrintScreen",
    "Pause",
];

impl Key {
    fn named(
        self,
    ) -> Option<&'static (
        Key,
        &'static str,
        &'static [&'static str],
        &'static str,
        u32,
    )> {
        NAMED.iter().find(|n| n.0 == self)
    }

    /// Nome que aparece para o usuário (`PrintScreen`, `F9`, `S`).
    pub fn label(self) -> String {
        match self {
            Key::F(n) => format!("F{n}"),
            Key::Letter(c) => c.to_string(),
            Key::Digit(d) => d.to_string(),
            k => k.named().map_or("?", |n| n.1).into(),
        }
    }

    fn parse(s: &str) -> Option<Key> {
        let low = s.to_ascii_lowercase();
        if let Some(n) = NAMED
            .iter()
            .find(|n| n.1.eq_ignore_ascii_case(&low) || n.2.contains(&low.as_str()))
        {
            return Some(n.0);
        }
        let mut chars = s.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if c.is_ascii_alphabetic() {
                return Some(Key::Letter(c.to_ascii_uppercase()));
            }
            if let Some(d) = c.to_digit(10) {
                return Some(Key::Digit(d as u8));
            }
        }
        let n: u8 = low.strip_prefix('f')?.parse().ok()?;
        (1..=24).contains(&n).then_some(Key::F(n))
    }

    /// Letras e números sozinhos atrapalhariam a digitação.
    fn needs_modifier(self) -> bool {
        matches!(self, Key::Letter(_) | Key::Digit(_))
    }
}

impl Hotkey {
    /// Lê a forma canônica (aceita minúsculas, espaços e apelidos como
    /// `Control`, `Win`, `Print`).
    pub fn parse(s: &str) -> Result<Hotkey, String> {
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let Some((key, mods)) = parts.split_last() else {
            return Err("atalho vazio".into());
        };
        let mut m = Mods::NONE;
        for p in mods {
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "ctl" => m.ctrl = true,
                "alt" => m.alt = true,
                "shift" => m.shift = true,
                "super" | "win" | "meta" | "cmd" => m.sup = true,
                "" => return Err(format!("atalho inválido: {s}")),
                other => return Err(format!("modificador desconhecido: {other}")),
            }
        }
        let key = Key::parse(key).ok_or_else(|| format!("tecla não suportada: {key}"))?;
        Hotkey { mods: m, key }.validated()
    }

    /// Aplica as regras (letras e números precisam de Ctrl, Alt ou Super).
    pub fn validated(self) -> Result<Hotkey, String> {
        let m = self.mods;
        if self.key.needs_modifier() && !(m.ctrl || m.alt || m.sup) {
            return Err("Letras e números precisam de Ctrl, Alt ou Super.".into());
        }
        Ok(self)
    }

    /// Valor de `hotkey` do config.json; padrão PrintScreen se ausente ou inválido.
    pub fn from_config(value: Option<&str>) -> Hotkey {
        value.and_then(|s| Hotkey::parse(s).ok()).unwrap_or(DEFAULT)
    }

    /// Nome para o usuário e forma salva no config (`Ctrl+Shift+S`).
    pub fn label(&self) -> String {
        self.to_string()
    }

    /// As partes do atalho, uma por tecla (`["Ctrl", "Shift", "S"]`).
    pub fn parts(&self) -> Vec<String> {
        let m = self.mods;
        let mut v: Vec<String> = [
            (m.ctrl, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.sup, "Super"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| n.to_string())
        .collect();
        v.push(self.key.label());
        v
    }

    pub fn uses_print_screen(&self) -> bool {
        self.key == Key::PrintScreen
    }

    /// `(modificadores, código virtual)` para o `RegisterHotKey`, já com `MOD_NOREPEAT`.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn windows(&self) -> (u32, u32) {
        let m = self.mods;
        let mut mods = MOD_NOREPEAT;
        for (on, bit) in [
            (m.alt, MOD_ALT),
            (m.ctrl, MOD_CONTROL),
            (m.shift, MOD_SHIFT),
            (m.sup, MOD_WIN),
        ] {
            if on {
                mods |= bit;
            }
        }
        let vk = match self.key {
            Key::F(n) => 0x70 + n as u32 - 1,
            Key::Letter(c) => c as u32,
            Key::Digit(d) => '0' as u32 + d as u32,
            k => k.named().map_or(0, |n| n.4),
        };
        (mods, vk)
    }

    /// Acelerador do GNOME (`Print`, `<Control><Shift>s`, `<Alt>Print`).
    #[cfg_attr(windows, allow(dead_code))]
    pub fn gnome(&self) -> String {
        let m = self.mods;
        let mut s = String::new();
        for (on, name) in [
            (m.ctrl, "<Control>"),
            (m.alt, "<Alt>"),
            (m.shift, "<Shift>"),
            (m.sup, "<Super>"),
        ] {
            if on {
                s.push_str(name);
            }
        }
        match self.key {
            Key::F(n) => s.push_str(&format!("F{n}")),
            Key::Letter(c) => s.push(c.to_ascii_lowercase()),
            Key::Digit(d) => s.push_str(&d.to_string()),
            k => s.push_str(k.named().map_or("", |n| n.3)),
        }
        s
    }

    /// Lê um acelerador do GNOME (`<Primary><Shift>s`, `<Super>Print`, `Insert`).
    /// `None` se usar algo que o Recortei não representa.
    #[cfg_attr(windows, allow(dead_code))]
    pub fn from_gnome(accel: &str) -> Option<Hotkey> {
        let mut rest = accel.trim();
        let mut m = Mods::NONE;
        while let Some(r) = rest.strip_prefix('<') {
            let (name, after) = r.split_once('>')?;
            match name.to_ascii_lowercase().as_str() {
                "control" | "ctrl" | "ctl" | "primary" => m.ctrl = true,
                "alt" | "mod1" => m.alt = true,
                "shift" => m.shift = true,
                "super" | "mod4" | "meta" => m.sup = true,
                _ => return None,
            }
            rest = after;
        }
        if rest.is_empty() {
            return None;
        }
        let key = NAMED
            .iter()
            .find(|n| n.3.eq_ignore_ascii_case(rest))
            .map(|n| n.0)
            .or_else(|| Key::parse(rest))?;
        Hotkey { mods: m, key }.validated().ok()
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.parts().join("+"))
    }
}

/// Situação do atalho global, mostrada na janela de Configurações.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Status {
    /// Aplicando ou ainda verificando.
    Pending,
    Active,
    /// Linux fora do GNOME: o usuário cria o atalho no próprio ambiente.
    #[cfg_attr(windows, allow(dead_code))]
    Manual,
    /// Windows: outro programa já registrou a combinação.
    Taken,
    Error(String),
}

/// Resultado de uma tecla apertada durante a gravação de um atalho.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Recorded {
    /// Só um modificador (ou tecla sem uso): continua esperando.
    Waiting,
    /// Esc sem modificadores.
    Cancel,
    Hotkey(Hotkey),
    /// Combinação recusada, com o motivo.
    Invalid(String),
}

/// Interpreta uma tecla do winit para a gravação. Usa a tecla lógica (o que
/// o layout produz) e cai para a física quando a lógica não ajuda (Ctrl+…).
pub fn record(physical: PhysicalKey, logical: &WKey, mods: ModifiersState) -> Recorded {
    let mods = Mods::from_winit(mods);
    let named = match logical {
        WKey::Named(n) => Some(*n),
        _ => None,
    };
    if named == Some(NamedKey::Escape) && !mods.any() {
        return Recorded::Cancel;
    }
    let from_named = named.and_then(|n| match n {
        NamedKey::PrintScreen => Some(Key::PrintScreen),
        NamedKey::Insert => Some(Key::Insert),
        NamedKey::Pause => Some(Key::Pause),
        NamedKey::ScrollLock => Some(Key::ScrollLock),
        NamedKey::Home => Some(Key::Home),
        NamedKey::End => Some(Key::End),
        NamedKey::PageUp => Some(Key::PageUp),
        NamedKey::PageDown => Some(Key::PageDown),
        NamedKey::Delete => Some(Key::Delete),
        _ => function_key(n),
    });
    let from_char = match logical {
        WKey::Character(s) => {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => Key::parse(&c.to_string()),
                _ => None,
            }
        }
        _ => None,
    };
    let key = from_named.or(from_char).or_else(|| physical_key(physical));
    match key {
        Some(key) => match (Hotkey { mods, key }).validated() {
            Ok(h) => Recorded::Hotkey(h),
            Err(e) => Recorded::Invalid(e),
        },
        None if is_modifier(named) => Recorded::Waiting,
        None => Recorded::Invalid("Essa tecla não pode ser usada como atalho.".into()),
    }
}

fn is_modifier(named: Option<NamedKey>) -> bool {
    matches!(
        named,
        Some(
            NamedKey::Control
                | NamedKey::Shift
                | NamedKey::Alt
                | NamedKey::AltGraph
                | NamedKey::Super
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::Fn
                | NamedKey::CapsLock
                | NamedKey::NumLock
        )
    )
}

fn function_key(n: NamedKey) -> Option<Key> {
    use NamedKey::*;
    const F: [NamedKey; 24] = [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
        F21, F22, F23, F24,
    ];
    F.iter().position(|&f| f == n).map(|i| Key::F(i as u8 + 1))
}

fn physical_key(p: PhysicalKey) -> Option<Key> {
    use KeyCode::*;
    let PhysicalKey::Code(code) = p else {
        return None;
    };
    const LETTERS: [KeyCode; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    ];
    const F: [KeyCode; 24] = [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
        F21, F22, F23, F24,
    ];
    if let Some(i) = LETTERS.iter().position(|&k| k == code) {
        return Some(Key::Letter((b'A' + i as u8) as char));
    }
    if let Some(i) = DIGITS.iter().position(|&k| k == code) {
        return Some(Key::Digit(i as u8));
    }
    if let Some(i) = F.iter().position(|&k| k == code) {
        return Some(Key::F(i as u8 + 1));
    }
    Some(match code {
        PrintScreen => Key::PrintScreen,
        Insert => Key::Insert,
        Pause => Key::Pause,
        ScrollLock => Key::ScrollLock,
        Home => Key::Home,
        End => Key::End,
        PageUp => Key::PageUp,
        PageDown => Key::PageDown,
        Delete => Key::Delete,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(s: &str) -> Hotkey {
        Hotkey::parse(s).unwrap()
    }

    #[test]
    fn parse_and_label_are_canonical() {
        assert_eq!(hk("PrintScreen"), DEFAULT);
        assert_eq!(hk("print").label(), "PrintScreen");
        assert_eq!(hk("shift+ctrl+s").label(), "Ctrl+Shift+S");
        assert_eq!(
            hk("Super + Alt + Shift + Control + F9").label(),
            "Ctrl+Alt+Shift+Super+F9"
        );
        assert_eq!(hk("Win+PrtSc").label(), "Super+PrintScreen");
        assert_eq!(hk("alt+printscreen").label(), "Alt+PrintScreen");
        assert_eq!(hk("Ctrl+1").label(), "Ctrl+1");
        assert_eq!(hk("F24").label(), "F24");
        assert_eq!(hk("pgdn").label(), "PageDown");
        for p in PRESETS {
            assert_eq!(hk(p).label(), p, "preset canônico");
            assert_eq!(Hotkey::parse(&hk(p).label()), Ok(hk(p)), "ida e volta");
        }
        assert_eq!(hk("Ctrl+Shift+S").parts(), ["Ctrl", "Shift", "S"]);
    }

    #[test]
    fn parse_rejects_invalid() {
        for bad in [
            "",
            "S",
            "5",
            "Shift+S",
            "Ctrl+",
            "Hyper+S",
            "F25",
            "F0",
            "Space",
            "Ctrl+Ctrl",
        ] {
            assert!(Hotkey::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(Hotkey::from_config(None), DEFAULT);
        assert_eq!(Hotkey::from_config(Some("lixo")), DEFAULT);
        assert_eq!(Hotkey::from_config(Some("Insert")), hk("Insert"));
    }

    #[test]
    fn windows_codes() {
        let nr = MOD_NOREPEAT;
        assert_eq!(hk("PrintScreen").windows(), (nr, 0x2C));
        assert_eq!(hk("Insert").windows(), (nr, 0x2D));
        assert_eq!(hk("Pause").windows(), (nr, 0x13));
        assert_eq!(
            hk("Ctrl+Shift+S").windows(),
            (nr | MOD_CONTROL | MOD_SHIFT, b'S' as u32)
        );
        assert_eq!(hk("Alt+PrintScreen").windows(), (nr | MOD_ALT, 0x2C));
        assert_eq!(hk("Super+F1").windows(), (nr | MOD_WIN, 0x70));
        assert_eq!(hk("F24").windows(), (nr, 0x87));
        assert_eq!(hk("Ctrl+0").windows(), (nr | MOD_CONTROL, 0x30));
        assert_eq!(hk("ScrollLock").windows().1, 0x91);
        assert_eq!(hk("Delete").windows().1, 0x2E);
    }

    #[test]
    fn gnome_accelerators() {
        assert_eq!(hk("PrintScreen").gnome(), "Print");
        assert_eq!(hk("Ctrl+Shift+S").gnome(), "<Control><Shift>s");
        assert_eq!(hk("Alt+PrintScreen").gnome(), "<Alt>Print");
        assert_eq!(hk("Insert").gnome(), "Insert");
        assert_eq!(hk("PageUp").gnome(), "Page_Up");
        assert_eq!(hk("Super+ScrollLock").gnome(), "<Super>Scroll_Lock");
        for p in PRESETS {
            assert_eq!(Hotkey::from_gnome(&hk(p).gnome()), Some(hk(p)));
        }
        assert_eq!(
            Hotkey::from_gnome("<Primary><Shift>S"),
            Some(hk("Ctrl+Shift+S"))
        );
        assert_eq!(
            Hotkey::from_gnome("<Ctrl>Print"),
            Some(hk("Ctrl+PrintScreen"))
        );
        assert_eq!(
            Hotkey::from_gnome("<Super>Print"),
            Some(hk("Super+PrintScreen"))
        );
        assert_eq!(Hotkey::from_gnome("<Super>space"), None);
        assert_eq!(Hotkey::from_gnome("s"), None, "letra sem modificador");
        assert_eq!(Hotkey::from_gnome(""), None);
        assert_eq!(Hotkey::from_gnome("<Hyper>Print"), None);
    }

    #[test]
    fn recording_from_winit_keys() {
        let none = ModifiersState::empty();
        let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let code = |c| PhysicalKey::Code(c);
        assert_eq!(
            record(
                code(KeyCode::PrintScreen),
                &WKey::Named(NamedKey::PrintScreen),
                none
            ),
            Recorded::Hotkey(DEFAULT)
        );
        assert_eq!(
            record(code(KeyCode::Escape), &WKey::Named(NamedKey::Escape), none),
            Recorded::Cancel
        );
        assert_eq!(
            record(
                code(KeyCode::ShiftLeft),
                &WKey::Named(NamedKey::Shift),
                ctrl_shift
            ),
            Recorded::Waiting
        );
        // Ctrl+Shift+S: a tecla lógica pode vir como "S" ou como controle; a física resolve
        assert_eq!(
            record(
                code(KeyCode::KeyS),
                &WKey::Character("S".into()),
                ctrl_shift
            ),
            Recorded::Hotkey(hk("Ctrl+Shift+S"))
        );
        assert_eq!(
            record(
                code(KeyCode::KeyS),
                &WKey::Character("\u{13}".into()),
                ctrl_shift
            ),
            Recorded::Hotkey(hk("Ctrl+Shift+S"))
        );
        // layout AZERTY: a tecla lógica manda
        assert_eq!(
            record(
                code(KeyCode::KeyQ),
                &WKey::Character("a".into()),
                ModifiersState::ALT
            ),
            Recorded::Hotkey(hk("Alt+A"))
        );
        assert_eq!(
            record(
                code(KeyCode::F9),
                &WKey::Named(NamedKey::F9),
                ModifiersState::SUPER
            ),
            Recorded::Hotkey(hk("Super+F9"))
        );
        assert!(matches!(
            record(code(KeyCode::KeyS), &WKey::Character("s".into()), none),
            Recorded::Invalid(_)
        ));
        assert!(matches!(
            record(code(KeyCode::Space), &WKey::Named(NamedKey::Space), none),
            Recorded::Invalid(_)
        ));
        assert_eq!(
            record(
                code(KeyCode::Escape),
                &WKey::Named(NamedKey::Escape),
                ModifiersState::CONTROL
            ),
            Recorded::Invalid("Essa tecla não pode ser usada como atalho.".into())
        );
    }
}
