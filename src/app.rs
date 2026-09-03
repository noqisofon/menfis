use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::renderer::Renderer;

/// ウィンドウとレンダラーがそろって初めて存在する状態。
/// `resumed`が呼ばれるまでウィンドウは作れないため`Option`で保持する。
struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
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

        let renderer = pollster::block_on(Renderer::new(window.clone()));

        self.state = Some(AppState { window, renderer });
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
