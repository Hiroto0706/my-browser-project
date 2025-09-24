//! saba_core — アプリ共通の“コア”ライブラリ（no_std・初心者向け解説）
//!
//! 目的（なにを入れる？）
//! - アプリ全体で再利用するロジックをまとめる場所です。
//! - OS風の `no_std` 環境で動くことを前提にしています（ファイル/スレッドなどの標準APIは使いません）。
//!
//! キーワード（TS/Python/Go の感覚で）
//! - `#![no_std]`:
//!   - 標準ライブラリ `std` をリンクしないモード。言語のコア機能は `core` から提供されます。
//!   - TS/Python なら「OS 依存 API を使わない」制限モードのイメージ。
//! - `extern crate alloc;`:
//!   - `String`/`Vec`/`Box` など“ヒープ確保型”を使うための拡張パック。
//!   - TS の `string`/`Array<T>`、Python の `str`/`list`、Go の `[]T` に相当する可変データ構造。
//! - `pub mod xxx;`:
//!   - サブモジュールを公開（TS の `export * from './xxx'`、Python の `from . import xxx`）。
//!
//! 使い方（超ミニ例）
//! ```ignore
//! use saba_core::{url, http};
//!
//! // URL をパース（Result で成功/失敗を扱う）
//! let u = url::parse("https://example.com/index.html").expect("valid url");
//!
//! // 受け取った HTTP レスポンス文字列を構造体へ（イメージ）
//! // let res = http::HttpResponse::new(raw_http_string)?;
//! ```
//!
//! モジュール構成
//! - `error`:
//!   - 共有の `Error` 型。`Result<T, Error>` の `Err(...)` に使います。
//! - `http`:
//!   - HTTP 文字列の分解・表現（プロトコル層の型）。ネットワーク I/O は別クレートが担当。
//! - `url`:
//!   - URL のパース/ユーティリティ。
//!
//! メモ（設計指針）
//! - 失敗は `panic!` ではなく `Result` を返して上位に伝えます（安定性重視）。
//! - ネットワーク等の I/O は上位クレート（例: `net_wasabi`）に任せ、ここは“純粋な処理”に集中します。
#![no_std]

extern crate alloc; // no_std でも `String`/`Vec` を使うために必要

pub mod browser;
pub mod error; // 共有エラー型（Result<T, Error> 用）
pub mod http; // HTTP の型/処理（I/Oは含まない）
pub mod renderer; // HTMLレンダリング
pub mod url;
pub mod utils; // URL ユーティリティ
