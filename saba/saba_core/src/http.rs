//! saba_core::http — 最小限の HTTP レスポンス表現とパーサ
//!
//! 目的（概要）
//! - テキストの生HTTPレスポンス（`String`）を、扱いやすい構造体 `HttpResponse` に変換します。
//! - OS風の `no_std` 環境でも使えるよう、`alloc` クレートの `String`/`Vec` のみを前提にしています。
//!
//! 用語の橋渡し（TS / Python）
//! - trait ≈ TSのinterface / Pythonのprotocol（ここでは `ToString` を利用）。
//! - `Result<T, E>` ≈ try/except の結果。成功= `Ok(T)` / 失敗= `Err(E)`。`?` でエラー伝播できます。
//! - 所有文字列 `String` と借用文字列 `&str`：`to_string()`/`String::from(...)` で `String` を作ります。
//!
//! 入出力（このモジュール）
//! - 入力: `HttpResponse::new(raw_response: String)` — 生のレスポンステキスト。
//! - 出力: `Result<HttpResponse, Error>` — 解析成功なら `HttpResponse`、失敗なら `Error`。
//!
//! 使い方（例）
//! ```ignore
//! use saba_core::http::HttpResponse;
//! use saba_core::error::Error;
//!
//! fn parse(raw: String) -> Result<HttpResponse, Error> {
//!     HttpResponse::new(raw)
//! }
//! ```
//!
//! 注意（簡易パーサ）
//! - CRLF(\r\n) を LF(\n) に正規化し、`"\n\n"` でヘッダとボディを分割するシンプル実装です。
//! - ステータス行は `"HTTP/1.1 200 OK"` の3要素を想定。理由句に空白が含まれるケースなどは未対応です。
//! - ステータスコードのパース失敗時は 404 にフォールバックしています（本番実装では要見直し）。

use crate::alloc::string::ToString;
use crate::error::Error;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
/// 単一HTTPヘッダを表す(name, value)ペア。
/// - 例: `("Content-Length", "42")`
pub struct Header {
    /// ヘッダ名（大文字小文字の扱いは簡易実装。完全な正規化は未実装）
    name: String,
    /// ヘッダ値（前後の空白はトリムして格納）
    value: String,
}

impl Header {
    /// ヘッダを新規作成します。
    /// 引数は所有文字列 `String`。`&str` から渡す場合は `to_string()` 等で変換してください。
    pub fn new(name: String, value: String) -> Self {
        Self { name, value }
    }
}

#[derive(Debug, Clone)]
/// 簡易HTTPレスポンス。
/// - `version`: 例 `"HTTP/1.1"`
/// - `status_code`: 例 `200`
/// - `reason`: 例 `"OK"`
/// - `headers`: 例 `[Header { name: "Date", value: "..." }]`
/// - `body`: 本文（テキスト）
pub struct HttpResponse {
    version: String,
    status_code: u32,
    reason: String,
    headers: Vec<Header>,
    body: String,
}

impl HttpResponse {
    /// 生のHTTPレスポンス文字列を解析して `HttpResponse` を作ります。
    ///
    /// 振る舞い（簡易）：
    /// - 先頭の空白を除去し、改行を CRLF→LF に正規化します。
    /// - 1行目をステータス行 (`HTTP/1.1 200 OK`) として分解します。
    /// - 続くブロックを `\n\n` でヘッダ/ボディに分割し、ヘッダは `:` で `name: value` に分けます。
    /// - 想定と異なる形の場合は `Error::Network` を返します（用途的に簡易化）。
    pub fn new(raw_response: String) -> Result<Self, Error> {
        // 余分な先頭空白を削り、CRLF -> LF に統一。
        let preprocessed_response = raw_response.trim_start().replace("\r\n", "\n");

        // 1行目（ステータス行）を取り出す。
        let (status_line, remaining) = match preprocessed_response.split_once('\n') {
            Some((s, r)) => (s, r),
            None => {
                // 1行も取れない＝HTTPとして不正。
                return Err(Error::Network(format!(
                    "invalid http response: {}",
                    preprocessed_response
                )));
            }
        };

        // ヘッダ部とボディ部を空行で分割。
        let (headers, body) = match remaining.split_once("\n\n") {
            Some((h, b)) => {
                let mut headers = Vec::new();
                for header in h.split('\n') {
                    // `name: value` の最初のコロンで2分割し、前後空白をトリム。
                    let splitted_header: Vec<&str> = header.splitn(2, ':').collect();
                    headers.push(Header::new(
                        String::from(splitted_header[0].trim()),
                        String::from(splitted_header[1].trim()),
                    ));
                }
                (headers, b)
            }
            // ヘッダがなく、直ちにボディ（または空）のケース。
            None => (Vec::new(), remaining),
        };

        // ステータス行を空白で分割（例: ["HTTP/1.1", "200", "OK"]).
        let statuses: Vec<&str> = status_line.split(' ').collect();

        // 注意：本実装は要素数や理由句に空白があるケースなどを厳密に扱っていません。
        Ok(Self {
            version: statuses[0].to_string(),
            // 数値パース失敗時は 404 にフォールバック（暫定）。
            status_code: statuses[1].parse().unwrap_or(404),
            reason: statuses[2].to_string(),
            headers,
            body: body.to_string(),
        })
    }

    /// HTTPバージョン（例: `"HTTP/1.1"`）を返します。
    pub fn version(&self) -> String {
        self.version.clone()
    }

    /// ステータスコード（例: `200`）を返します。
    pub fn status_code(&self) -> u32 {
        self.status_code
    }

    /// 理由句（例: `"OK"`）を返します。
    pub fn reason(&self) -> String {
        self.reason.clone()
    }

    /// ヘッダ一覧をクローンして返します。
    pub fn headers(&self) -> Vec<Header> {
        self.headers.clone()
    }

    /// 本文（テキスト）を返します。
    pub fn body(&self) -> String {
        self.body.clone()
    }

    /// 指定したヘッダ名の値を返します（見つからなければ `Err(String)`）。
    /// - 大文字小文字や重複ヘッダは簡易対応です。必要に応じて拡張してください。
    pub fn header_value(&self, name: &str) -> Result<String, String> {
        for h in &self.headers {
            if h.name == name {
                return Ok(h.value.clone());
            }
        }

        Err(format!("failed to find {} in headers", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid() {
        let raw = "HTTP/1.1 200 OK".to_string();
        assert!(HttpResponse::new(raw).is_err());
    }

    #[test]
    fn test_status_line_only() {
        let raw = "HTTP/1.1 200 OK\n\n".to_string();
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");
    }

    #[test]
    fn test_one_header() {
        let raw = "HTTP/1.1 200 OK\nDate:xx xx xx\n\n".to_string();
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");

        assert_eq!(res.header_value("Date"), Ok("xx xx xx".to_string()));
    }

    #[test]
    fn test_two_headers_with_white_space() {
        let raw = "HTTP/1.1 200 OK\nDate: xx xx xx\nContent-Length: 42\n\n".to_string();
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");

        assert_eq!(res.header_value("Date"), Ok("xx xx xx".to_string()));
        assert_eq!(res.header_value("Content-Length"), Ok("42".to_string()));
    }

    #[test]
    fn test_body() {
        let raw = "HTTP/1.1 200 OK\nDate: xx xx xx\n\nbody message".to_string();
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");

        assert_eq!(res.header_value("Date"), Ok("xx xx xx".to_string()));

        assert_eq!(res.body(), "body message".to_string());
    }
}
