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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_q_maps_to_quit() {
        assert!(matches!(map_key(key(KeyCode::Char('q'))), AppAction::Quit));
    }

    #[test]
    fn test_esc_maps_to_quit() {
        assert!(matches!(map_key(key(KeyCode::Esc)), AppAction::Quit));
    }

    #[test]
    fn test_ctrl_c_maps_to_quit() {
        let ctrl_c = key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches!(map_key(ctrl_c), AppAction::Quit));
    }

    #[test]
    fn test_tab_maps_to_cycle_view() {
        assert!(matches!(map_key(key(KeyCode::Tab)), AppAction::CycleView));
    }

    #[test]
    fn test_p_maps_to_toggle_process_view() {
        assert!(matches!(map_key(key(KeyCode::Char('p'))), AppAction::ToggleProcessView));
    }

    #[test]
    fn test_d_maps_to_next_device() {
        assert!(matches!(map_key(key(KeyCode::Char('d'))), AppAction::NextDevice));
    }

    #[test]
    fn test_right_arrow_maps_to_next_device() {
        assert!(matches!(map_key(key(KeyCode::Right)), AppAction::NextDevice));
    }

    #[test]
    fn test_uppercase_d_maps_to_prev_device() {
        assert!(matches!(map_key(key(KeyCode::Char('D'))), AppAction::PrevDevice));
    }

    #[test]
    fn test_left_arrow_maps_to_prev_device() {
        assert!(matches!(map_key(key(KeyCode::Left)), AppAction::PrevDevice));
    }

    #[test]
    fn test_question_mark_maps_to_toggle_help() {
        assert!(matches!(map_key(key(KeyCode::Char('?'))), AppAction::ToggleHelp));
    }

    #[test]
    fn test_s_maps_to_cycle_sort_column() {
        assert!(matches!(map_key(key(KeyCode::Char('s'))), AppAction::CycleSortColumn));
    }

    #[test]
    fn test_r_maps_to_reverse_sort_direction() {
        assert!(matches!(map_key(key(KeyCode::Char('r'))), AppAction::ReverseSortDirection));
    }

    #[test]
    fn test_plus_maps_to_increase_refresh() {
        assert!(matches!(map_key(key(KeyCode::Char('+'))), AppAction::IncreaseRefresh));
    }

    #[test]
    fn test_equals_maps_to_increase_refresh() {
        assert!(matches!(map_key(key(KeyCode::Char('='))), AppAction::IncreaseRefresh));
    }

    #[test]
    fn test_minus_maps_to_decrease_refresh() {
        assert!(matches!(map_key(key(KeyCode::Char('-'))), AppAction::DecreaseRefresh));
    }

    #[test]
    fn test_f_maps_to_toggle_fast_mode() {
        assert!(matches!(map_key(key(KeyCode::Char('f'))), AppAction::ToggleFastMode));
    }

    #[test]
    fn test_unbound_key_maps_to_none() {
        assert!(matches!(map_key(key(KeyCode::Char('z'))), AppAction::None));
    }
}
