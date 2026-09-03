mod app;
mod renderer;

use winit::event_loop::{ControlFlow, EventLoop};

use app::App;

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("イベントループの作成に失敗しました");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app).expect("イベントループの実行に失敗しました");
}
