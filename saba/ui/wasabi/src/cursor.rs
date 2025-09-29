//! カーソル描画用の小さなユーティリティ（初心者向け解説付き）
//!
//! 目的
//! - 画面上に 10x10 の赤い四角形を「カーソル」として表示します。
//! - `Sheet`（スプライト/レイヤーのようなもの）上に描画し、座標を動かして `flush` で反映します。
//!
//! ざっくり用語
//! - `Sheet` … 透明背景の小さなキャンバス。ウィンドウやスプライトのように個別に移動・更新できます。
//! - `Rect` … 位置とサイズ（x, y, width, height）。左上が (0,0)、右と下へ増える 2D 座標です。
//! - `bitmap_draw_rect` … 指定色で長方形をベタ塗りする低レベル API。
//! - 色 … `0xRRGGBB` の 24bit。例: `0xff0000` は赤。`0x00ff00` は緑。
//!
//! TS/Python にたとえると
//! - `Sheet` は「ミニキャンバスを持つオブジェクト」。`rect()` はプロパティ getter。
//! - `flush()` は「バッファに描いた内容を画面へコミット」する関数。Canvas の `present()` 的なイメージ。
//!
//! 使い方例
//! ```rust
//! // 作成（原点に 10x10 の赤いカーソル）
//! let mut cursor = Cursor::new();
//! // 好きな位置へ移動
//! cursor.set_position(100, 120);
//! // 画面に反映（この呼び出しで初めて見える）
//! cursor.flush();
//! ```
//!
//! メモ（no_std/OS 風味）
//! - 本プロジェクトは OS 風の環境で動くため、標準 GUI はありません。
//! - `noli` クレートの API を使って、直接ピクセル/レイヤーを扱います。

use noli::bitmap::bitmap_draw_rect;
use noli::rect::Rect;
use noli::sheet::Sheet;

/// 赤い四角で表現するカーソル。
///
/// - 内部に `Sheet` を 1 枚持ちます（10x10 px）。
/// - `set_position(x, y)` でシートを移動し、`flush()` で画面に反映します。
#[derive(Debug, Eq, PartialEq)]
pub struct Cursor {
    sheet: Sheet,
}

impl Cursor {
    /// 原点 `(0,0)` に 10x10 の赤いカーソルを作る
    ///
    /// 実装メモ
    /// - `Sheet::new(Rect::new(0, 0, 10, 10)?)` で 10x10 の描画面を用意。
    /// - `bitmap_draw_rect(..., 0xff0000, 0, 0, 10, 10)` で全面を赤く塗る。
    /// - ここでは `expect` を使っていますが、初期化失敗時は即座に分かりやすく止めたい意図です。
    pub fn new() -> Self {
        let mut sheet = Sheet::new(Rect::new(0, 0, 10, 10).unwrap());
        let bitmap = sheet.bitmap();
        bitmap_draw_rect(bitmap, 0xff0000, 0, 0, 10, 10).expect("failed to draw a cursor");

        Self { sheet }
    }

    /// 現在のカーソル矩形（座標とサイズ）を返す
    ///
    /// - 返り値の `Rect` は `x, y, width, height` を持ちます。
    pub fn rect(&self) -> Rect {
        self.sheet.rect()
    }

    /// カーソルの座標を更新する（バッファ上の移動）
    ///
    /// - まだ画面には反映されません。`flush()` を呼ぶまで待機します。
    pub fn set_position(&mut self, x: i64, y: i64) {
        self.sheet.set_position(x, y);
    }

    /// シートの変更内容を画面へ反映する（部分更新）
    ///
    /// - 頻繁に座標を動かす場合、`set_position` → `flush` をセットで呼びます。
    pub fn flush(&mut self) {
        self.sheet.flush();
    }
}
