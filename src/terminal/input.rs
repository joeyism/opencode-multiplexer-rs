use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn key_event_to_bytes(event: KeyEvent) -> Option<Vec<u8>> {
    // Don't forward the focus toggle (Ctrl-\, reported as Ctrl-4)
    if event.code == KeyCode::Char('4') && event.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    // Don't forward the panel toggle (Ctrl-H)
    if event.code == KeyCode::Char('h') && event.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }

    let mods = event.modifiers;
    let has_alt = mods.contains(KeyModifiers::ALT);
    let has_ctrl = mods.contains(KeyModifiers::CONTROL);
    let has_shift = mods.contains(KeyModifiers::SHIFT);

    match event.code {
        KeyCode::Char(ch) => {
            if has_ctrl {
                let mut bytes = ctrl_char(ch)?;
                if has_alt {
                    bytes.insert(0, 0x1b);
                }
                Some(bytes)
            } else if has_alt {
                let mut bytes = vec![0x1b];
                bytes.extend(ch.to_string().as_bytes());
                Some(bytes)
            } else {
                Some(ch.to_string().into_bytes())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab if has_shift => Some(b"\x1b[Z".to_vec()),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Delete => modified_key(b"3", mods),
        KeyCode::Insert => modified_key(b"2", mods),
        KeyCode::Home => modified_special(b"H", mods),
        KeyCode::End => modified_special(b"F", mods),
        KeyCode::PageUp => modified_key(b"5", mods),
        KeyCode::PageDown => modified_key(b"6", mods),
        KeyCode::Up => modified_arrow(b'A', mods),
        KeyCode::Down => modified_arrow(b'B', mods),
        KeyCode::Right => modified_arrow(b'C', mods),
        KeyCode::Left => modified_arrow(b'D', mods),
        KeyCode::F(n) => f_key(n, mods),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        _ => None,
    }
}

fn ctrl_char(ch: char) -> Option<Vec<u8>> {
    let upper = ch.to_ascii_uppercase() as u8;
    if upper.is_ascii_uppercase() {
        Some(vec![upper - b'@'])
    } else {
        None
    }
}

fn modifier_param(mods: KeyModifiers) -> Option<u8> {
    let shift = mods.contains(KeyModifiers::SHIFT);
    let alt = mods.contains(KeyModifiers::ALT);
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let param = 1
        + if shift { 1 } else { 0 }
        + if alt { 2 } else { 0 }
        + if ctrl { 4 } else { 0 };
    if param > 1 { Some(param) } else { None }
}

fn modified_arrow(arrow: u8, mods: KeyModifiers) -> Option<Vec<u8>> {
    match modifier_param(mods) {
        Some(m) => Some(format!("\x1b[1;{}{}", m, arrow as char).into_bytes()),
        None => Some(vec![0x1b, b'[', arrow]),
    }
}

fn modified_special(code: &[u8], mods: KeyModifiers) -> Option<Vec<u8>> {
    match modifier_param(mods) {
        Some(m) => {
            let mut seq = format!("\x1b[1;{}", m).into_bytes();
            seq.extend_from_slice(code);
            Some(seq)
        }
        None => {
            let mut seq = vec![0x1b, b'['];
            seq.extend_from_slice(code);
            Some(seq)
        }
    }
}

fn modified_key(num: &[u8], mods: KeyModifiers) -> Option<Vec<u8>> {
    match modifier_param(mods) {
        Some(m) => {
            let mut seq = vec![0x1b, b'['];
            seq.extend_from_slice(num);
            seq.extend(format!(";{}~", m).as_bytes());
            Some(seq)
        }
        None => {
            let mut seq = vec![0x1b, b'['];
            seq.extend_from_slice(num);
            seq.push(b'~');
            Some(seq)
        }
    }
}

fn f_key(n: u8, mods: KeyModifiers) -> Option<Vec<u8>> {
    let code = match n {
        1 => return modified_ss3(b'P', mods),
        2 => return modified_ss3(b'Q', mods),
        3 => return modified_ss3(b'R', mods),
        4 => return modified_ss3(b'S', mods),
        5 => b"15",
        6 => b"17",
        7 => b"18",
        8 => b"19",
        9 => b"20",
        10 => b"21",
        11 => b"23",
        12 => b"24",
        _ => return None,
    };
    modified_key(code, mods)
}

fn modified_ss3(letter: u8, mods: KeyModifiers) -> Option<Vec<u8>> {
    match modifier_param(mods) {
        Some(m) => Some(format!("\x1b[1;{}{}", m, letter as char).into_bytes()),
        None => Some(vec![0x1b, b'O', letter]),
    }
}
