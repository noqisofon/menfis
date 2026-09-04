use glyphon::{
    Attrs, Buffer as GlyphBuffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics,
    Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer as GlyphonTextRenderer, Viewport,
};

/// テキスト描画領域の左上パディング。カーソル位置計算でも同じ値を使う。
pub const TEXT_ORIGIN: (f32, f32) = (16.0, 16.0);

/// glyphon(cosmic-text)によるテキスト描画とグリフキャッシュの管理。
pub struct TextLayer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: GlyphonTextRenderer,
    buffer: GlyphBuffer,
}

impl TextLayer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: f32,
        height: f32,
        initial_text: &str,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer =
            GlyphonTextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let mut buffer = GlyphBuffer::new(&mut font_system, Metrics::new(18.0, 26.0));
        buffer.set_size(&mut font_system, Some(width), Some(height));
        buffer.set_text(
            &mut font_system,
            initial_text,
            Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut font_system, false);

        Self {
            font_system,
            swash_cache,
            viewport,
            atlas,
            renderer,
            buffer,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.buffer
            .set_size(&mut self.font_system, Some(width), Some(height));
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn set_text(&mut self, text: &str) {
        self.buffer.set_text(
            &mut self.font_system,
            text,
            Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// 指定した(行番号, 行内バイトオフセット)に対応するカーソルの画面上の位置を返す。
    /// `(x, top_y, line_height)`。対象行が現在のビューポート外などで見つからない場合は`None`。
    ///
    /// 合字(リガチャ)などでバイト境界がグリフ境界と一致しないことがあるため、
    /// 対象バイトオフセットを含むグリフをクラスタ内で按分して座標を求める。
    pub fn cursor_pixel_position(&self, line: usize, byte_col: usize) -> Option<(f32, f32, f32)> {
        for run in self.buffer.layout_runs() {
            if run.line_i != line {
                continue;
            }

            let mut x = 0.0;
            for glyph in run.glyphs {
                if byte_col < glyph.end {
                    let span = (glyph.end - glyph.start).max(1) as f32;
                    let ratio = (byte_col.saturating_sub(glyph.start)) as f32 / span;
                    x = glyph.x + glyph.w * ratio;
                    return Some((
                        TEXT_ORIGIN.0 + x,
                        TEXT_ORIGIN.1 + run.line_top,
                        run.line_height,
                    ));
                }
                x = glyph.x + glyph.w;
            }

            return Some((
                TEXT_ORIGIN.0 + x,
                TEXT_ORIGIN.1 + run.line_top,
                run.line_height,
            ));
        }
        None
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });

        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                [TextArea {
                    buffer: &self.buffer,
                    left: TEXT_ORIGIN.0,
                    top: TEXT_ORIGIN.1,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: width as i32,
                        bottom: height as i32,
                    },
                    default_color: GlyphonColor::rgb(230, 230, 235),
                    custom_glyphs: &[],
                }],
                &mut self.swash_cache,
            )
            .expect("テキストの準備に失敗しました");
    }

    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.renderer
            .render(&self.atlas, &self.viewport, pass)
            .expect("テキストの描画に失敗しました");
    }

    /// グリフキャッシュのうち直近使われなかったものを解放する。
    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}
