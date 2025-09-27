//! DisplayItem — “描画命令”の最小表現（レンダリングの中間表現）
//!
//! 役割
//! - レイアウト計算の結果（どこに/どのサイズで描くか）と、スタイル（色など）を束ねて
//!   実際の描画バックエンドに渡しやすい形へ変換します。
//! - 実ブラウザでいえば「描画リスト」「ディスプレイリスト」に相当する超簡易版です。
//!
//! 現在の種類（学習用）
//! - `Rect` … 矩形の塗り潰しや背景など。スタイル一式 + 位置 + サイズを持ちます。
//! - `Text` … 文字列の描画。テキスト内容 + スタイル + 位置を持ちます。
//!
//! 例（イメージ）
//! - <p style="background-color:yellow">Hi</p>
//!   → [
//!        Rect { style(bg=yellow), point=(x,y), size=(w,h) },
//!        Text { text="Hi", style(color=...), point=(x_text,y_text) }
//!      ]
use crate::renderer::layout::computed_style::ComputedStyle;
use crate::renderer::layout::layout_object::LayoutPoint;
use crate::renderer::layout::layout_object::LayoutSize;
use alloc::string::String;

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    Rect {
        // 塗る/描くときに必要な CSS の最終的な値（色/フォントなど）
        style: ComputedStyle,
        // 左上座標（レイアウト座標系）
        layout_point: LayoutPoint,
        // 幅・高さ（ピクセル）
        layout_size: LayoutSize,
    },
    Text {
        // 描く文字列（等幅フォント前提の簡易モデル）
        text: String,
        // 文字色などのスタイル。
        style: ComputedStyle,
        // 左上座標（ベースラインではなく簡易に左上で扱う）
        layout_point: LayoutPoint,
    },
}
