//! HTML 属性（attribute）の最小表現とユーティリティ
//!
//! 目的（概要）
//! - HTML の `name="value"` のペアを表すシンプルな構造体です。
//! - パーサーが1文字ずつ読み進める想定で、`add_char` で名前／値に文字を追加できます。
//!
//! 用語の橋渡し（TS / Python / Go）
//! - `String` は“所有する文字列”。TS の `string`、Python の `str`、Go の `string` に相当。
//! - `push(c)` は末尾に1文字追加。TS: `s += c`、Python: `s += c`（実際は新規作成）、Go: `s += string(c)`。
//! - `clone()` は“複製を返す”。所有権を保ったまま呼び出し側に値を渡すときに便利です。
//!
//! 使い方（イメージ）
//! ```ignore
//! use saba_core::renderer::html::attribute::Attribute;
//!
//! // name="value" を1文字ずつ作る
//! let mut attr = Attribute::new();
//! for ch in "class".chars() { attr.add_char(ch, true); }     // name 部分
//! for ch in "button primary".chars() { attr.add_char(ch, false); } // value 部分
//! assert_eq!(attr.name(), "class".to_string());
//! assert_eq!(attr.value(), "button primary".to_string());
//! ```

use alloc::string::String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    name: String,  // 例: "class"
    value: String, // 例: "button primary"
}

impl Attribute {
    // いわゆる“コンストラクタ”。空の name/value で開始します。
    pub fn new() -> Self {
        Self {
            name: String::new(),
            value: String::new(),
        }
    }

    // 1文字を追加する。`is_name=true` なら name 側、false なら value 側へ。
    // パーサーがトークンを読んでいる途中に少しずつ構築するユースケースを想定しています。
    pub fn add_char(&mut self, c: char, is_name: bool) {
        if is_name {
            self.name.push(c);
        } else {
            self.value.push(c)
        }
    }

    // 所有権を保ったまま取得できるよう、複製（clone）を返します。
    pub fn name(&self) -> String {
        self.name.clone()
    }

    // 同上。必要に応じて `&str` を返す API にすると余分なコピーを避けられます。
    pub fn value(&self) -> String {
        self.value.clone()
    }
}
