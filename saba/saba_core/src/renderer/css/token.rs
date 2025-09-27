//! CSS トークナイザ（初心者向け）
//!
//! 目的
//! - CSS の文字列を“トークン”（部品）に分解します。例: `color: #fff;` → `Ident("color")`, `Colon`,
//!   `HashToken("fff")`, `SemiColon`。
//! - ここで得たトークン列は、このあとセレクタ/宣言ブロックなどのパーサに渡されます。
//!
//! 言語ブリッジ（TS / Python / Go）
//! - `Iterator` 実装により、`next()` で1トークンずつ取り出せます（TS の `for..of`、Python の `for x in it`）。
//! - `String`/`Vec<char>` は `alloc` のヒープ型（no_std なので明示利用）。
//! - 仕様リンク（W3C）を各トークン定義に添えていますが、実装は学習用に単純化しています。
//!
//! トークナイズの流れ（ざっくり）
//! 1) 1文字読む → 種別を判定（記号/数字/識別子/文字列/特殊）
//! 2) 必要なら `consume_xxx_token` で連続した塊を読み切る
//! 3) 1トークンを返す（`Some(token)`）
//! 4) 呼び出し側が `next()` を再度呼ぶ
//!
//! 簡易化している点（制約）
//! - コメント `/* ... */` のスキップやバックスラッシュエスケープは未対応。
//! - 数値の指数表記（1e3）や単位（px, em）は別フェーズで扱う前提。
//! - `.` 単体は Delim とし、`.5` のような先頭ドット数値は未対応。
//! - `-` 先頭の負数は未対応（`-` は識別子として扱う）。

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub enum CssToken {
    /// https://www.w3.org/TR/css-syntax-3/#typedef-hash-token
    /// 例: `#fff` → `HashToken("fff")`（本実装では常に ID/カラー風の文字列として扱う）
    HashToken(String),
    /// https://www.w3.org/TR/css-syntax-3/#typedef-delim-token
    /// 単独記号。例: `,` や `.` など。
    Delim(char),
    /// https://www.w3.org/TR/css-syntax-3/#typedef-number-token
    /// 数値。例: `12`, `0.5`。
    Number(f64),
    /// https://www.w3.org/TR/css-syntax-3/#typedef-colon-token
    /// `:`（プロパティ名と値の区切り）
    Colon,
    /// https://www.w3.org/TR/css-syntax-3/#typedef-semicolon-token
    /// `;`（宣言の終端）
    SemiColon,
    /// https://www.w3.org/TR/css-syntax-3/#tokendef-open-paren
    /// `(`（関数呼び出しや `rgb(` など）
    OpenParenthesis,
    /// https://www.w3.org/TR/css-syntax-3/#tokendef-close-paren
    /// `)`
    CloseParenthesis,
    /// https://www.w3.org/TR/css-syntax-3/#tokendef-open-curly
    /// `{`（宣言ブロック開始）
    OpenCurly,
    /// https://www.w3.org/TR/css-syntax-3/#tokendef-close-curly
    /// `}`（宣言ブロック終了）
    CloseCurly,
    /// https://www.w3.org/TR/css-syntax-3/#typedef-ident-token
    /// 識別子。例: `color`, `background`, `--var`（本実装は英数字/`-`/`_` 程度に限定）
    Ident(String),
    /// https://www.w3.org/TR/css-syntax-3/#typedef-string-token
    /// 引用符で囲まれた文字列。例: `"Helvetica"` → `StringToken("Helvetica")`
    StringToken(String),
    /// https://www.w3.org/TR/css-syntax-3/#typedef-at-keyword-token
    /// `@xxx`。例: `@media`, `@import` → `AtKeyword("media")` など
    AtKeyword(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CssTokenizer {
    pos: usize,       // 次に読む位置（0..len）
    input: Vec<char>, // 入力を1文字ずつに分割した配列（単純化のため事前に chars().collect()）
}

impl CssTokenizer {
    /// 文字列からトークナイザを作成します。
    /// - 内部では `chars().collect()` で `Vec<char>` を作り、`pos` を 0 から進めていきます。
    pub fn new(css: String) -> Self {
        Self {
            pos: 0,
            input: css.chars().collect(),
        }
    }

    // 再びダブルクオーテーションまたはシングルクォーテーションが現れるまで入力を文字として解釈する
    /// https://www.w3.org/TR/css-syntax-3/#consume-a-string-token
    /// 例: `"Helvetica"` → `"Helvetica"`（両端の引用は除いた内容を返す）
    /// 注意: この簡易実装では `\"` などのエスケープや改行・EOF の扱いを省略しています。
    fn consume_string_token(&mut self) -> String {
        let mut s = String::new();

        loop {
            if self.pos >= self.input.len() {
                return s;
            }

            // 現在の引用符の次の文字から走査を開始する想定
            self.pos += 1;
            let c = self.input[self.pos];
            match c {
                '"' | '\'' => break,
                _ => s.push(c),
            }
        }

        s
    }

    // 数字またはピリオドが出続けている間、数字として解釈する
    /// https://www.w3.org/TR/css-syntax-3/#consume-number
    /// https://www.w3.org/TR/css-syntax-3/#consume-a-numeric-token
    /// 例: `12.34` → 12.34
    /// 注意: `.5` のような先頭ドット数値や指数表記 `1e3` は未対応です。
    fn consume_numeric_token(&mut self) -> f64 {
        let mut num = 0f64;
        let mut floating = false;
        let mut floating_digit = 1f64;

        loop {
            if self.pos >= self.input.len() {
                return num;
            }

            let c = self.input[self.pos];

            match c {
                '0'..='9' => {
                    if floating {
                        floating_digit *= 1f64 / 10f64;
                        num += (c.to_digit(10).unwrap() as f64) * floating_digit
                    } else {
                        num = num * 10.0 + (c.to_digit(10).unwrap() as f64);
                    }
                    self.pos += 1;
                }
                '.' => {
                    floating = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }

        num
    }

    // 文字、数字、ハイフン、アンダースコアが出続けている間、識別子として扱う
    // それ以外の入力が出てきたら、今までの文字を返してメソッドを終了する
    /// https://www.w3.org/TR/css-syntax-3/#consume-ident-like-token
    /// https://www.w3.org/TR/css-syntax-3/#consume-name
    /// 例: `color` や `background-color` → `"color"`, `"background-color"`
    /// 注意: CSS の厳密な name-start / name-char 規則（非ASCII, escape など）は簡略化しています。
    fn consume_ident_token(&mut self) -> String {
        let mut s = String::new();
        s.push(self.input[self.pos]);

        loop {
            self.pos += 1;
            let c = self.input[self.pos];
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => {
                    s.push(c);
                }
                _ => break,
            }
        }

        s
    }
}

impl Iterator for CssTokenizer {
    type Item = CssToken;

    /// https://www.w3.org/TR/css-syntax-3/#consume-token
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // 0) 入力末尾ならイテレータ終了（None）。
            if self.pos >= self.input.len() {
                return None;
            }

            // 1) まだ pos は進めず、現在位置の1文字を見て種別を判定。
            let c = self.input[self.pos];

            // 2) 先頭文字に応じて分岐。必要なら consume_* で“塊”を読み切る。
            let token = match c {
                // 記号類: コロン、セミコロン、丸括弧、波括弧など
                // CSS では宣言の区切りや関数呼び出しの括弧に使われます。
                '(' => CssToken::OpenParenthesis,
                ')' => CssToken::CloseParenthesis,
                ',' => CssToken::Delim(','),
                '.' => {
                    // 簡易: 直後が数字でも `.5` を Number とせず Delim('.') とする。
                    // 対応したい場合はここで先読みして consume_numeric_token を呼ぶ分岐を追加します。
                    CssToken::Delim('.')
                }
                ':' => CssToken::Colon,
                ';' => CssToken::SemiColon,
                '{' => CssToken::OpenCurly,
                '}' => CssToken::CloseCurly,
                ' ' | '\n' => {
                    // 空白・改行は“トークン化せず”読み飛ばす。
                    self.pos += 1;
                    continue;
                }
                // 文字列: ダブル/シングルクォートで囲まれたもの
                '"' | '\'' => {
                    let value = self.consume_string_token();
                    CssToken::StringToken(value)
                }
                // 数値
                '0'..='9' => {
                    let t = CssToken::Number(self.consume_numeric_token());
                    // consume_* 内で pos を進めた分、末尾の pos += 1 と釣り合うように 1 戻す（帳尻合わせ）。
                    self.pos -= 1;
                    t
                }
                // #ID or 色コード風（本実装では単純化して識別子の連結）
                '#' => {
                    // 簡易版: 常に `#` に続く識別子を HashToken とする。
                    let value = self.consume_ident_token();
                    self.pos -= 1;
                    CssToken::HashToken(value)
                }
                // ハイフン始まりは識別子とみなす（負の数は扱わない前提）
                '-' => {
                    // 例: `--var` や `border-left` の先頭 `-`
                    let t = CssToken::Ident(self.consume_ident_token());
                    self.pos -= 1;
                    t
                }
                '@' => {
                    // `@media` / `@import` のような at-keyword を簡易判定。
                    // 3文字先までが英数字のとき AtKeyword とみなし、そうでなければ単独記号扱い。
                    // 本来は「@ の後に ident を読む」処理で十分ですが、
                    // 短絡評価の例として3文字先までの簡易チェックを入れています。
                    if self.input[self.pos + 1].is_ascii_alphabetic()
                        && self.input[self.pos + 2].is_alphanumeric()
                        && self.input[self.pos + 3].is_alphanumeric()
                    {
                        // skip '@'
                        self.pos += 1;
                        let t = CssToken::AtKeyword(self.consume_ident_token());
                        self.pos -= 1;
                        t
                    } else {
                        CssToken::Delim('@')
                    }
                }
                // 識別子（プロパティ名やキーワード）
                'a'..='z' | 'A'..='Z' | '_' => {
                    let t = CssToken::Ident(self.consume_ident_token());
                    self.pos -= 1;
                    t
                }
                _ => {
                    // 未対応の文字は学習用に unimplemented! で明示
                    unimplemented!("char {} is not supported yet", c);
                }
            };

            // 3) 1 トークン確定。読み位置を 1 進めて返す。
            self.pos += 1;
            // 呼び出し側は再度 next() を呼んで次トークンを取得する。
            return Some(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_empty() {
        // 入力が空文字のとき、最初の next() で None（トークン無し）になることを確認。
        let style = "".to_string();
        let mut t = CssTokenizer::new(style);
        assert!(t.next().is_none());
    }

    #[test]
    fn test_one_rule() {
        // 単一ルール: p { color: red; }
        // 期待トークン列（順に取り出せることを確認）
        let style = "p { color: red; }".to_string();
        let mut t = CssTokenizer::new(style);
        let expected = [
            CssToken::Ident("p".to_string()), // セレクタ（要素名）
            CssToken::OpenCurly,              // '{'
            CssToken::Ident("color".to_string()),
            CssToken::Colon, // ':'
            CssToken::Ident("red".to_string()),
            CssToken::SemiColon,  // ';'
            CssToken::CloseCurly, // '}'
        ];
        for e in expected {
            assert_eq!(Some(e.clone()), t.next());
        }
        // 取り尽くしたら None
        assert!(t.next().is_none());
    }

    #[test]
    fn test_id_selector() {
        // ID セレクタ: #id { color: red; }
        // 簡易実装では HashToken("#id") として“#を含んだまま”読ませています。
        let style = "#id { color: red; }".to_string();
        let mut t = CssTokenizer::new(style);
        let expected = [
            CssToken::HashToken("#id".to_string()),
            CssToken::OpenCurly,
            CssToken::Ident("color".to_string()),
            CssToken::Colon,
            CssToken::Ident("red".to_string()),
            CssToken::SemiColon,
            CssToken::CloseCurly,
        ];
        for e in expected {
            assert_eq!(Some(e.clone()), t.next());
        }
        assert!(t.next().is_none());
    }

    #[test]
    fn test_class_selector() {
        // クラスセレクタ: .class { color: red; }
        // 現状の簡易実装では先頭 '.' を Delim('.') として返し、続く "class" を Ident として返します。
        let style = ".class { color: red; }".to_string();
        let mut t = CssTokenizer::new(style);
        let expected = [
            CssToken::Delim('.'),
            CssToken::Ident("class".to_string()),
            CssToken::OpenCurly,
            CssToken::Ident("color".to_string()),
            CssToken::Colon,
            CssToken::Ident("red".to_string()),
            CssToken::SemiColon,
            CssToken::CloseCurly,
        ];
        for e in expected {
            assert_eq!(Some(e.clone()), t.next());
        }
        assert!(t.next().is_none());
    }

    #[test]
    fn test_multiple_rules() {
        // 複数ルールと混在する値の検証。
        // 入力: p { content: "Hey"; } h1 { font-size: 40; color: blue; }
        // - StringToken("Hey") は引用符付き文字列
        // - Number(40.0) は数値トークンとして読まれる（単位は未対応）
        let style = "p { content: \"Hey\"; } h1 { font-size: 40; color: blue; }".to_string();
        let mut t = CssTokenizer::new(style);
        let expected = [
            CssToken::Ident("p".to_string()),
            CssToken::OpenCurly,
            CssToken::Ident("content".to_string()),
            CssToken::Colon,
            CssToken::StringToken("Hey".to_string()),
            CssToken::SemiColon,
            CssToken::CloseCurly,
            CssToken::Ident("h1".to_string()),
            CssToken::OpenCurly,
            CssToken::Ident("font-size".to_string()),
            CssToken::Colon,
            CssToken::Number(40.0),
            CssToken::SemiColon,
            CssToken::Ident("color".to_string()),
            CssToken::Colon,
            CssToken::Ident("blue".to_string()),
            CssToken::SemiColon,
            CssToken::CloseCurly,
        ];
        for e in expected {
            assert_eq!(Some(e.clone()), t.next());
        }
        assert!(t.next().is_none());
    }
}
