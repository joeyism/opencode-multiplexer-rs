use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn key_event_to_bytes(event: KeyEvent) -> Option<Vec<u8>> {
    if event.code == KeyCode::Char('4') && event.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match event.code {
        KeyCode::Char(ch) => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                ctrl_char(ch)
            } else {
                Some(ch.to_string().into_bytes())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        _ => None,
    }
}

fn ctrl_char(ch: char) -> Option<Vec<u8>> {
    let uppercase = ch.to_ascii_uppercase() as u8;
    if uppercase.is_ascii_uppercase() {
        Some(vec![uppercase - b'@'])
    } else {
        None
    }
}
