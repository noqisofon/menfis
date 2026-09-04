use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::buffer::Buffer;
use crate::input;
use crate::renderer::Renderer;

const INITIAL_TEXT: &str = "Hello, menfis — こんにちは、軽快に動くテキストエディタへようこそ。";

/// マウスホイール1ノッチあたりにスクロールする行数。
const WHEEL_LINES_PER_NOTCH: isize = 3;
const PIXELS_PER_LINE: f64 = 26.0;

/// ウィンドウとレンダラーがそろって初めて存在する状態。
/// `resumed`が呼ばれるまでウィンドウは作れないため`Option`で保持する。
struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    buffer: Buffer,
    modifiers: ModifiersState,
    clipboard: Option<arboard::Clipboard>,
    cursor_pos: PhysicalPosition<f64>,
    mouse_selecting: bool,
}

impl AppState {
    /// バッファの変更をレンダラーへ反映する。テキスト自体が変わった場合のみ
    /// 再シェイピングを行い、カーソル移動だけの場合は位置更新のみ行う。
    fn sync_renderer(&mut self) {
        if self.buffer.take_text_dirty() {
            self.renderer.set_text(&self.buffer.text());
        }
        if self.buffer.take_cursor_dirty() {
            let (line, byte_col) = self.buffer.cursor_byte_col();
            let selection = self.buffer.selection_byte_range();
            self.renderer.update_cursor(line, byte_col, selection);
        }
        self.window.request_redraw();
    }
}

#[derive(Default)]
pub struct App {
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes().with_title("menfis");
        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("ウィンドウの作成に失敗しました"),
        );
        window.set_ime_allowed(true);

        let buffer = Buffer::new(INITIAL_TEXT);
        let (cursor_line, cursor_byte_col) = buffer.cursor_byte_col();
        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            &buffer.text(),
            cursor_line,
            cursor_byte_col,
        ));

        let clipboard = arboard::Clipboard::new()
            .inspect_err(|e| log::warn!("クリップボードを初期化できませんでした: {e}"))
            .ok();

        self.state = Some(AppState {
            window,
            renderer,
            buffer,
            modifiers: ModifiersState::empty(),
            clipboard,
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            mouse_selecting: false,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if window_id != state.window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => state.renderer.resize(new_size),
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                input::handle_key_event(&mut state.buffer, &event, state.modifiers, &mut state.clipboard);
                state.sync_renderer();
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor_pos = position;
                if state.mouse_selecting {
                    if let Some((line, byte_col)) = state
                        .renderer
                        .hit_test(position.x as f32, position.y as f32)
                    {
                        state.buffer.set_cursor_from_line_byte_col(line, byte_col, true);
                        state.sync_renderer();
                    }
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => {
                match button_state {
                    ElementState::Pressed => {
                        let pos = state.cursor_pos;
                        if let Some((line, byte_col)) =
                            state.renderer.hit_test(pos.x as f32, pos.y as f32)
                        {
                            let extend = state.modifiers.shift_key();
                            state
                                .buffer
                                .set_cursor_from_line_byte_col(line, byte_col, extend);
                            state.sync_renderer();
                        }
                        state.mouse_selecting = true;
                    }
                    ElementState::Released => {
                        state.mouse_selecting = false;
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        -(y.signum() as isize) * WHEEL_LINES_PER_NOTCH
                    }
                    MouseScrollDelta::PixelDelta(pos) => -(pos.y / PIXELS_PER_LINE).round() as isize,
                };
                if lines != 0 {
                    state.renderer.scroll_by_lines(lines);
                    state.window.request_redraw();
                }
            }
            WindowEvent::Ime(ime) => {
                use winit::event::Ime;
                match ime {
                    Ime::Commit(text) => {
                        for c in text.chars() {
                            state.buffer.insert_char(c);
                        }
                        state.sync_renderer();
                    }
                    Ime::Enabled | Ime::Preedit(..) | Ime::Disabled => {
                        // 変換候補(preedit)の下線表示は未対応。確定入力(Commit)のみ反映する。
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                match state.renderer.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        state.renderer.resize(state.window.inner_size());
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPUメモリ不足のため終了します");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("描画エラー: {e:?}"),
                }
                state.window.request_redraw();
            }
            _ => {}
        }
    }
}
