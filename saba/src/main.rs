//! Saba アプリのエントリポイント（初心者向けの解説つき）
//!
//! 目的
//! - OS 風の実行環境（Wasabi 上）で最小の“ブラウザ”アプリを起動します。
//! - UI（アドレスバーなど）とネットワーク(HTTP)を橋渡しします。
//!
//! 環境の前提（no_std / no_main）
//! - `#![no_std]` … 標準ライブラリ(std)を使わないモード。OS の上で直接動くため、
//!   メモリ確保や I/O は OS 向けの薄いランタイム（`noli` など）に任せます。
//! - `#![no_main]` … 普通の `fn main()` をリンカが使わない設定。代わりに最後の
//!   `entry_point!(main)` マクロが OS 向けのエントリを定義します。
//! - `extern crate alloc;` … `Vec`/`String`/`Rc` などヒープ確保が必要な型を使うためのクレート。
//!
//! 全体の流れ
//! 1) `Browser::new()` でモデルを用意
//! 2) `WasabiUI::new(browser)` で UI を作成
//! 3) `ui.start(handle_url)` でイベントループ開始
//! 4) Enter で `handle_url(url)` が呼ばれ、HTTP 取得 → ページへ反映 → 再描画
//!
//! TS/Python にたとえると
//! - `Result<T, E>` は「成功 or 例外」の値。`match` は `try/except` と同じ役割。
//! - `Rc<RefCell<T>>` は「共有ポインタ + 実行時可変チェック」。TS の `readonly`/`mutable` と
//!   Python の参照共有の中間のようなイメージ（可変借用は 1 つだけ）。
#![no_std]
#![no_main]

extern crate alloc;

use crate::alloc::string::ToString;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use core::cell::RefCell;
use net_wasabi::http::HttpClient;
use noli::*;
use saba_core::browser::Browser;
use saba_core::error::Error;
use saba_core::http::HttpResponse;
use saba_core::url::Url;
use ui_wasabi::app::WasabiUI;

/// UI から渡された URL を解釈して HTTP GET し、レスポンスを返す
///
/// 入力/出力
/// - 入力: `url: String` … 例: "http://example.com:80/" のような完全 URL
/// - 出力: `Result<HttpResponse, Error>` … 成功なら HTTP レスポンス、失敗なら理由
///
/// 処理の手順（ざっくり）
/// 1) `Url::parse()` で `host/port/path` を取り出す（ドメイン名、ポート番号、パス）
/// 2) `HttpClient::get(host, port, path)` で HTTP を実行
/// 3) ステータスが 302 の場合は `Location` を読んで 1 回だけリダイレクト
///
/// 設計メモ（シンプル化のための前提）
/// - HTTP のみ想定（HTTPS/TLS は未対応）。
/// - リダイレクトは 1 回だけ追う（多段/無限ループ対策は未実装）。
/// - `Location` は絶対URLを想定（相対URLの解決は未実装）。
/// - ポート番号は `u16` に収まる前提（異常値は `expect` で早期失敗）。
fn handle_url(url: String) -> Result<HttpResponse, Error> {
    // 1) URL をパース（失敗したら人間に分かるメッセージで返す）
    //    TS/Python の try/except に相当。ここでは `UnexpectedInput` にマップします。
    let parsed_url = match Url::new(url.to_string()).parse() {
        Ok(url) => url,
        Err(e) => {
            return Err(Error::UnexpectedInput(format!(
                "input html is not supported: {:?}",
                e
            )));
        }
    };

    // 2) HTTP リクエストを送信（`get(host, port, path)` という素朴な API）
    //    - `host()` は例: "example.com"
    //    - `port()` は例: "80"（文字列）。数値に直してから渡す必要がある。
    //    - `path()` は例: "/index.html"（クエリ等は `Url` 実装に依存）
    let client = HttpClient::new();
    let response = match client.get(
        parsed_url.host(),
        // ポートは `u16` にパース。ここでは URL 側が既に正しい形式である前提で `expect` を使い、
        // 想定外の文字列が来たら開発時にすぐ気付けるようにします（本番用なら `?` でエラーにした方が安全）。
        parsed_url.port().parse::<u16>().expect(&format!(
            "port number should be u16 but got {}",
            parsed_url.port()
        )),
        parsed_url.path(),
    ) {
        Ok(res) => {
            // 3) 302 Found のときは 1 回だけ Location に従って再取得（簡易リダイレクト）
            //    - 一般的なブラウザは 301/302/303/307/308 などに対応しますが、ここでは 302 のみ。
            //    - `Location` ヘッダが無ければ、そのまま現在のレスポンスを返す（何もしない）。
            if res.status_code() == 302 {
                let location = match res.header_value("Location") {
                    Ok(value) => value,
                    Err(_) => return Ok(res),
                };
                // Location の URL を（簡易的に）解釈
                // 注意: 相対 URL の解決（"/path" を元 URL と合成する等）は未実装。
                let redirect_parsed_url = Url::new(location);

                let redirect_res = match client.get(
                    redirect_parsed_url.host(),
                    // リダイレクト先のポートも同様に数値化（同じ方針で `expect`）
                    redirect_parsed_url.port().parse::<u16>().expect(&format!(
                        "port number should be u16 but got {}",
                        parsed_url.port() // 実装メモ: メッセージは本来リダイレクト先のポートを表示したい
                    )),
                    redirect_parsed_url.path(),
                ) {
                    Ok(res) => res,
                    Err(e) => return Err(Error::Network(format!("{:?}", e))),
                };

                redirect_res
            } else {
                res
            }
        }
        Err(e) => {
            // ネットワーク層の失敗（接続不可など）は `Error::Network` として返す
            return Err(Error::Network(format!(
                "failed to get http response: {:?}",
                e
            )));
        }
    };
    Ok(response)
}

/// アプリのメイン関数（OS マクロ `entry_point!` から呼ばれる）
///
/// - 戻り値 `u64` は終了コード（0=成功）。標準の `std::process::ExitCode` がないため数値で返します。
fn main() -> u64 {
    // 1) Browser 構造体（モデル）を初期化
    let browser = Browser::new();

    // 2) WasabiUI を作成。UI は内部で可変状態を持つので `Rc<RefCell<_>>` で共有＆可変化
    let ui = Rc::new(RefCell::new(WasabiUI::new(browser)));

    // 3) イベントループ開始。`handle_url` をコールバックとして渡す。
    //    （TS なら `start(handleUrl)`, Python なら関数オブジェクトを渡すイメージ）
    match ui.borrow_mut().start(handle_url) {
        Ok(_) => {}
        Err(e) => {
            println!("browser fails to start {:?}", e);
            return 1;
        }
    };

    0
}

// OS 向けのエントリポイントを張るマクロ。
// - `no_main` 環境ではリンカに通常の `main` を見せない代わりに、
//   このマクロが UEFI/Wasabi 用の起動点を生成して `main()` を呼び出します。
entry_point!(main);
