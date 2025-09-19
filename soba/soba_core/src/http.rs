//! soba_core::http — HTTPレスポンス文字列を「構造体」に分解する最小モジュール
//!
//! ねらい
//! - ネットワーク処理はしません（ソケットは別層が担当）。
//! - 受け取った生の HTTP/1.1 風の文字列から、バージョン/ステータス/理由句/ヘッダ/ボディを抜き出し、
//!   `HttpResponse` として保持します。
//!
//! 重要ポイント（初心者向け）
//! - コンストラクタ: Rust ではクラスの `constructor` の代わりに「関連関数」を使います。
//!   失敗しうる初期化は `fn new(...) -> Result<Self, Error>` の形にして、
//!   成功は `Ok(Self { ... })`、失敗は `Err(Error::... )` を返します。
//! - `Ok(...)` は「成功の箱」。中に“完成した値（構造体など）”を入れて返します。
//! - 行の区切りは本来 `\r\n`（CRLF）ですが、扱いやすさのため一旦 `\n` に正規化しています。
//! - `split_once`, `splitn` は「最初の1回だけ区切る」分割。ヘッダの `Name: Value` に便利です。
//!
//! 言語ブリッジ（TS / Python / Go の感覚）
//! - 反復子と `collect()`（Rust）
//!   - Rust の多くの分割・生成APIは「イテレータ（iterator）」を返します。配列にしたいときは
//!     `collect::<Vec<_>>()` でベクタ（動的配列）に“集める”必要があります。
//!   - TS: `Array.from(iterable)` や `[...iterable]` に相当。Python: `list(iterable)`。
//!   - Go: `strings.Split` は最初から `[]string` を返すイメージ（Rust では明示に collect する）。
//! - `String::from(..)` と `to_string()`（所有する文字列を作る）
//!   - `&str`（借用された文字列スライス）→ `String`（所有する可変文字列）へ“確保してコピー”します。
//!   - TS: `String(value)`、Python: `str(value)`、Go: `string(bytes)` や `s` のコピーに近い感覚。
//!   - どちらもヒープ確保が起きます（`alloc` が必要）。`to_string()` は `Display` 実装からも生成可能。
//! - `parse::<u32>()` は文字列→数値変換。Python: `int(s)`、TS: `Number(s)`、Go: `strconv.Atoi(s)`。
//!
//! 例（入力のイメージ）
//! ```text
//! HTTP/1.1 200 OK\r\n
//! Content-Type: text/plain\r\n
//! Content-Length: 5\r\n
//! \r\n
//! hello
//! ```
//! これを `HttpResponse::new(raw_string)` に渡すと、各フィールドに分解されます。

use crate::alloc::string::ToString;
use crate::error::Error;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    version: String,      // 例: "HTTP/1.1"
    status_code: u32,     // 例: 200
    reason: String,       // 例: "OK"
    headers: Vec<Header>, // 例: [("Content-Type","text/html;charset=utf-8"), ...]
    body: String,         // 例: "Hello World!"
}

// 1行のヘッダ "Name: Value" を表す単純構造体。
#[derive(Debug, Clone)]
pub struct Header {
    name: String,  // 例: "Content-Type"
    value: String, // 例: "text/html; charset=utf-8"
}

impl HttpResponse {
    // 失敗しうる“コンストラクタ”パターン。
    // - Rust では「関連関数」`fn new(...) -> Result<Self, Error>` をよく使います。
    // - 成功したら `Ok(Self { ... })` を返す（= 成功の箱に完成品を入れて返す）。
    // - 失敗したら `Err(Error::...)` を返す（= 何が悪いかを型で示す）。
    pub fn new(raw_response: String) -> Result<Self, Error> {
        // 0) 前処理: 先頭の空白/改行を削り、CRLF(\r\n) → LF(\n) へ正規化
        //    - RFC準拠は CRLF ですが、パースを簡単にするため内部表現を LF に寄せます。
        let preprocessed_response = raw_response.trim_start().replace("\r\n", "\n");

        // 1) ステータスラインを取り出す
        //    例: "HTTP/1.1 200 OK\n..."
        let (status_line, remaining) = match preprocessed_response.split_once("\n") {
            Some((s, r)) => (s, r),
            None => {
                // 行が無ければ“レスポンスとして不正”。ここでは Network エラーで包む
                return Err(Error::Network(format!(
                    "invalid http response {}",
                    preprocessed_response
                )));
            }
        };

        // 2) ヘッダ部とボディ部を空行(\n\n)で分割
        //    - 本来は CRLF+CRLF が正規の区切り。ここでは 0) で正規化済みなので LF で探します。
        let (headers, body) = match remaining.split_once("\n\n") {
            Some((h, b)) => {
                let mut headers = Vec::new();
                for header in h.split("\n") {
                    // ヘッダ1行 "Name: Value" を name/value に分割
                    // `splitn(2, ':')` は最初のコロンで1回だけ分割するので、値にコロンがあっても安全。
                    // 注意: `splitn` は“イテレータ”を返すので、そのままでは配列のように使えません。
                    // ここで `collect()` して `Vec<&str>`（可変長配列）に変換します。
                    // TS: `Array.from(header.split(...))`、Python: `list(header.split(...))` 的な変換です。
                    let splitted_header: Vec<&str> = header.splitn(2, ":").collect();
                    headers.push(Header::new(
                        // `String::from(..)` は `&str` → `String` へ“所有権のある文字列”を作る関数。
                        // Python: `str(s)`, TS: `String(s)`, Go: `s`（コピー）に近いイメージです。
                        String::from(splitted_header[0].trim()),
                        String::from(splitted_header[1].trim()),
                    ));
                }
                (headers, b)
            }
            None => (Vec::new(), remaining),
        };

        // 3) ステータスラインを空白で分解 → [バージョン, ステータスコード, 理由句]
        let statuses: Vec<&str> = status_line.split(' ').collect();

        // 4) ここがポイント: Ok(...) で "成功" として完成した値を返す
        // - `Self { ... }` は構造体リテラル: 各フィールドに値を詰めて1つの値を作る
        // - それを `Ok( ... )` に包むと「成功の Result」として返る
        // - `status_code` は `parse()` で文字列→数値へ。失敗時は `unwrap_or(404)` で既定値に落とす例。
        Ok(Self {
            version: statuses[0].to_string(), // 例: "HTTP/1.1"
            status_code: statuses[1].parse().unwrap_or(404),
            reason: statuses[2].to_string(), // 例: "OK"
            // ヘッダは `(name, value)` のベクタとして保持。検索や加工がしやすい形です。
            headers,
            body: body.to_string(),
        })
    }

