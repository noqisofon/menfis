use ropey::{Rope, RopeSlice};

/// Ropeベースのテキストバッファ。カーソル位置は文字(char)オフセットで管理する。
///
/// 改行は`\n`のみを用いる前提としている(`\r`は挿入しない)。これによりropeyの
/// 行分割とcosmic-textの行分割(`\r`/`\n`/`\r\n`/`\n\r`)が常に一致する。
pub struct Buffer {
    rope: Rope,
    cursor: usize,
    dirty: bool,
}

impl Buffer {
    pub fn new(initial_text: &str) -> Self {
        let rope = Rope::from_str(initial_text);
        let cursor = rope.len_chars();
        Self {
            rope,
            cursor,
            dirty: true,
        }
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// 前回の呼び出し以降にバッファが変更されていればtrueを返し、フラグを消費する。
    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    pub fn insert_char(&mut self, c: char) {
        self.rope.insert_char(self.cursor, c);
        self.cursor += 1;
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.cursor - 1;
        self.rope.remove(start..self.cursor);
        self.cursor = start;
        self.dirty = true;
    }

    pub fn delete_forward(&mut self) {
        if self.cursor >= self.rope.len_chars() {
            return;
        }
        self.rope.remove(self.cursor..self.cursor + 1);
        self.dirty = true;
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.dirty = true;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.rope.len_chars() {
            self.cursor += 1;
            self.dirty = true;
        }
    }

    pub fn move_up(&mut self) {
        let (line, col) = self.cursor_line_col();
        if line == 0 {
            return;
        }
        self.move_to_line_col(line - 1, col);
    }

    pub fn move_down(&mut self) {
        let (line, col) = self.cursor_line_col();
        if line + 1 >= self.rope.len_lines() {
            return;
        }
        self.move_to_line_col(line + 1, col);
    }

    fn move_to_line_col(&mut self, line: usize, col: usize) {
        let line_start = self.rope.line_to_char(line);
        let target_col = col.min(line_char_len_without_break(self.rope.line(line)));
        self.cursor = line_start + target_col;
        self.dirty = true;
    }

    /// カーソルの(行番号, 行内の文字オフセット)。
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let line = self.rope.char_to_line(self.cursor);
        let line_start = self.rope.line_to_char(line);
        (line, self.cursor - line_start)
    }

    /// カーソルの(行番号, 行内のバイトオフセット)。cosmic-text::Cursorの構築に使う。
    pub fn cursor_byte_col(&self) -> (usize, usize) {
        let (line, col_chars) = self.cursor_line_col();
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
        buffer.move_left();
        buffer.move_left();
        buffer.move_left();
        assert_eq!(buffer.cursor_line_col(), (0, 0));
        buffer.delete_forward();
        assert_eq!(buffer.text(), "bc");
        buffer.move_right();
        buffer.move_right();
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
        buffer.move_up();
        // 1行目は6文字あるので列2のまま
        assert_eq!(buffer.cursor_line_col(), (0, 2));
        buffer.move_left();
        buffer.move_left();
        assert_eq!(buffer.cursor_line_col(), (0, 0));
        buffer.move_down();
        // 2行目は2文字しかないので列0にクランプされない(0はそのまま収まる)
        assert_eq!(buffer.cursor_line_col(), (1, 0));
    }

    #[test]
    fn cursor_byte_col_accounts_for_multibyte_chars() {
        let mut buffer = Buffer::new("あ");
        // "あ"はUTF-8で3バイト、カーソルはその直後(文字オフセット1)にある
        assert_eq!(buffer.cursor_byte_col(), (0, 3));
        buffer.move_left();
        assert_eq!(buffer.cursor_byte_col(), (0, 0));
    }
}
