//! URL 型と、URL を扱うための最小限のデータ構造。
//!
//! 初学者向けメモ（TypeScript / Python に例えると）
//! - ここで定義する `Url` は、TS の `interface Url { ... }` や
//!   Python の `@dataclass` 的な「データの入れ物」です。
//! - フィールドは `String`（ヒープ確保される文字列型）。`no_std` 環境なので、
//!   `alloc::string::String` を使います（TS/Python の `string/str` に相当）。
//! - まずは型だけを定義し、パーサやバリデータは今後 `impl Url` や別モジュールに
//!   追加していく前提です。

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
/// URL の分解結果を保持する構造体。
/// - いまは単純化のため、すべて文字列で保持しています。
/// - 厳密な仕様準拠（RFC 3986 等）は今後の拡張で対応します。
pub struct Url {
    /// 元の入力文字列（例: "https://example.com:8080/path?key=val"）
    url: String,
    /// ホスト名（例: "example.com"）。IPv4/IPv6 も今後ここに入れる想定。
    host: String,
    /// ポート番号（例: "8080"）。未指定なら空文字列にするなど運用で決めます。
    port: String,
    /// パス（例: "/path"）。ルートのみなら "/"。
    path: String,
    /// クエリ文字列やフラグメントなどの「検索部分」。
    /// 例: "?key=val#section"（実際に含める範囲は仕様決めに合わせて調整）。
    searchpart: String,
}

impl Url {
    /// コンストラクタ的メソッド。
    ///
    /// TS/Python のイメージ:
    /// - TS: `new Url(url: string)`
    /// - Py: `Url(url: str)`
    ///
    /// Rust では `String` の所有権をこの構造体に「ムーブ」します。
    /// 呼び出し側は `String::from("...")` や `"...".to_string()` で作った
    /// 文字列を渡してください（`&str` を受け取る版を追加することも可能）。
    pub fn new(url: String) -> Self {
        Self {
            url,
            host: "".to_string(),
            port: "".to_string(),
            path: "".to_string(),
            searchpart: "".to_string(),
        }
    }

    /// Getter: ホスト名を返します。
    /// 注意: ここでは `String` をクローンして返しています（所有権を移すため）。
    /// 性能目的で借用参照を返すなら `&str` を返す API にする選択肢もあります。
    pub fn host(&self) -> String {
        self.host.clone()
    }

    /// Getter: ポート番号を返します。未指定のときは `"80"` を採用しています。
    pub fn port(&self) -> String {
        self.port.clone()
    }

    /// Getter: パス（例: "/path"）。未指定なら空文字。
    pub fn path(&self) -> String {
        self.path.clone()
    }

    /// Getter: クエリやフラグメントを含む「検索部分」。例: `"key=val#section"`。
    pub fn searchpart(&self) -> String {
        self.searchpart.clone()
    }

    /// スキームが `http://` かどうかの超簡易チェック。
    /// ここでは `https://` には対応していません（練習用の最小実装）。
    fn is_http(&self) -> bool {
        if self.url.contains("http://") {
            return true;
        }
        false
    }

    /// ホスト部分を取り出す。
    /// 手順（ざっくり）:
    /// 1) 先頭の `https://` を取り除く
    /// 2) 最初の `/` までをドメイン＋ポートとみなす
    /// 3) `:` があれば、その前半をホストとする
    fn extract_host(&self) -> String {
        let url_parts: Vec<&str> = self
            .url
            .trim_start_matches("http://")
            .splitn(2, "/")
            .collect();

        if let Some(index) = url_parts[0].find(":") {
            url_parts[0][..index].to_string()
        } else {
            url_parts[0].to_string()
        }
    }

    /// ポート番号を取り出す。なければ既定値 `80`。
    /// 例: `http://example.com:8080/x` → `8080`
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

    /// パス部分を取り出す。`?` より前の部分がパス。
    /// 例: `/a/b?x=1` → `/a/b`
    fn extract_path(&self) -> String {
        let url_parts: Vec<&str> = self
            .url
            .trim_start_matches("http://")
            .splitn(2, "/")
            .collect();

        if url_parts.len() < 2 {
            return "".to_string();
        }

        let path_and_searchpart: Vec<&str> = url_parts[1].splitn(2, "?").collect();
        path_and_searchpart[0].to_string()
    }

    /// クエリ（と将来はフラグメントも）からなる「検索部分」を取り出す。
    /// 例: `/a?x=1` → `x=1`（先頭の `?` は除外する設計にしています）
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

    /// URL を分解して各フィールドへ格納します。
    ///
    /// 戻り値の `Result<Self, String>` は、TS の `Result<T, E>` 型や
    /// Python の `try/except` に相当するエラーハンドリングの仕組みです。
    /// - 成功: `Ok(Url)`
    /// - 失敗: `Err("エラー内容")`
    /// 呼び出し側では `?` 演算子で自然に伝播できます（Py の `raise` に近い感覚）。
    pub fn parse(&mut self) -> Result<Self, String> {
        if !self.is_http() {
            return Err("Only HTTP scheme is supported.".to_string());
        }

        self.host = self.extract_host();
        self.port = self.extract_port();
        self.path = self.extract_path();
        self.searchpart = self.extract_searchpart();

        // 解析後の自分自身のコピーを返す（所有権の都合上、いったん clone しています）。
        // メモ: API 設計としては `&self` で新しい構造体を返す純関数的な `fn parsed(&self) -> Url`
        // などにする手もあります。
        Ok(self.clone())
    }
}

#[cfg(test)]
mod tests {
    // この `tests` モジュールは「親モジュール（= このファイルの実装側）」とは
    // 別スコープになります。そこで、親の公開アイテム（`pub struct Url` など）を
    // 使えるようにするために `use super::*;` を書きます。
    //
    // 解説（TS/Pythonの比喩）:
    // - TS: `import * as parent from ".."` して親の公開メンバーを使うイメージ。
    // - Python: `from .. import *` に近い（ただし Rust では静的に解決）。
    // - `super` は「1つ上のモジュール階層」を指します（`crate` ルートではなく親）。
    use super::*;

    // Arrange-Act-Assert で最小ケースを検証: ホストのみ
    #[test]
    fn test_url_host() {
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

    // ホスト + ポート（デフォルト80以外）
    #[test]
    fn test_url_host_port() {
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

    // ホスト + ポート + パス
    #[test]
    fn test_url_host_port_path() {
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

    // ホスト + パス（ポート省略 → 既定値 80）
    #[test]
    fn test_url_host_path() {
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

    // ホスト + ポート + パス + クエリ（`?` は除外した形で保持）
    #[test]
    fn test_url_host_port_path_searchpart() {
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

    // スキームなしはエラー。
    // 注意: 実装側のメッセージは "Only HTTP schema is supported."（schema）です。
    // テストと合わせるにはどちらかを統一してください。
    #[test]
    fn test_no_scheme() {
        let url = "example.com".to_string();
        let expected = Err("Only HTTP scheme is supported.".to_string());
        assert_eq!(expected, Url::new(url).parse());
    }

    // https は未サポート → エラー
    // 上記と同様、エラーメッセージの表記（scheme/schema）は統一が必要です。
    #[test]
    fn test_unsupported_scheme() {
        let url = "https://example.com:8888/index.html".to_string();
        let expected = Err("Only HTTP scheme is supported.".to_string());
        assert_eq!(expected, Url::new(url).parse());
    }
}
