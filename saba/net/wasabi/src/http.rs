//! net_wasabi::http — no_std環境向けの極小HTTPクライアント
//!
//! 目的（概要）
//! - OS風(no_std)環境で、TCP越しに単純な HTTP/1.1 GET を投げて文字列レスポンスを得ます。
//! - パースは `saba_core::http::HttpResponse` に委譲します（このモジュールは送受信担当）。
//!
//! 言語の橋渡し（TS/Python）
//! - `Result<T, E>` ≈ try/catch(try/except) の結果。成功は `Ok(T)`、失敗は `Err(E)`。
//! - `String` は所有文字列。`&str` からは `to_string()` や `String::from(...)` で作ります。
//! - trait ≈ TS interface / Python protocol。ここでは `write`/`read` などのメソッドを使います。
//!
//! 入出力（このモジュール）
//! - 入力: `HttpClient::get(host, port, path)` — 例: ("example.com", 80, "index.html")。
//! - 出力: `Result<HttpResponse, Error>` — 成功時はHTTPレスポンス、失敗時は `Error::Network` など。
//!
//! 注意
//! - 簡易実装です。リダイレクト、TLS、分割転送、ヘッダの詳細（大文字小文字）、HTTP/2 など未対応。
//! - 改行は `\n` を用い、`Connection: close` でサーバ側の接続終了まで受信します。
//! - `path` は先頭 `/` なしで渡す想定。`GET /{path} HTTP/1.1` を送ります。
extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use noli::net::lookup_host;
use noli::net::SocketAddr;
use noli::net::TcpStream;
use saba_core::error::Error;
use saba_core::http::HttpResponse;

/// 最小限のHTTPクライアント。状態は持たないので空構造体です。
pub struct HttpClient {}

impl HttpClient {
    /// クライアントを生成します。コネクションはこの時点では作りません。
    pub fn new() -> Self {
        Self {}
    }

    /// HTTP/1.1 の GET を送ります。
    ///
    /// 引数
    /// - `host`: 例 "example.com"（DNSルックアップ対象）
    /// - `port`: 例 80
    /// - `path`: 例 "index.html"（先頭に `/` を付けない想定）
    ///
    /// 戻り値
    /// - 成功: `Ok(HttpResponse)`
    /// - 失敗: `Err(Error::Network(...))`（DNS/接続/送信/受信/UTF-8変換など）
    pub fn get(&self, host: String, port: u16, path: String) -> Result<HttpResponse, Error> {
        // 1) DNS解決: ホスト名 → IPリスト
        let ips = match lookup_host(&host) {
            Ok(ips) => ips,
            Err(e) => {
                return Err(Error::Network(format!(
                    "Failed to find IP addresses: {:#?}",
                    e
                )))
            }
        };

        // 解決結果が空なら失敗。
        if ips.len() < 1 {
            return Err(Error::Network("Failed to find IP addresses".to_string()));
        }

        // 2) 最初のIPに対してTCP接続。
        let socket_addr: SocketAddr = (ips[0], port).into();

        let mut stream = match TcpStream::connect(socket_addr) {
            Ok(stream) => stream,
            Err(_) => {
                return Err(Error::Network(
                    "Failed to connect to TCP stream".to_string(),
                ))
            }
        };

        // 3) リクエストラインとヘッダを組み立て。
        let mut request = String::from("GET /");
        request.push_str(&path);
        request.push_str(" HTTP/1.1\n");

        // ヘッダの追加
        request.push_str("Host: ");
        request.push_str(&host);
        request.push('\n');
        request.push_str("Accept: text/html\n");
        request.push_str("Connection: close\n"); // keep-aliveは使わず、応答後に切ってもらう
        request.push('\n');

        // 4) 送信。
        let _bytes_written = match stream.write(request.as_bytes()) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(Error::Network(
                    "Failed to send a request to TCP stream".to_string(),
                ))
            }
        };

        // 5) 受信。サーバ側が切る（0バイト）まで読み続ける。
        let mut received = Vec::new();
        loop {
            let mut buf = [0u8; 4096];
            let bytes_read = match stream.read(&mut buf) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return Err(Error::Network(
                        "Failed to receive a request from TCP stream".to_string(),
                    ))
                }
            };
            if bytes_read == 0 {
                break;
            }
            received.extend_from_slice(&buf[..bytes_read]);
        }

        // 6) 受信バイト列 → UTF-8文字列 → `HttpResponse` にパース。
        match core::str::from_utf8(&received) {
            Ok(response) => HttpResponse::new(response.to_string()),
            Err(e) => Err(Error::Network(format!("Invalid received response: {}", e))),
        }
    }
}