    // 以下は“ゲッター”。所有権を保ったまま呼び出し側に値を渡すため、clone で複製しています。
    pub fn version(&self) -> String {
        // `clone()` は“同じ中身を新しく作って返す”。
        // TS/Python/Goの文字列は不変（immutalbe）なのでコピーの概念に近いです。
        // Rustでは所有権を維持したまま呼び出し側へ渡すために、明示的に複製します。
        self.version.clone()
    }

    pub fn status_code(&self) -> u32 {
        self.status_code
    }

    pub fn reason(&self) -> String {
        // ここも同様に複製して返します（所有権を移動させない）。
        self.reason.clone()
    }

    pub fn headers(&self) -> Vec<Header> {
        // 配列（Vec）も `.clone()` で要素ごと複製されます（各要素の `Clone` が必要）。
        // 参照で返す設計も可能ですが、学習用にシンプルさを優先しています。
        self.headers.clone()
    }

    pub fn body(&self) -> String {
        self.body.clone()
    }

    // 単一ヘッダの検索。線形探索（O(n)）ですが、少数のヘッダなら十分です。
    // 大量のヘッダで効率化したい場合は、構築時に `BTreeMap`/`HashMap` を使う案もあります。
    pub fn header_value(&self, name: &str) -> Result<String, String> {
        for h in &self.headers {
            if h.name == name {
                return Ok(h.value.clone());
            }
        }

        Err(format!("failed to find {} in headers", name))
    }
}

impl Header {
    // 失敗しない“コンストラクタ”。完成した構造体をそのまま返します。
    // TS/Python なら「ただの初期化関数」に相当します。
    pub fn new(name: String, value: String) -> Self {
        Self { name, value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // テスト全体の目的
    // - 生の HTTP 文字列を `HttpResponse::new` に渡したとき、想定どおりに
    //   フィールドへ分解できるかを小さなケースで確認します。
    // - それぞれのテストは「準備 → 実行 → 検証(assert)」の3段構成です。
    // - `assert_eq!(left, right)` は left == right を期待するアサーションです。

    #[test]
    fn test_invalid() {
        // 準備: ヘッダ区切りや空行が無い“未完成”レスポンス
        let raw = "HTTP/1.1 200 OK".to_string();
        // 実行/検証: パースに失敗するはず（Result が Err になる）
        assert!(HttpResponse::new(raw).is_err());
    }

    #[test]
    fn test_status_line_only() {
        // 準備: ステータスライン + 空行（= ヘッダ無し・ボディ無しの最小形）
        let raw = "HTTP/1.1 200 OK\n\n".to_string();
        // 実行: Ok(...) を期待するので `expect` で取り出す。
        // `expect(..)` は Err の場合にメッセージ付きでテスト失敗にします（TS/Python/Go の assert に近い体験）。
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        // 検証: バージョン/コード/理由句が正しく取れること
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");
    }

    #[test]
    fn test_one_header() {
        // 準備: コロンの直後に空白が無いヘッダ（"Date:xx xx xx"）を含むケース。
        // `splitn(2, ':')` + `trim()` でスペース有無の差を吸収できることを確認します。
        let raw = "HTTP/1.1 200 OK\nDate:xx xx xx\n\n".to_string();
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        // 検証: ステータス系
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");
        // 検証: `splitn(2, ':')` と `trim()` により値が正しく取れる
        assert_eq!(res.header_value("Date"), Ok("xx xx xx".to_string()));
    }

    #[test]
    fn test_two_headers_with_white_space() {
        // 準備: 空白の有無が混在する2つのヘッダ（前後の空白を trim で吸収できるか）
        let raw = "HTTP/1.1 200 OK\nDate: xx xx xx\nContent-Length: 42\n\n".to_string();
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        // 検証: ステータス系
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");
        // 検証: それぞれのヘッダ値が期待どおり
        assert_eq!(res.header_value("Date"), Ok("xx xx xx".to_string()));
        assert_eq!(res.header_value("Content-Length"), Ok("42".to_string()));
    }

    #[test]
    fn test_body() {
        // 準備: 本文（ボディ）付きのレスポンス
        let raw = "HTTP/1.1 200 OK\nDate: xx xx xx\n\nbody message".to_string();
        let res = HttpResponse::new(raw).expect("failed to parse http response");
        // 検証: ステータス系
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.reason(), "OK");
        // 検証: ヘッダ/本文の抽出
        assert_eq!(res.header_value("Date"), Ok("xx xx xx".to_string()));
        assert_eq!(res.body(), "body message".to_string());
    }
}
