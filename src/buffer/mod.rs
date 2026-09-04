mod history;

use ropey::{Rope, RopeSlice};

use history::{EditKind, History};

/// Ropeベースのテキストバッファ。カーソルと選択範囲は文字オフセットで管理する。
///
/// 改行は`\n`のみを用いる前提としている(`\r`は挿入しない)。これによりropeyの
/// 行分割とcosmic-textの行分割(`\r`/`\n`/`\r\n`/`\n\r`)が常に一致する。
pub struct Buffer {
    rope: Rope,
    cursor: usize,
    selection_anchor: Option<usize>,
    history: History,
    /// テキスト内容そのものが変わった(再シェイピングが必要)
    text_dirty: bool,
    /// カーソル・選択範囲の表示位置が変わった(再シェイピングは不要)
    cursor_dirty: bool,
}

impl Buffer {
    pub fn new(initial_text: &str) -> Self {
        let rope = Rope::from_str(initial_text);
        let cursor = rope.len_chars();
        Self {
            rope,
            cursor,
            selection_anchor: None,
            history: History::new(),
            text_dirty: true,
            cursor_dirty: true,
        }
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// 前回の呼び出し以降にテキスト内容が変更されていればtrueを返し、フラグを消費する。
    pub fn take_text_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.text_dirty, false)
    }

    /// 前回の呼び出し以降にカーソル・選択範囲の表示位置が変わっていればtrueを返し、
    /// フラグを消費する。
    pub fn take_cursor_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.cursor_dirty, false)
    }

    fn mark_cursor_dirty(&mut self) {
        self.cursor_dirty = true;
    }

    fn mark_text_dirty(&mut self) {
        self.text_dirty = true;
        self.cursor_dirty = true;
    }

    // --- 編集 ---------------------------------------------------------

    pub fn insert_char(&mut self, c: char) {
        let kind = if c == '\n' { EditKind::Other } else { EditKind::Insert };
        self.history.checkpoint(kind, &self.rope, self.cursor, self.selection_anchor);
        self.delete_selection_raw();
        self.rope.insert_char(self.cursor, c);
        self.cursor += 1;
        self.history.note_cursor_after_edit(self.cursor);
        self.mark_text_dirty();
    }

    /// クリップボードからの貼り付けなど、複数文字をまとめて挿入する。
    pub fn paste(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.history.checkpoint(EditKind::Other, &self.rope, self.cursor, self.selection_anchor);
        self.delete_selection_raw();
        self.rope.insert(self.cursor, text);
        self.cursor += text.chars().count();
        self.history.note_cursor_after_edit(self.cursor);
        self.mark_text_dirty();
    }

    pub fn backspace(&mut self) {
        if self.selection_char_range().is_none() && self.cursor == 0 {
            return;
        }
        self.history.checkpoint(EditKind::Delete, &self.rope, self.cursor, self.selection_anchor);
        if self.delete_selection_raw().is_none() {
            let start = self.cursor - 1;
            self.rope.remove(start..self.cursor);
            self.cursor = start;
        }
        self.history.note_cursor_after_edit(self.cursor);
        self.mark_text_dirty();
    }

    pub fn delete_forward(&mut self) {
        if self.selection_char_range().is_none() && self.cursor >= self.rope.len_chars() {
            return;
        }
        self.history.checkpoint(EditKind::Delete, &self.rope, self.cursor, self.selection_anchor);
        if self.delete_selection_raw().is_none() {
            self.rope.remove(self.cursor..self.cursor + 1);
        }
        self.history.note_cursor_after_edit(self.cursor);
        self.mark_text_dirty();
    }

    pub fn undo(&mut self) {
        if let Some((rope, cursor, selection_anchor)) =
            self.history.undo(&self.rope, self.cursor, self.selection_anchor)
        {
            self.rope = rope;
            self.cursor = cursor;
            self.selection_anchor = selection_anchor;
            self.mark_text_dirty();
        }
    }

    pub fn redo(&mut self) {
        if let Some((rope, cursor, selection_anchor)) =
            self.history.redo(&self.rope, self.cursor, self.selection_anchor)
        {
            self.rope = rope;
            self.cursor = cursor;
            self.selection_anchor = selection_anchor;
            self.mark_text_dirty();
        }
    }

    // --- 選択範囲・クリップボード ---------------------------------------

    fn begin_or_clear_selection(&mut self, extend: bool) {
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
    }

    fn selection_char_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(if anchor < self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        })
    }

    /// 選択範囲があれば削除し、削除した文字列を返す。カーソルは削除位置に置かれる。
    fn delete_selection_raw(&mut self) -> Option<String> {
        let (start, end) = self.selection_char_range()?;
        let text = self.rope.slice(start..end).to_string();
        self.rope.remove(start..end);
        self.cursor = start;
        self.selection_anchor = None;
        Some(text)
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.rope.len_chars();
        self.mark_cursor_dirty();
    }

    pub fn copy(&self) -> Option<String> {
        let (start, end) = self.selection_char_range()?;
        Some(self.rope.slice(start..end).to_string())
    }

    pub fn cut(&mut self) -> Option<String> {
        self.selection_char_range()?;
        self.history.checkpoint(EditKind::Other, &self.rope, self.cursor, self.selection_anchor);
        let text = self.delete_selection_raw();
        self.mark_text_dirty();
        text
    }

    /// 選択範囲を(開始(行,行内バイトオフセット), 終了(同))で返す。選択がなければNone。
    pub fn selection_byte_range(&self) -> Option<((usize, usize), (usize, usize))> {
        let (start, end) = self.selection_char_range()?;
        Some((self.char_to_line_byte_col(start), self.char_to_line_byte_col(end)))
    }

    // --- カーソル移動 ---------------------------------------------------

    pub fn move_left(&mut self, extend: bool) {
        self.begin_or_clear_selection(extend);
        if self.cursor > 0 {
            self.cursor -= 1;
        }
        self.mark_cursor_dirty();
    }

    pub fn move_right(&mut self, extend: bool) {
        self.begin_or_clear_selection(extend);
        if self.cursor < self.rope.len_chars() {
            self.cursor += 1;
        }
        self.mark_cursor_dirty();
    }

    pub fn move_up(&mut self, extend: bool) {
        self.begin_or_clear_selection(extend);
        let (line, col) = self.cursor_line_col();
        if line > 0 {
            self.move_to_line_col(line - 1, col);
        }
        self.mark_cursor_dirty();
    }

    pub fn move_down(&mut self, extend: bool) {
        self.begin_or_clear_selection(extend);
        let (line, col) = self.cursor_line_col();
        if line + 1 < self.rope.len_lines() {
            self.move_to_line_col(line + 1, col);
        }
        self.mark_cursor_dirty();
    }

    /// マウスクリック/ドラッグなど、画面上の(行, 行内バイトオフセット)からカーソルを設定する。
    pub fn set_cursor_from_line_byte_col(&mut self, line: usize, byte_col: usize, extend: bool) {
        self.begin_or_clear_selection(extend);
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        let line_start = self.rope.line_to_char(line);
        let line_slice = self.rope.line(line);
        let byte_col = byte_col.min(line_slice.len_bytes());
        let char_col = line_slice.byte_to_char(byte_col);
        self.cursor = line_start + char_col;
        self.mark_cursor_dirty();
    }

    fn move_to_line_col(&mut self, line: usize, col: usize) {
        let line_start = self.rope.line_to_char(line);
        let target_col = col.min(line_char_len_without_break(self.rope.line(line)));
        self.cursor = line_start + target_col;
    }

    /// カーソルの(行番号, 行内の文字オフセット)。
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let line = self.rope.char_to_line(self.cursor);
        let line_start = self.rope.line_to_char(line);
        (line, self.cursor - line_start)
    }

    /// カーソルの(行番号, 行内のバイトオフセット)。cosmic-text::Cursorの構築に使う。
    pub fn cursor_byte_col(&self) -> (usize, usize) {
        self.char_to_line_byte_col(self.cursor)
    }

    fn char_to_line_byte_col(&self, char_idx: usize) -> (usize, usize) {
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let col_chars = char_idx - line_start;
        let line_slice = self.rope.line(line);
        let byte_col = line_slice.slice(0..col_chars).len_bytes();
        (line, byte_col)
    }
}

