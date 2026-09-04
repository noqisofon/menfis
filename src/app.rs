use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::buffer::Buffer;
use crate::input;
use crate::renderer::Renderer;

const INITIAL_TEXT: &str = "Hello, menfis — こんにちは、軽快に動くテキストエディタへようこそ。";

/// ウィンドウとレンダラーがそろって初めて存在する状態。
/// `resumed`が呼ばれるまでウィンドウは作れないため`Option`で保持する。
struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    buffer: Buffer,
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

        let buffer = Buffer::new(INITIAL_TEXT);
        let (cursor_line, cursor_byte_col) = buffer.cursor_byte_col();
        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            &buffer.text(),
            cursor_line,
            cursor_byte_col,
        ));

        self.state = Some(AppState {
            window,
            renderer,
            buffer,
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
            WindowEvent::KeyboardInput { event, .. } => {
                input::handle_key_event(&mut state.buffer, &event);
                if state.buffer.take_dirty() {
                    let (line, byte_col) = state.buffer.cursor_byte_col();
                    state
                        .renderer
                        .set_content(&state.buffer.text(), line, byte_col);
                    state.window.request_redraw();
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
