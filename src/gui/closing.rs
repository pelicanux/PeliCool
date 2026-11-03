#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum WindowState {
    #[default]
    Open,
    Asking,
    Hidden,
    Exiting,
}

#[derive(Clone, Copy)]
pub(super) enum Choice {
    Cancel,
    Minimize,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Effect {
    None,
    Hide,
    Minimize,
    Show,
    Exit,
}

impl WindowState {
    /// Return false only for a close already explicitly confirmed by the user.
    pub(super) fn request_close(&mut self) -> bool {
        if *self == Self::Exiting {
            return false;
        }
        *self = Self::Asking;
        true
    }

    pub(super) fn choose(&mut self, choice: Choice, tray: bool, uploading: bool) -> Effect {
        match choice {
            Choice::Cancel => {
                *self = Self::Open;
                Effect::None
            }
            Choice::Minimize if tray => {
                *self = Self::Hidden;
                Effect::Hide
            }
            Choice::Minimize => {
                *self = Self::Open;
                Effect::Minimize
            }
            Choice::Exit if uploading => {
                *self = Self::Asking;
                Effect::Show
            }
            Choice::Exit => {
                *self = Self::Exiting;
                Effect::Exit
            }
        }
    }

    pub(super) fn reconcile_tray(&mut self, available: bool) -> Effect {
        if *self == Self::Hidden && !available {
            self.restore()
        } else {
            Effect::None
        }
    }

    pub(super) fn restore(&mut self) -> Effect {
        *self = Self::Open;
        Effect::Show
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_waits_for_a_choice_and_cancel_keeps_the_app_open() {
        let mut state = WindowState::Open;
        assert!(state.request_close());
        assert_eq!(state, WindowState::Asking);
        assert!(state.request_close());
        assert_eq!(state.choose(Choice::Cancel, true, false), Effect::None);
        assert_eq!(state, WindowState::Open);
    }

    #[test]
    fn without_a_tray_the_window_is_minimized_and_never_hidden() {
        let mut state = WindowState::Asking;
        assert_eq!(
            state.choose(Choice::Minimize, false, false),
            Effect::Minimize
        );
        assert_eq!(state, WindowState::Open);
    }

    #[test]
    fn losing_the_tray_restores_a_hidden_window() {
        let mut state = WindowState::Asking;
        assert_eq!(state.choose(Choice::Minimize, true, false), Effect::Hide);
        assert_eq!(state.reconcile_tray(true), Effect::None);
        assert_eq!(state.reconcile_tray(false), Effect::Show);
        assert_eq!(state, WindowState::Open);
    }

    #[test]
    fn tray_activation_restores_the_window() {
        let mut state = WindowState::Hidden;
        assert_eq!(state.restore(), Effect::Show);
        assert_eq!(state, WindowState::Open);
    }

    #[test]
    fn upload_can_be_minimized_but_cannot_be_interrupted_by_exit() {
        let mut state = WindowState::Hidden;
        assert_eq!(state.choose(Choice::Exit, true, true), Effect::Show);
        assert_eq!(state, WindowState::Asking);
        assert_eq!(state.choose(Choice::Minimize, true, true), Effect::Hide);
        assert_eq!(
            state.choose(Choice::Minimize, false, true),
            Effect::Minimize
        );
    }

    #[test]
    fn confirmed_exit_does_not_open_another_prompt() {
        let mut state = WindowState::Asking;
        assert_eq!(state.choose(Choice::Exit, true, false), Effect::Exit);
        assert!(!state.request_close());
        assert_eq!(state, WindowState::Exiting);
    }
}
