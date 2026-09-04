use std::path::PathBuf;
use std::sync::mpsc::Sender;

use encoding_rs::Encoding;

/// バックグラウンドスレッドからメインスレッドへ通知するファイルI/Oの結果。
pub enum IoMessage {
    Opened {
        path: PathBuf,
        text: String,
        encoding: &'static Encoding,
    },
    OpenFailed {
        message: String,
    },
    /// 「名前を付けて保存」ダイアログで保存先が選ばれた。実際の書き込みは
    /// メインスレッド側(呼び出し時点のバッファ内容)で行う。
    SaveTargetChosen(PathBuf),
}

/// バイト列をテキストへデコードする。UTF-8として妥当であればUTF-8を採用し、
/// そうでなければ(BOM付きUTF-16やレガシーな日本語ファイルを想定して)
/// Shift-JISとして解釈する。
pub fn decode_bytes(bytes: &[u8]) -> (String, &'static Encoding) {
    let (text, _, had_errors) = encoding_rs::UTF_8.decode(bytes);
    if !had_errors {
        return (text.into_owned(), encoding_rs::UTF_8);
    }
    let (text, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    (text.into_owned(), encoding_rs::SHIFT_JIS)
}

/// テキストを指定した文字コードのバイト列へエンコードする。
pub fn encode_text(text: &str, encoding: &'static Encoding) -> Vec<u8> {
    let (bytes, _, _) = encoding.encode(text);
    bytes.into_owned()
}

/// 「開く」ダイアログを表示し、選択されたファイルを読み込む。
/// ダイアログの表示・ファイル読み込みはいずれも別スレッドで行うため、
/// 大きなファイルでもUIはブロックされない。
pub fn spawn_open_dialog(sender: Sender<IoMessage>) {
    std::thread::spawn(move || {
        let Some(path) = rfd::FileDialog::new().pick_file() else {
            return;
        };
        let message = match std::fs::read(&path) {
            Ok(bytes) => {
                let (text, encoding) = decode_bytes(&bytes);
                IoMessage::Opened { path, text, encoding }
            }
            Err(e) => IoMessage::OpenFailed {
                message: format!("{}: {e}", path.display()),
            },
        };
        let _ = sender.send(message);
    });
}

/// 「名前を付けて保存」ダイアログを表示する。
pub fn spawn_save_dialog(sender: Sender<IoMessage>) {
    std::thread::spawn(move || {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            let _ = sender.send(IoMessage::SaveTargetChosen(path));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_without_falling_back() {
        let bytes = "こんにちは、menfis".as_bytes();
        let (text, encoding) = decode_bytes(bytes);
        assert_eq!(text, "こんにちは、menfis");
        assert_eq!(encoding, encoding_rs::UTF_8);
    }

    #[test]
    fn decodes_shift_jis_when_not_valid_utf8() {
        let (sjis_bytes, _, had_errors) = encoding_rs::SHIFT_JIS.encode("軽快に動く");
        assert!(!had_errors);
        // Shift-JISのバイト列は(偶然一致しない限り)有効なUTF-8ではないため
        // フォールバックが働く。
        assert!(std::str::from_utf8(&sjis_bytes).is_err());

        let (text, encoding) = decode_bytes(&sjis_bytes);
        assert_eq!(text, "軽快に動く");
        assert_eq!(encoding, encoding_rs::SHIFT_JIS);
    }

    #[test]
    fn encode_decode_roundtrip_shift_jis() {
        let original = "日本語のテキストファイル";
        let bytes = encode_text(original, encoding_rs::SHIFT_JIS);
        let (decoded, encoding) = decode_bytes(&bytes);
        assert_eq!(decoded, original);
        assert_eq!(encoding, encoding_rs::SHIFT_JIS);
    }

    /// ダイアログを介さない、実ファイルへの読み書きの往復テスト。
    /// Ctrl+O/Ctrl+Sが実際に行う`fs::read`/`fs::write`と同じ経路を検証する。
    #[test]
    fn round_trip_through_real_file_utf8_and_shift_jis() {
        for encoding in [encoding_rs::UTF_8, encoding_rs::SHIFT_JIS] {
            let dir = std::env::temp_dir();
            let path = dir.join(format!("menfis_file_io_test_{}.txt", encoding.name()));
            let original = "menfisで日本語のファイルを開いたり保存したりする。";

            std::fs::write(&path, encode_text(original, encoding)).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let (decoded, detected) = decode_bytes(&bytes);

            assert_eq!(decoded, original);
            assert_eq!(detected, encoding);

            std::fs::remove_file(&path).unwrap();
        }
    }
}
