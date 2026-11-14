//! Desktop input routing contracts.

use crate::SurfaceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Maustaste.
pub enum Button {
    /// Linke Taste.
    Left,
    /// Rechte Taste.
    Right,
    /// Mittlere Taste.
    Middle,
    /// Taste mit dieser Nummer.
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Taste oder Zeichen.
pub enum KeyCode {
    /// Escape.
    Escape,
    /// Enter.
    Enter,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Pfeil nach oben.
    ArrowUp,
    /// Pfeil nach unten.
    ArrowDown,
    /// Pfeil nach links.
    ArrowLeft,
    /// Pfeil nach rechts.
    ArrowRight,
    /// Zeichen als Codepunkt.
    Character(u32),
    /// Unbekannter Scancode.
    Unknown(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zusatztasten waehrend eines Tastenereignisses.
pub struct Modifiers {
    /// Shift ist gedrueckt.
    pub shift: bool,
    /// Strg ist gedrueckt.
    pub ctrl: bool,
    /// Alt ist gedrueckt.
    pub alt: bool,
    /// Meta oder Super ist gedrueckt.
    pub meta: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein Eingabeereignis.
pub enum InputEvent {
    /// Der Zeiger hat sich bewegt.
    PointerMove {
        /// Zeigerposition auf der X-Achse in Pixeln.
        x: i32,
        /// Zeigerposition auf der Y-Achse in Pixeln.
        y: i32,
    },
    /// Eine Maustaste wurde gedrueckt oder losgelassen.
    PointerButton {
        /// Die betroffene Maustaste.
        button: Button,
        /// True beim Druecken, false beim Loslassen.
        pressed: bool,
        /// Zeigerposition auf der X-Achse in Pixeln.
        x: i32,
        /// Zeigerposition auf der Y-Achse in Pixeln.
        y: i32,
    },
    /// Eine Taste wurde gedrueckt oder losgelassen.
    Key {
        /// Die betroffene Taste.
        code: KeyCode,
        /// True beim Druecken, false beim Loslassen.
        pressed: bool,
        /// Zusatztasten, die im Moment des Ereignisses gesetzt waren.
        modifiers: Modifiers,
    },
    /// Das Scrollrad wurde bewegt.
    Scroll {
        /// Verschiebung auf der X-Achse seit dem letzten Ereignis.
        delta_x: i32,
        /// Verschiebung auf der Y-Achse seit dem letzten Ereignis.
        delta_y: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ereignis mit dem Ziel, an das es geht.
pub struct RoutedEvent {
    /// Flaeche, die das Ereignis bekommt; None, wenn keine hat.
    pub target: Option<SurfaceId>,
    /// Das Ereignis selbst.
    pub event: InputEvent,
}

#[derive(Debug, Default)]
/// Haelt Zeiger- und Tastaturfokus und stellt Ereignisse zu.
pub struct InputRouter {
    pointer_focus: Option<SurfaceId>,
    keyboard_focus: Option<SurfaceId>,
}

impl InputRouter {
    /// Router ohne Fokus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Setzt den Fokus fuer Zeigerereignisse.
    pub fn set_pointer_focus(&mut self, target: Option<SurfaceId>) {
        self.pointer_focus = target;
    }

    /// Setzt den Fokus fuer Tastaturereignisse.
    pub fn set_keyboard_focus(&mut self, target: Option<SurfaceId>) {
        self.keyboard_focus = target;
    }

    /// Bestimmt das Ziel des Ereignisses anhand des Fokus.
    pub fn route(&self, event: InputEvent) -> RoutedEvent {
        let target = match event {
            InputEvent::PointerMove { .. }
            | InputEvent::PointerButton { .. }
            | InputEvent::Scroll { .. } => self.pointer_focus,
            InputEvent::Key { .. } => self.keyboard_focus,
        };
        RoutedEvent { target, event }
    }

    /// Aktueller Zeigerfokus.
    pub fn pointer_focus(&self) -> Option<SurfaceId> {
        self.pointer_focus
    }

    /// Aktueller Tastaturfokus.
    pub fn keyboard_focus(&self) -> Option<SurfaceId> {
        self.keyboard_focus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_and_pointer_have_independent_targets() {
        let mut r = InputRouter::new();
        r.set_pointer_focus(Some(SurfaceId(1)));
        r.set_keyboard_focus(Some(SurfaceId(2)));
        let pointer = r.route(InputEvent::PointerMove { x: 1, y: 2 });
        let key = r.route(InputEvent::Key {
            code: KeyCode::Enter,
            pressed: true,
            modifiers: Modifiers {
                shift: false,
                ctrl: false,
                alt: false,
                meta: false,
            },
        });
        assert_eq!(pointer.target, Some(SurfaceId(1)));
        assert_eq!(key.target, Some(SurfaceId(2)));
        assert_eq!(r.pointer_focus(), Some(SurfaceId(1)));
    }
}
