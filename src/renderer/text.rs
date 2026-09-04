use glyphon::cosmic_text::Scroll;
use glyphon::{
    Attrs, Buffer as GlyphBuffer, Cache, Color as GlyphonColor, Cursor, Family, FontSystem,
    Metrics, Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer as GlyphonTextRenderer, Viewport,
};

/// テキスト描画領域の左上パディング。カーソル位置計算でも同じ値を使う。
pub const TEXT_ORIGIN: (f32, f32) = (16.0, 16.0);

const FONT_SIZE: f32 = 18.0;
const LINE_HEIGHT: f32 = 26.0;

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

        let mut buffer = GlyphBuffer::new(&mut font_system, Metrics::new(FONT_SIZE, LINE_HEIGHT));
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

    /// 選択範囲(開始/終了の(行, 行内バイトオフセット))をハイライト矩形の一覧に変換する。
    /// 戻り値は`(x, top_y, width, height)`(いずれも画面座標)。
    pub fn selection_spans(
        &self,
        start: (usize, usize),
        end: (usize, usize),
    ) -> Vec<(f32, f32, f32, f32)> {
        let cursor_start = Cursor::new(start.0, start.1);
        let cursor_end = Cursor::new(end.0, end.1);
        let mut spans = Vec::new();
        for run in self.buffer.layout_runs() {
            if let Some((x, width)) = run.highlight(cursor_start, cursor_end) {
                spans.push((
                    TEXT_ORIGIN.0 + x,
                    TEXT_ORIGIN.1 + run.line_top,
                    width,
                    run.line_height,
                ));
            }
        }
        spans
    }

    /// 画面座標(テキスト描画領域基準ではなくウィンドウ全体基準)から
    /// 最も近い(行, 行内バイトオフセット)を求める。マウスクリック/ドラッグに使う。
    pub fn hit_test(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        self.buffer
            .hit(x - TEXT_ORIGIN.0, y - TEXT_ORIGIN.1)
            .map(|cursor| (cursor.line, cursor.index))
    }

    /// マウスホイールなどによる相対スクロール。
    pub fn scroll_by_lines(&mut self, delta: isize) {
        let current = self.buffer.scroll().line as isize;
        let max_line = self.buffer.lines.len().saturating_sub(1) as isize;
        let new_line = (current + delta).clamp(0, max_line) as usize;
        self.buffer.set_scroll(Scroll::new(new_line, 0.0, 0.0));
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// 指定した論理行が現在のビューポート内に収まるようスクロール位置を調整する。
    pub fn ensure_line_visible(&mut self, line: usize, viewport_height: f32) {
        let visible_lines = ((viewport_height - 2.0 * TEXT_ORIGIN.1) / LINE_HEIGHT)
            .floor()
            .max(1.0) as usize;
        let scroll_line = self.buffer.scroll().line;
        let new_scroll_line = if line < scroll_line {
            line
        } else if line >= scroll_line + visible_lines {
            line + 1 - visible_lines
        } else {
            scroll_line
        };
        if new_scroll_line != scroll_line {
            self.buffer.set_scroll(Scroll::new(new_scroll_line, 0.0, 0.0));
            self.buffer.shape_until_scroll(&mut self.font_system, false);
        }
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
