//! net_wasabi — WasabiOS 向けのネットワーク層ユーティリティ（no_std）
//!
//! 役割
//! - OS 側（WasabiOS）のネットワーク API（`noli` クレート）を使って、
//!   アプリが利用しやすい形にした“薄いラッパー”を提供します。
//! - このクレート自身は `no_std` で動作し、OS 提供の機能だけに依存します。
//!
//! モジュール
//! - `http`: TCP 上で HTTP/1.1 の最低限の送受信を行うクライアント実装（GET のみ簡易版）。
//!   - 文字列のパース（レスポンス分解）は `saba_core::http` に委ね、
//!     ここでは「DNS → TCP 接続 → 書き込み/読み込み」の I/O に専念します。
//!
//! 使い方（例）
//! ```ignore
//! use net_wasabi::http::HttpClient;
//!
//! let client = HttpClient::new();
//! let res = client.get("example.com".to_string(), 80, "index.html".to_string())?;
//! // `res` は `saba_core::http::HttpResponse` 相当の型を想定
//! ```
//!
//! 設計メモ
//! - `no_std` なので標準ライブラリの `std::net` や `std::io` は使いません。
//!   ソケットや I/O は `noli` クレートの API を使います。
//! - ヒープ確保型（`String`/`Vec`）を使う場合は、各モジュール側で `extern crate alloc;` を宣言します。
#![no_std]

pub mod http; // HTTP クライアント（GET の I/O を担当）
