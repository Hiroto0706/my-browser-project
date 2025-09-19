//! Wasabi/ネットワーク層: 超ミニ HTTP クライアント（初心者向けコメント付き）
//!
//! 目的
//! - `no_std` 方針の環境で、ソケット I/O を使って HTTP/1.1 の GET を最小構成で試す。
//! - DNS で IP を引く → TCP 接続 → リクエスト文字列を送る → バイト列で受信 → UTF-8 文字列へ。
//!
//! 用語の橋渡し（TypeScript / Python に例えると）
//! - `extern crate alloc;` は、`Vec`/`String`/`format!` など“ヒープを使う型”を有効化するスイッチ。
//!   - TS の `string[]` や `string`、Python の `list`/`str` を使うための準備に近い。
//! - `Result<T, E>` は try/except の結果。`?` は `await/raise` っぽく失敗を上に伝える。
//!
//! 入出力（このファイルの中心 API）
//! - `HttpClient::get(host, port, path)`
//!   - 入力: `host`（例: `"example.com"`）, `port`（例: `80`）, `path`（例: `"index.html"`）
//!   - 注意: 本実装は `"GET /" + path` で組み立てるため、`path` の先頭に `/` を付けない想定。
//!     すでに `/` を含めると `//index.html` のように二重スラッシュになります。
//!   - 出力: サーバからのレスポンスをアプリ側型（`HttpResponse` 等）に包んで返す `Result`。
//!
//! 使い方（イメージ）
//! ```ignore
//! use soba::net::wasabi::http::HttpClient;
//!
//! let client = HttpClient::new();
//! let resp = client.get("example.com".to_string(), 80, "index.html".to_string())?;
//! // ここで `resp` からステータスやボディを取り出して表示・解析する。
//! ```
//!
//! 実装の流れ（ざっくり）
//! 1. `lookup_host(host)` で A レコード等を解決（DNS）。
//! 2. 先頭の IP + `port` で `TcpStream::connect`（TCP 三者間ハンドシェイク）。
//! 3. `GET` リクエスト文字列を組み立てる（リクエストライン + ヘッダ）。
//! 4. `stream.write(..)` で送信。
//! 5. `stream.read(..)` をループして受信バッファ `Vec<u8>` に追記。
//! 6. `core::str::from_utf8(..)` で UTF-8 として解釈し、アプリ側の型へ移す。
//!
//! 注意事項（学習ポイント）
//! - HTTP/1.1 の行区切りは本来 `\r\n`（CRLF）。ここでは簡便のため `\n` を使っており、
//!   サーバによっては正しく受け取られないことがあります（改善候補）。
//! - `Host:` ヘッダは仮想ホストに必須。変数名のタイプミスがないか注意（`host` を使う）。
//! - `alloc` を使うため、別途グローバルアロケータが必要なターゲットもあります。
//! - ネットワーク I/O は失敗しやすいので、丁寧に `Result` を返して上位で扱う設計にします。
//!
//! ここは“プロトコル文字列のやり取り”に専念し、URL の構築や高機能なパーサは
//! `soba_core::url` や `soba_core::http` 側に育てていく想定です。

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

pub mod http;

pub struct HttpClient {}

impl HttpClient {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get(&self, host: String, port: u16, path: String) -> Result<HttpResult, Error> {
        // 1) DNS: noli の `lookup_host` で `host` → IP アドレス群を取得
        //    例: "example.com" → [93.184.216.34]
        let ips = match lookup_host(&host) {
            Ok(ips) => ips,
            Err(e) => {
                return Err(Error::Network(format!(
                    "Failed to find IP addresses: {:#?}",
                    e
                )))
            }
        };

        if ips.len() < 1 {
            return Err(Error::Network("Failed to find IP addresses".to_string()));
        }

        // 2) 最初の IP と `port` からソケットアドレスを作成
        let socket_addr: SocketAddr = (ips[0], port).into();

        // 3) TCP 接続を確立する
        let mut stream = match TcpStream::connect(socket_addr) {
            Ok(stream) => stream,
            Err(_) => {
                return Err(Error::Network(
                    "Failed to connect to TCP stream".to_string(),
                ))
            }
        };

        // 4) リクエストラインの作成
        //    注意: ここでは `"GET /" + path + " HTTP/1.1\n"` としており、CRLF ではない簡易版。
        //    RFC 準拠にするなら "\r\n" を使い、ヘッダ行も CRLF にします（改善候補）。
        let mut request = String::from("GET /");
        request.push_str(&path);
        request.push_str(" HTTP/1.1\n");

        // 5) ヘッダの追加
        //    Host は必須。ここでは最小限の Accept と Connection も付ける。
        request.push_str("Host: ");
        request.push_str(&host);
        request.push_str("\n");
        request.push_str("Accept: text/html\n");
        request.push_str("Connection: close\n"); // 毎回接続を切るため
        request.push_str("\n");

        // 6) サーバへ送信
        //    `write` でバイト列を送り、書き込めたバイト数が返る。
        let bytes_written = match stream.write(request.as_bytes()) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(Error::Network(
                    "Failed to send a request to TCP stream".to_string(),
                ))
            }
        };

        // 7) レスポンス受信
        //    先に `Vec<u8>` を用意し、`read` で受け取った分だけ順次追記する。
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

        // 8) UTF-8 として解釈し、アプリ側の HTTP 型へ渡す
        match core::str::from_utf8(&received) {
            Ok(response) => HttpResponse::new(response.to_string()),
            Err(e) => Err(Error::Network(format!("Invalid received response: {}", e))),
        }
    }
}
