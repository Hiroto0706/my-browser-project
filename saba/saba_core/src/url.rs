//! saba_core::url — とてもシンプルな HTTP 用 URL パーサ（初心者向け）
//!
//! なにをする？
//! - `http://host[:port][/path][?query]` 形式の文字列から、`host` / `port` / `path` /
//!   `searchpart`（クエリ文字列）を取り出して `Url` 構造体に入れます。
//! - ネットワーク I/O は行いません。純粋に“文字列の分解”だけです。
//!
//! 言語ブリッジ（TS / Python / Go の感覚）
//! - `String` は“所有する文字列”。`&str`（借用）から `String::from(..)` や `to_string()` で作ります。
//! - `splitn(2, sep)` は「最初の区切りで1回だけ分割」。TS: `str.split(sep, 2)`、Python: `str.split(sep, 1)`、Go: `strings.SplitN(str, sep, 2)`。
//! - `trim_start_matches("http://")` は先頭の `http://` を1回だけ取り除きます。
//! - 返り値 `Result<Self, String>` は、成功= `Ok(url)` / 失敗= `Err(メッセージ)` の形です。
//!
//! 注意
//! - 学習用の最小実装です。`https` や `#fragment`、パーセントエンコードなどは扱いません。
//! - 本番用途では公式の `url` crate などの利用を検討してください。

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub struct Url {
    url: String,        // 元のURL文字列（例: "http://example.com:8888/index.html?a=1"）
    host: String,       // 例: "example.com"
    port: String,       // 例: "80" / "8888"
    path: String,       // 例: "index.html"（先頭のスラッシュは除いた形）
    searchpart: String, // 例: "a=1&b=2"（? の後ろ全体）
}

impl Url {
    // いわゆる“コンストラクタ”。まだ分解はせず、空のフィールドで作ります。
    pub fn new(url: String) -> Self {
        Self {
            url,
            host: "".to_string(),
            port: "".to_string(),
            path: "".to_string(),
            searchpart: "".to_string(),
        }
    }

    // 以下はゲッター。所有権を保つため clone で複製を返しています。
    pub fn host(&self) -> String {
        self.host.clone()
    }

    pub fn port(&self) -> String {
        self.port.clone()
    }

    pub fn path(&self) -> String {
        self.path.clone()
    }

    pub fn searchpart(&self) -> String {
        self.searchpart.clone()
    }

    // スキームが http:// かを確認します。
    fn is_http(&self) -> bool {
        if self.url.contains("http://") {
            return true;
        }
        false
    }

    // host[:port]/path?... の先頭部分から `host` だけを取り出します。
    fn extract_host(&self) -> String {
        let url_parts: Vec<&str> = self
            .url
            .trim_start_matches("http://")
            .splitn(2, "/")
            .collect();

        if let Some(index) = url_parts[0].find(':') {
            url_parts[0][..index].to_string()
        } else {
            url_parts[0].to_string()
        }
    }

    // host[:port]/path?... から `path` 部分（? より前）を取り出します。
    fn extract_path(&self) -> String {
        let url_parts: Vec<&str> = self
            .url
            .trim_start_matches("http://")
            .splitn(2, "/")
            .collect();

        if url_parts.len() < 2 {
            return "".to_string();
        }

        // 右側（スラッシュ以降）を `?` で最初の1回だけ分割
        let path_and_searchpart: Vec<&str> = url_parts[1].splitn(2, "?").collect();
        path_and_searchpart[0].to_string() // (d3)
    }

    // host[:port]/... の先頭から、コロン以降があれば `port` として取り出し、なければ "80"。
    fn extract_port(&self) -> String {
        let url_parts: Vec<&str> = self
            .url
            .trim_start_matches("http://")
            .splitn(2, "/")
            .collect();

        if let Some(index) = url_parts[0].find(':') {
            url_parts[0][index + 1..].to_string()
        } else {
            "80".to_string()
        }
    }

    // ? の後ろ全体を `searchpart` として取り出します。なければ空文字。
    fn extract_searchpart(&self) -> String {
        let url_parts: Vec<&str> = self
            .url
            .trim_start_matches("http://")
            .splitn(2, "/")
            .collect();

        if url_parts.len() < 2 {
            return "".to_string();
        }

        let path_and_searchpart: Vec<&str> = url_parts[1].splitn(2, "?").collect();
        if path_and_searchpart.len() < 2 {
            "".to_string()
        } else {
            path_and_searchpart[1].to_string()
        }
    }

    // ここが“本体”。スキームを確認し、各フィールドを抽出して `Ok(self.clone())` を返します。
    // 失敗（http:// 以外）なら `Err("Only HTTP scheme is supported.")`。
    pub fn parse(&mut self) -> Result<Self, String> {
        if !self.is_http() {
            return Err("Only HTTP scheme is supported.".to_string());
        }

        self.host = self.extract_host();
        self.port = self.extract_port();
        self.path = self.extract_path();
        self.searchpart = self.extract_searchpart();

        Ok(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_host() {
        // 入力: ホストのみ
        let url = "http://example.com".to_string();
        let expected = Ok(Url {
            url: url.clone(),
            host: "example.com".to_string(),
            port: "80".to_string(),
            path: "".to_string(),
            searchpart: "".to_string(),
        });
        assert_eq!(expected, Url::new(url).parse());
    }

    #[test]
    fn test_url_host_port() {
        // 入力: ホスト + ポート
        let url = "http://example.com:8888".to_string();
        let expected = Ok(Url {
            url: url.clone(),
            host: "example.com".to_string(),
            port: "8888".to_string(),
            path: "".to_string(),
            searchpart: "".to_string(),
        });
        assert_eq!(expected, Url::new(url).parse());
    }

    #[test]
    fn test_url_host_port_path() {
        // 入力: ホスト + ポート + パス
        let url = "http://example.com:8888/index.html".to_string();
        let expected = Ok(Url {
            url: url.clone(),
            host: "example.com".to_string(),
            port: "8888".to_string(),
            path: "index.html".to_string(),
            searchpart: "".to_string(),
        });
        assert_eq!(expected, Url::new(url).parse());
    }

    #[test]
    fn test_url_host_path() {
        // 入力: ホスト + パス（ポート指定なし → 既定 80）
        let url = "http://example.com/index.html".to_string();
        let expected = Ok(Url {
            url: url.clone(),
            host: "example.com".to_string(),
            port: "80".to_string(),
            path: "index.html".to_string(),
            searchpart: "".to_string(),
        });
        assert_eq!(expected, Url::new(url).parse());
    }

    #[test]
    fn test_url_host_port_path_searchpart() {
        // 入力: ホスト + ポート + パス + クエリ（searchpart）
        let url = "http://example.com:8888/index.html?a=123&b=456".to_string();
        let expected = Ok(Url {
            url: url.clone(),
            host: "example.com".to_string(),
            port: "8888".to_string(),
            path: "index.html".to_string(),
            searchpart: "a=123&b=456".to_string(),
        });
        assert_eq!(expected, Url::new(url).parse());
    }

    #[test]
    fn test_no_scheme() {
        // エラー: スキームなし（http:// が必須）
        let url = "example.com".to_string();
        let expected = Err("Only HTTP scheme is supported.".to_string());
        assert_eq!(expected, Url::new(url).parse());
    }

    #[test]
    fn test_unsupported_scheme() {
        // エラー: https は未対応（学習用のため http のみ対応）
        let url = "https://example.com:8888/index.html".to_string();
        let expected = Err("Only HTTP scheme is supported.".to_string());
        assert_eq!(expected, Url::new(url).parse());
    }
}
