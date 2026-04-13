use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum AppAction {
    Quit,
    CycleView,
    ToggleProcessView,
    NextDevice,
    PrevDevice,
    ToggleHelp,
    CycleSortColumn,
    ReverseSortDirection,
    IncreaseRefresh,
    DecreaseRefresh,
    ToggleFastMode,
    None,
}

pub fn map_key(key: KeyEvent) -> AppAction {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return AppAction::Quit;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => AppAction::Quit,
        KeyCode::Tab => AppAction::CycleView,
        KeyCode::Char('p') => AppAction::ToggleProcessView,
        KeyCode::Char('d') | KeyCode::Right => AppAction::NextDevice,
        KeyCode::Char('D') | KeyCode::Left => AppAction::PrevDevice,
        KeyCode::Char('?') => AppAction::ToggleHelp,
        KeyCode::Char('s') => AppAction::CycleSortColumn,
        KeyCode::Char('r') => AppAction::ReverseSortDirection,
        KeyCode::Char('+') | KeyCode::Char('=') => AppAction::IncreaseRefresh,
        KeyCode::Char('-') => AppAction::DecreaseRefresh,
        KeyCode::Char('f') => AppAction::ToggleFastMode,
        _ => AppAction::None,
    }
}
