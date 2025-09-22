//! saba/bin — OS風(no_std)環境での最小エントリポイント
//!
//! 目的（概要）
//! - 標準ライブラリなし（`no_std`）で動くアプリの“入口”です。
//! - ネットワークI/Oはライブラリ側（`net_wasabi`）に任せ、ここでは呼び出して結果を表示します。
//!
//! 言語の橋渡し（TS/Python/Go）
//! - `Result<T, E>` は try/catch(try/except) の結果。成功= `Ok(T)` / 失敗= `Err(E)`。
//! - `to_string()` は借用文字列 `&str` → 所有文字列 `String` へ変換します。
//!   - TS: `String(value)`、Python: `str(value)`、Go: `string(bytes)` に近い変換。
//! - `entry_point!(main)` は“OS向けの起動点”を生成します。通常の `fn main(){}` とは別の仕組みです。
//!
//! 入出力（このファイル）
//! - 入力: なし（OSから起動）
//! - 出力: シリアル/コンソールにHTTPの結果を印字し、終了コード `u64` を返します。
#![no_std]
#![no_main]

extern crate alloc; // `String`/`Vec` 等のヒープ確保型を使うためのクレート（no_stdで必要）

use crate::alloc::string::ToString; // `to_string()` を使うためのトレイト
use net_wasabi::http::HttpClient;   // HTTPクライアント（TCPでGETを投げる最小実装）
use noli::prelude::*;               // OS側が提供する `print!` などのユーティリティ

// エントリ関数。戻り値 `u64` は“終了コード”のイメージです。
fn main() -> u64 {
    // 1) クライアントを作る。
    //    Rustでは“コンストラクタ”として関連関数 `new()` をよく使います。
    let client = HttpClient::new();

    // 2) HTTP GET を実行。
    //    引数: (ホスト名, ポート番号, パス)。パスの先頭に `/` は付けない想定。
    //    `&str` を `String` にするため `to_string()` を使っています。
    match client.get("host.test".to_string(), 8000, "test.html".to_string()) {
        Ok(res) => {
            // 成功。`Ok(res)` の中身 `res` をデバッグ表示。
            print!("response:\n{:#?}", res);
        }
        Err(e) => {
            // 失敗。`Err(e)` のエラー内容を表示。
            print!("error:\n{:#?}", e);
        }
    }
    // 3) 正常終了。
    0
}

// OS向けの“本当の”入口を生成するマクロ。
// ブートローダ/ランタイムがここで生成された入口を呼び、上の `main()` に制御が渡ります。
entry_point!(main);
