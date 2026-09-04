use ropey::Rope;

/// 直前の編集の種類。同種の編集(連続した文字入力やBackspaceの連打)は
/// 1つのUndo単位にまとめる。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Other,
}

struct Snapshot {
    rope: Rope,
    cursor: usize,
    selection_anchor: Option<usize>,
}

/// Undo/Redoのためのスナップショット履歴。
///
/// `Rope`のcloneは内部で構造共有されるため軽量であり、編集のたびに
/// バッファ全体のスナップショットを取っても実用上問題にならない。
#[derive(Default)]
pub struct History {
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    last_kind: Option<EditKind>,
    /// 直前の(まとめられた)編集が完了した直後のカーソル位置。
    /// 次の編集の開始カーソルがこれと一致する場合のみ連続編集とみなす。
    last_cursor_after: Option<usize>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// 編集を行う直前に呼び出す。Insert/Deleteが同じカーソル位置から連続する場合は
    /// 同じUndo単位としてまとめ、それ以外の編集は必ず新しいUndo単位を作る。
    /// カーソルが不連続(矢印キーやマウスでの移動、選択範囲の変更など)な場合は
    /// 種類が同じでも新しい単位として扱う。
    pub fn checkpoint(&mut self, kind: EditKind, rope: &Rope, cursor: usize, selection_anchor: Option<usize>) {
        let coalesce = matches!(kind, EditKind::Insert | EditKind::Delete)
            && self.last_kind == Some(kind)
            && self.last_cursor_after == Some(cursor);
        if !coalesce {
            self.undo_stack.push(Snapshot {
                rope: rope.clone(),
                cursor,
                selection_anchor,
            });
            self.redo_stack.clear();
        }
        self.last_kind = Some(kind);
    }

    /// 編集が完了した直後に呼び出し、次の連続編集判定に使うカーソル位置を記録する。
    pub fn note_cursor_after_edit(&mut self, cursor: usize) {
        self.last_cursor_after = Some(cursor);
    }

    pub fn undo(
        &mut self,
        rope: &Rope,
        cursor: usize,
        selection_anchor: Option<usize>,
    ) -> Option<(Rope, usize, Option<usize>)> {
        let snapshot = self.undo_stack.pop()?;
        self.redo_stack.push(Snapshot {
            rope: rope.clone(),
            cursor,
            selection_anchor,
        });
        self.last_kind = None;
        self.last_cursor_after = None;
        Some((snapshot.rope, snapshot.cursor, snapshot.selection_anchor))
    }

    pub fn redo(
        &mut self,
        rope: &Rope,
        cursor: usize,
        selection_anchor: Option<usize>,
    ) -> Option<(Rope, usize, Option<usize>)> {
        let snapshot = self.redo_stack.pop()?;
        self.undo_stack.push(Snapshot {
            rope: rope.clone(),
            cursor,
            selection_anchor,
        });
        self.last_kind = None;
        self.last_cursor_after = None;
        Some((snapshot.rope, snapshot.cursor, snapshot.selection_anchor))
    }
}