fn line_char_len_without_break(line: RopeSlice) -> usize {
    let len = line.len_chars();
    if len == 0 {
        return 0;
    }
    if line.char(len - 1) == '\n' {
        len - 1
    } else {
        len
    }
}

#[cfg(test)]
mod tests {
    use super::Buffer;

    #[test]
    fn insert_and_backspace_roundtrip() {
        let mut buffer = Buffer::new("");
        buffer.insert_char('あ');
        buffer.insert_char('b');
        assert_eq!(buffer.text(), "あb");
        buffer.backspace();
        assert_eq!(buffer.text(), "あ");
        buffer.backspace();
        assert_eq!(buffer.text(), "");
        // 空バッファでのbackspaceは何もしない
        buffer.backspace();
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn delete_forward_and_move() {
        let mut buffer = Buffer::new("abc");
        buffer.move_left(false);
        buffer.move_left(false);
        buffer.move_left(false);
        assert_eq!(buffer.cursor_line_col(), (0, 0));
        buffer.delete_forward();
        assert_eq!(buffer.text(), "bc");
        buffer.move_right(false);
        buffer.move_right(false);
        buffer.delete_forward(); // 末尾では何もしない
        assert_eq!(buffer.text(), "bc");
    }

    #[test]
    fn enter_creates_new_line_and_tracks_line_col() {
        let mut buffer = Buffer::new("ab");
        buffer.insert_char('\n');
        buffer.insert_char('c');
        assert_eq!(buffer.text(), "ab\nc");
        assert_eq!(buffer.cursor_line_col(), (1, 1));
    }

    #[test]
    fn move_up_down_clamps_column_to_shorter_line() {
        let mut buffer = Buffer::new("abcdef\nxy");
        // カーソルは末尾(2行目の"xy"の後ろ、列2)
        assert_eq!(buffer.cursor_line_col(), (1, 2));
        buffer.move_up(false);
        // 1行目は6文字あるので列2のまま
        assert_eq!(buffer.cursor_line_col(), (0, 2));
        buffer.move_left(false);
        buffer.move_left(false);
        assert_eq!(buffer.cursor_line_col(), (0, 0));
        buffer.move_down(false);
        // 2行目は2文字しかないので列0にクランプされない(0はそのまま収まる)
        assert_eq!(buffer.cursor_line_col(), (1, 0));
    }

    #[test]
    fn cursor_byte_col_accounts_for_multibyte_chars() {
        let mut buffer = Buffer::new("あ");
        // "あ"はUTF-8で3バイト、カーソルはその直後(文字オフセット1)にある
        assert_eq!(buffer.cursor_byte_col(), (0, 3));
        buffer.move_left(false);
        assert_eq!(buffer.cursor_byte_col(), (0, 0));
    }

    #[test]
    fn shift_arrow_selects_and_typing_replaces_selection() {
        let mut buffer = Buffer::new("hello world");
        for _ in 0..5 {
            buffer.move_left(true);
        }
        assert_eq!(buffer.copy().as_deref(), Some("world"));
        buffer.insert_char('!');
        assert_eq!(buffer.text(), "hello !");
    }

    #[test]
    fn cut_removes_selection_and_returns_text() {
        let mut buffer = Buffer::new("hello world");
        buffer.select_all();
        let cut = buffer.cut();
        assert_eq!(cut.as_deref(), Some("hello world"));
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn undo_redo_restores_state() {
        let mut buffer = Buffer::new("");
        buffer.insert_char('a');
        buffer.insert_char('b');
        buffer.insert_char('c');
        buffer.backspace();
        assert_eq!(buffer.text(), "ab");
        buffer.undo(); // まとめられたBackspaceを取り消す
        assert_eq!(buffer.text(), "abc");
        buffer.undo(); // まとめられたInsertを取り消す
        assert_eq!(buffer.text(), "");
        buffer.redo();
        assert_eq!(buffer.text(), "abc");
        buffer.redo();
        assert_eq!(buffer.text(), "ab");
    }

    #[test]
    fn selection_replace_does_not_coalesce_with_earlier_typing() {
        let mut buffer = Buffer::new("");
        buffer.insert_char('a');
        buffer.insert_char('b');
        buffer.insert_char('c'); // "abc"を1つのUndo単位にまとめる
        buffer.move_left(true);
        buffer.move_left(true);
        buffer.move_left(true); // "abc"全体を選択
        buffer.insert_char('x'); // 選択範囲の置き換えは新しいUndo単位になるべき
        assert_eq!(buffer.text(), "x");
        buffer.undo();
        assert_eq!(buffer.text(), "abc");
        buffer.undo();
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn cursor_jump_breaks_insert_coalescing() {
        let mut buffer = Buffer::new("");
        buffer.insert_char('a');
        buffer.insert_char('b');
        buffer.move_left(false);
        buffer.insert_char('X'); // カーソル移動後の挿入は新しいUndo単位になるべき
        assert_eq!(buffer.text(), "aXb");
        buffer.undo();
        assert_eq!(buffer.text(), "ab");
        buffer.undo();
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn set_cursor_from_line_byte_col_places_cursor() {
        let mut buffer = Buffer::new("ab\ncdef");
        buffer.set_cursor_from_line_byte_col(1, 2, false);
        assert_eq!(buffer.cursor_line_col(), (1, 2));
        buffer.set_cursor_from_line_byte_col(0, 0, true);
        assert_eq!(buffer.copy().as_deref(), Some("ab\ncd"));
    }
}
