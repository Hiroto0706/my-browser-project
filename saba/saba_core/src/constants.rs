//! レイアウト/描画のための固定定数（サンプル用）
//!
//! ここではウィンドウやテキストの基本寸法を“仮固定値”として定義します。
//! 実ブラウザのような DPI/フォントメトリクス/ズーム対応は行わず、分かりやすさを優先します。
//! - すべて `i64` で表しており、ピクセル単位の簡易モデルです。

// ウィンドウ全体のサイズ（ピクセル）
pub static WINDOW_WIDTH: i64 = 600;
pub static WINDOW_HEIGHT: i64 = 400;
// ウィンドウ内側の余白（上下左右に同値を適用）
pub static WINDOW_PADDING: i64 = 5;

// noli ライブラリの UI 部分に合わせたメニューバー/ツールバー等の高さ
// （このプロジェクトでは固定値として扱い、動的計測はしません）
pub static TITLE_BAR_HEIGHT: i64 = 24;

pub static TOOLBAR_HEIGHT: i64 = 26;

// 実際にコンテンツ（HTML）が描画される領域のサイズ
// - 横幅は左右のウィンドウ余白を差し引き
// - 高さはタイトルバー/ツールバー/上下余白を差し引き
pub static CONTENT_AREA_WIDTH: i64 = WINDOW_WIDTH - WINDOW_PADDING * 2;
pub static CONTENT_AREA_HEIGHT: i64 =
    WINDOW_HEIGHT - TITLE_BAR_HEIGHT - TOOLBAR_HEIGHT - WINDOW_PADDING * 2;

// 等幅フォントの想定文字サイズ（ピクセル）
// - 文字幅: 8px, 文字高: 16px（学習用の仮値）
// - 行の高さ（行送り）は適当に +4px して “詰まりすぎない” 程度に余白を追加
pub static CHAR_WIDTH: i64 = 8;
pub static CHAR_HEIGHT: i64 = 16;
pub static CHAR_HEIGHT_WITH_PADDING: i64 = CHAR_HEIGHT + 4;
