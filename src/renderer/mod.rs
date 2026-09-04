mod cursor;
mod selection;
mod text;

use std::sync::Arc;
use winit::window::Window;

use cursor::CursorLayer;
use selection::SelectionLayer;
use text::TextLayer;

/// wgpuの初期化状態一式と、背景クリア+選択範囲+テキスト+カーソル描画の最小パイプライン。
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    text: TextLayer,
    cursor: CursorLayer,
    selection: SelectionLayer,
    cursor_line: usize,
    cursor_byte_col: usize,
    selection_range: Option<((usize, usize), (usize, usize))>,
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        initial_text: &str,
        cursor_line: usize,
        cursor_byte_col: usize,
    ) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("サーフェスの作成に失敗しました");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("適合するGPUアダプタが見つかりませんでした");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("menfis device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .expect("デバイスの取得に失敗しました");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let text = TextLayer::new(
            &device,
            &queue,
            config.format,
            config.width as f32,
            config.height as f32,
            initial_text,
        );

        let mut cursor = CursorLayer::new(&device, config.format);
        if let Some((x, top, height)) = text.cursor_pixel_position(cursor_line, cursor_byte_col) {
            cursor.set_position(x, top, height);
        }

        let selection = SelectionLayer::new(&device, config.format);

        Self {
            surface,
            device,
            queue,
            config,
            size,
            text,
            cursor,
            selection,
            cursor_line,
            cursor_byte_col,
            selection_range: None,
        }
    }

    /// バッファのテキスト内容が変わったときに呼び出す(再シェイピングを伴う)。
    pub fn set_text(&mut self, text: &str) {
        self.text.set_text(text);
    }

    /// カーソル位置・選択範囲が変わったときに呼び出す。テキスト内容が不変なら
    /// 再シェイピングは行わず、カーソル・選択範囲の描画位置だけを更新する。
    pub fn update_cursor(
        &mut self,
        cursor_line: usize,
        cursor_byte_col: usize,
        selection_range: Option<((usize, usize), (usize, usize))>,
    ) {
        self.cursor_line = cursor_line;
        self.cursor_byte_col = cursor_byte_col;
        self.selection_range = selection_range;
        self.text
            .ensure_line_visible(cursor_line, self.config.height as f32);
        self.sync_cursor_position();
    }

    /// マウスホイールによる相対スクロール(行数)。
    pub fn scroll_by_lines(&mut self, delta: isize) {
        self.text.scroll_by_lines(delta);
        self.sync_cursor_position();
    }

    /// ウィンドウ座標から(行, 行内バイトオフセット)を求める。マウス操作に使う。
    pub fn hit_test(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        self.text.hit_test(x, y)
    }

    fn sync_cursor_position(&mut self) {
        if let Some((x, top, height)) = self
            .text
            .cursor_pixel_position(self.cursor_line, self.cursor_byte_col)
        {
            self.cursor.set_position(x, top, height);
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.text.resize(new_size.width as f32, new_size.height as f32);
        self.sync_cursor_position();
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.text
            .prepare(&self.device, &self.queue, self.config.width, self.config.height);
        self.cursor
            .prepare(&self.queue, self.config.width as f32, self.config.height as f32);

        let selection_spans = match self.selection_range {
            Some((start, end)) => self.text.selection_spans(start, end),
            None => Vec::new(),
        };
        self.selection.prepare(
            &self.device,
            &self.queue,
            self.config.width as f32,
            self.config.height as f32,
            &selection_spans,
        );

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("menfis render encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menfis clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.09,
                            g: 0.09,
                            b: 0.11,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            self.selection.render(&mut render_pass);
            self.text.render(&mut render_pass);
            self.cursor.render(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.text.trim();

        Ok(())
    }
}
