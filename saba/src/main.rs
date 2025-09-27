//! saba/bin — no_std 環境での最小メイン（初心者向けコメント）
//!
//! ここは“アプリの入口”。標準ライブラリを使わない no_std かつ OS 風の実行環境で動かします。
//! - `no_std` により `std` の代わりに OS が提供する `noli` のAPIを使います。
//! - サンプルとして、HTTPレスポンス相当の文字列を解析→DOM/CSSOM化→（内部で）レイアウト/描画準備まで行います。
//!   ここでは画面出力は行わず、パイプラインの入口とデータフローの最小例に留めます。
//! - TS/Python/Go の感覚: `Browser` は“ページ状態を持つクラス”、`to_string()` は `str(value)` 相当。
#![no_std]
#![no_main]

extern crate alloc; // ヒープ型（String/Vecなど）を使うためのクレート（no_std では明示）

use crate::alloc::string::ToString; // `&str` → `String` へ変換する `to_string()` を使うため
use noli::*; // OS 環境側のプリリュード（println! など）
use saba_core::browser::Browser; // ブラウザ本体（ページ/DOMの管理）
use saba_core::http::HttpResponse; // HTTP レスポンス文字列のパース/保持

// 簡易デモ用の“HTTPレスポンス風”文字列。
// ポイント:
// - ステータス行: HTTP/1.1 200 OK
// - ヘッダ: Data（本来は Date など）
// - 空行の後に HTML 本文
static TEST_HTTP_RESPONSE: &str = r#"HTTP/1.1 200 OK
Data: xx xx xx


<html>
<head></head>
<body>
  <h1 id="title">H1 title</h1>
  <h2 class="class">H2 title</h2>
  <p>Test text.</p>
  <p>
    <a href="example.com">Link1</a>
    <a href="example.com">Link2</a>
  </p>
</body>
</html>
"#;

fn main() -> u64 {
    // 1) ブラウザインスタンスを用意（内側に“現在のページ”を持つ）
    let browser = Browser::new();

    // 2) サンプルのHTTPレスポンス文字列を `HttpResponse` にパース
    //    - `to_string()` は &str → String。エラー時は expect で失敗表示。
    let response =
        HttpResponse::new(TEST_HTTP_RESPONSE.to_string()).expect("failed to parse http response");

    // 3) 現在のページへレスポンスを渡し、DOM/CSSOM の構築（以降のレイアウト/描画準備）を行う
    //    - `borrow()/borrow_mut()` は `RefCell` の可変/不変借用（内部可変）
    //    - 具体的には: HTML → DOM、<style> → CSSOM、レイアウトツリー作成・サイズ/位置計算・
    //      DisplayItem 生成、といった処理をページ内で進める設計（最小版）
    let page = browser.borrow().current_page();
    page.borrow_mut().receive_response(response);

    // 4) 今は画面描画やログ出力は行わず、正常終了コードを返すだけ

    0
}

// OS向けの起動点を登録（no_main 環境で main を呼び出すためのマクロ）
entry_point!(main);
