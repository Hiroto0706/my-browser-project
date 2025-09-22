//! soba_core::error — アプリ全体で使う簡易エラー型
//!
//! 目的（概要）
//! - `no_std` 環境で扱いやすい、軽量なエラー列挙体（enum）を提供します。
//! - ネットワークや入力など、よくある失敗パターンをざっくり分類して、
//!   `Result<T, Error>` の `Err(Error::...)` に使います。
//!
//! 用語の橋渡し（TS / Python / Go）
//! - Rustの `enum` は TS のユニオン型（タグ付き）や Python/Go の列挙に近いですが、
//!   個々のバリアントがデータ（ここでは文字列メッセージ）を運べるのが特徴です。
//! - `Result<T, Error>` は try/catch(try/except) 的なもの：成功 `Ok(T)` / 失敗 `Err(Error)`。
//! - メッセージは `String`（所有文字列）。`&str` から作るときは `String::from(...)` や `to_string()` を使います。
//!
//! 使い方（例）
//! ```ignore
//! use soba_core::error::Error;
//!
//! fn fetch() -> Result<(), Error> {
//!     // 失敗時に `Err(Error::Network("...".to_string()))` のように返す
//!     Err(Error::Network("dns lookup failed".to_string()))
//! }
//! ```

use alloc::string::String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// ネットワーク関連の失敗（接続不可・送受信エラー・DNS失敗など）
    Network(String),
    /// 予期しない入力（フォーマット不正・未対応の値など）
    UnexpectedInput(String),
    /// UIまわりの不整合（存在しない要素参照など）。プロジェクト都合の分類。
    InvalidUI(String),
    /// 上記に当てはまらない汎用的な失敗
    Other(String),
}
