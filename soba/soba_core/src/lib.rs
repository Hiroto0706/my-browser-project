//! soba_core: アプリ共通の「コア」機能（Rust 初心者向け解説付き）
//!
//! 目的:
//! - アプリから再利用する機能をまとめたライブラリ（crate）。
//! - WasabiOS のような「標準ライブラリなし」の環境でも動く設計。
//!
//! 用語のやさしい説明（TypeScript / Python に例えると）
//! - `#![no_std]`
//!   - Rust の標準ライブラリ（`std`）を使いません。TS/Pythonで言えば、
//!     「Node 標準 API や `os`/`threading` のような OS 依存 API を使わない」モード。
//!     代わりに言語の基礎だけ（`core`）で動きます。
//! - `extern crate alloc;`
//!   - `Vec`/`String`/`Box` のような「ヒープ確保」を可能にします。
//!     TS の `Array<string>` や Python の `list`/`str` のような可変コンテナを
//!     使うイメージ。ただし実際のメモリ確保は OS/ランタイムが提供する必要あり。
//! - `pub mod http;`
//!   - HTTP の“プロトコル層”ヘルパー（メッセージのパース/整形など）を置く予定の
//!     モジュールです。ネットワーク I/O（ソケット）までは扱いません。
//!     TS なら `parseRequest`, `formatResponse` のような文字列処理の居場所。
//! - `pub mod url;`
//!   - URL のパース/編集ユーティリティを公開します。TS の `export * from "./url";`
//!     や Python の `from . import url` に近い感覚です。
//!
//! 使い方（例）
//! ```ignore
//! use soba_core::{url, http};
//! // TS: import * as url from "soba_core/url"; import * as http from "soba_core/http";
//!
//! let u = url::parse("https://example.com/path").expect("有効なURL");
//! // Python なら try/except、Rust では Result を使います。
//! //   let u = url::parse("...")?;  // `?` は失敗を呼び出し元へ伝播（Pythonのraise相当）
//!
//! // HTTP は I/O なしの文字列/バイト列処理に特化します。
//! // ここに今後、`http::...` の型や関数（リクエスト/レスポンスの表現やパース）が追加されます。
//! ```
//!
//! メモ:
//! - 性能や安定性のため、パニックより `Result` を返す設計を優先しましょう。
//! - 共有ロジックは `soba_core`、アプリ固有の処理は `soba` 側へ置くと整理しやすいです。
#![no_std]

// no_std 環境でもヒープ確保型（Vec, String, Box など）を使えるようにする。
// TS/Pythonの可変配列や文字列に近い機能を Rust で利用できるようにするための一手間です。
extern crate alloc;

// HTTP/URL 関連のユーティリティを公開する（プロトコルと文字列処理が中心）。
// TS: export * from "./http"; export * from "./url";
pub mod error;
pub mod http;
pub mod url;
