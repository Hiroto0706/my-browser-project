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
//! - `pub mod url;`
//!   - `src/url.rs` の機能を公開します。TS の `export * from "./url";`
//!     や Python の `from . import url` に近い感覚です。
//!
//! 使い方（例）
//! ```ignore
//! use soba_core::url::parse; // TS: import { parse } from "soba_core/url";
//!
//! let u = parse("https://example.com/path").expect("有効なURL");
//! // Python なら try/except で例外処理、Rust では Result を使います。
//! //   let u = parse("...")?;  // `?` は失敗を呼び出し元へ伝播（Pythonのraise相当）
//! ```
//!
//! メモ:
//! - 性能や安定性のため、パニックより `Result` を返す設計を優先しましょう。
//! - 共有ロジックは `soba_core`、アプリ固有の処理は `soba` 側へ置くと整理しやすいです。
#![no_std]

// no_std 環境でもヒープ確保型（Vec, String, Box など）を使えるようにする。
// TS/Pythonの可変配列や文字列に近い機能を Rust で利用できるようにするための一手間です。
extern crate alloc;

// URL 関連のユーティリティを公開する。
// TS: export * from "./url"; / Python: from . import url
pub mod url;
