use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::buffer::Buffer;

/// キーイベントをテキストバッファへの編集・カーソル移動・クリップボード操作に変換する。
pub fn handle_key_event(
    buffer: &mut Buffer,
    event: &KeyEvent,
    modifiers: ModifiersState,
    clipboard: &mut Option<arboard::Clipboard>,
) {
    if event.state != ElementState::Pressed {
        return;
    }

    let shift = modifiers.shift_key();
    // macOSのCmdキーもショートカット用のCtrl相当として扱う。
    let ctrl = modifiers.control_key() || modifiers.super_key();

    if ctrl {
        if let Key::Character(s) = &event.logical_key {
            match s.as_str().to_ascii_lowercase().as_str() {
                "c" => {
                    if let Some(text) = buffer.copy() {
                        if let Some(cb) = clipboard {
                            let _ = cb.set_text(text);
                        }
                    }
                    return;
                }
                "x" => {
                    if let Some(text) = buffer.cut() {
                        if let Some(cb) = clipboard {
                            let _ = cb.set_text(text);
                        }
                    }
                    return;
                }
                "v" => {
                    let text = clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                    if let Some(text) = text {
                        buffer.paste(&text);
                    }
                    return;
                }
                "a" => {
                    buffer.select_all();
                    return;
                }
                "z" => {
                    if shift {
                        buffer.redo();
                    } else {
                        buffer.undo();
                    }
                    return;
                }
                "y" => {
                    buffer.redo();
                    return;
                }
                _ => {}
            }
        }
    }

    match &event.logical_key {
        Key::Named(NamedKey::Backspace) => buffer.backspace(),
        Key::Named(NamedKey::Delete) => buffer.delete_forward(),
        Key::Named(NamedKey::Enter) => buffer.insert_char('\n'),
        Key::Named(NamedKey::ArrowLeft) => buffer.move_left(shift),
        Key::Named(NamedKey::ArrowRight) => buffer.move_right(shift),
        Key::Named(NamedKey::ArrowUp) => buffer.move_up(shift),
        Key::Named(NamedKey::ArrowDown) => buffer.move_down(shift),
        _ => {
            if let Some(text) = &event.text {
                for c in text.chars() {
                    if !c.is_control() {
                        buffer.insert_char(c);
                    }
                }
            }
        }
    }
}
