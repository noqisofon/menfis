use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::buffer::Buffer;

/// キーイベントをテキストバッファへの編集・カーソル移動に変換する。
pub fn handle_key_event(buffer: &mut Buffer, event: &KeyEvent) {
    if event.state != ElementState::Pressed {
        return;
    }

    match &event.logical_key {
        Key::Named(NamedKey::Backspace) => buffer.backspace(),
        Key::Named(NamedKey::Delete) => buffer.delete_forward(),
        Key::Named(NamedKey::Enter) => buffer.insert_char('\n'),
        Key::Named(NamedKey::ArrowLeft) => buffer.move_left(),
        Key::Named(NamedKey::ArrowRight) => buffer.move_right(),
        Key::Named(NamedKey::ArrowUp) => buffer.move_up(),
        Key::Named(NamedKey::ArrowDown) => buffer.move_down(),
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
