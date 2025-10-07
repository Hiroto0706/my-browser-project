use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// 予約語の簡易リスト（学習用）
/// - 仕様のごく一部だけを対象にしたサンプルです。
/// - ここに載っている単語は識別子としてではなく“キーワード”として扱いたいときに使います。
/// - 大文字小文字は区別します（`return` は一致、`Return` は不一致）。
/// - 本実装は最小限なので、網羅的な予約語チェックは行いません。
static RESERVED_WORDS: [&str; 3] = ["var", "function", "return"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// https://262.ecma-international.org/#sec-punctuators
    Punctuator(char),
    /// https://262.ecma-international.org/#sec-literals-numeric-literals
    Number(u64),
    /// https://262.ecma-international.org/#sec-identifier-names
    Identifier(String),
    /// https://262.ecma-international.org/#sec-keywords-and-reserved-words
    Keyword(String),
    /// https://262.ecma-international.org/#sec-literals-string-literals
    StringLiteral(String),
}

pub struct JsLexer {
    pos: usize,
    input: Vec<char>,
}

impl JsLexer {
    pub fn new(js: String) -> Self {
        Self {
            pos: 0,
            input: js.chars().collect(),
        }
    }

    /// 現在位置 `self.pos` から始まる入力が `keyword` で“文字通り始まっているか”を確認します。
    ///
    /// 例
    /// - 入力: "return x"、`pos=0`、`keyword="return"` → `true`
    /// - 入力: "ret"、`pos=0`、`keyword="return"` → 未定義動作（範囲外アクセスの可能性）
    ///
    /// 注意（重要）
    /// - 長さチェックをしていないため、残り入力が `keyword.len()` 未満だと配列アクセスでパニックします。
    ///   実運用では事前に長さを確認してから呼び出すか、安全な比較関数に置き換えてください。
    /// - “接頭辞”だけを比較するため、`returnX` に対しても `keyword="return"` は `true` になります。
    ///   本格実装では“単語境界（英数字・アンダースコア以外）”の確認が必要です。
    fn contains(&self, keyword: &str) -> bool {
        for i in 0..keyword.len() {
            if keyword
                .chars()
                .nth(i)
                .expect("failed to access to i-th char")
                != self.input[self.pos + i]
            {
                return false;
            }
        }

        true
    }

    /// 現在位置から予約語表（`RESERVED_WORDS`）のどれかで始まっていれば、その単語を返します。
    /// 一致しなければ `None`。
    ///
    /// 仕様の簡略化ポイント
    /// - 接頭辞一致のみで判定しており、単語境界は検証しません（`returnX` でも `return` と一致）。
    /// - パフォーマンスのためには長い単語から試す、Trie を使う、などの工夫が考えられますが、
    ///   学習用のため単純な線形探索にしています。
    fn check_reserved_word(&self) -> Option<String> {
        for word in RESERVED_WORDS {
            if self.contains(word) {
                return Some(word.to_string());
            }
        }

        None
    }

    /// 現在位置から“識別子”を読み切って返します（学習用の簡易版）。
    ///
    /// 仕様（この実装でのルール）
    /// - 英数字（`A-Z`/`a-z`/`0-9`）、アンダースコア（`_`）、ドル記号（`$`）の連続を 1 つの識別子とみなす。
    /// - それ以外の文字に当たったら読み取りを終了し、これまでの文字列を返す。
    /// - 先頭文字の区別（英字または `_`/`$` から始まる等）は“していません”。
    ///   本格実装では「先頭は英字/`_`/`$`、以降は英数字/`_`/`$`」のようなルールや Unicode も考慮します。
    /// - 位置 `self.pos` は読み取った分だけ前進します。
    ///
    /// 例
    /// - 入力: `foo123+1`（`pos=0`） → 戻り値 `"foo123"`、終了後 `pos` は `'+'` を指す
    /// - 入力: `9lives`（`pos=0`） → 戻り値 `"9lives"`（先頭が数字でも許容する簡易仕様）
    fn consume_identifier(&mut self) -> String {
        let mut result = String::new();

        loop {
            // 入力末尾に到達したら終了
            if self.pos >= self.input.len() {
                return result;
            }

            // 許可する文字: 英数字 / '_' / '$'
            if self.input[self.pos].is_ascii_alphanumeric()
                || self.input[self.pos] == '_'
                || self.input[self.pos] == '$'
            {
                // 1 文字追加して、読み位置を進める
                result.push(self.input[self.pos]);
                self.pos += 1;
            } else {
                // 許可外の文字に当たったので、ここまでを識別子として返す
                return result;
            }
        }
    }

    /// 現在位置が開き二重引用符（`"`）にある前提で、次の閉じ `"` までを文字列として読み取ります。
    ///
    /// ポインタの動き（概念）
    /// - 呼び出し時点: `pos` は開き `"` を指している
    /// - 関数冒頭で `pos += 1`（開き `"` をスキップ）
    /// - ループで次文字を見ながら、閉じ `"` までを `result` に push
    /// - 閉じ `"` に当たったらそれも消費して終了（`pos` は閉じの“次”を指す）
    /// - EOF に達したら、その時点までの `result` を返す（未閉じでもクラッシュしない簡易仕様）
    ///
    /// 制限（簡易実装のため）
    /// - エスケープ（`\"`, `\n` など）や `\u{..}` は未処理。実装する場合は `\` を見たら次文字の扱いを
    ///   分岐させる必要があります。
    /// - 単引用符（`'...'`）は別関数で扱う前提です（必要なら同様のロジックを追加）。
    fn consume_string(&mut self) -> String {
        let mut result = String::new();
        // 開きの `"` を読み飛ばす
        self.pos += 1;

        loop {
            // 入力末尾なら未閉じのまま終了（学習用の緩い仕様）
            if self.pos >= self.input.len() {
                return result;
            }

            // 閉じの `"` に到達したら、それも消費して終了
            if self.input[self.pos] == '"' {
                self.pos += 1;
                return result;
            }

            // 通常文字を蓄積して前進
            result.push(self.input[self.pos]);
            self.pos += 1;
        }
    }

    /// 連続する10進数字を読み取り、`u64` の値にして返す（簡易版）
    ///
    /// 仕様（学習用に簡略化）
    /// - 先頭位置 `self.pos` から `'0'..='9'` の間の文字をできるだけ読む。
    /// - 各桁を 10 倍 + 加算で組み立て、最初に数字以外が来た時点で停止。
    /// - 非数字は消費せず（`self.pos` は非数字の位置で止まる）。
    /// - 入力末尾に達したら、その時点の数値を返す。
    ///
    /// 例
    /// - 入力: "123;" で `pos=0` → 戻り値 `123`、終了後の `pos` は `';'` を指す（非数字は未消費）。
    /// - 入力: "  42" で `pos` が空白なら、呼び出し側で空白スキップ後に使う想定。
    ///
    /// 注意
    /// - 桁あふれ（`u64::MAX` 超え）は未検出。実運用なら `checked_mul`/`checked_add` 等で検知を推奨。
    /// - 先頭に符号（`+`/`-`）や 16 進/浮動小数などは未対応（純粋な 10 進整数のみ）。
    fn consume_number(&mut self) -> u64 {
        let mut num = 0;

        loop {
            if self.pos >= self.input.len() {
                return num; // 入力末尾に到達
            }

            let c = self.input[self.pos];

            match c {
                '0'..='9' => {
                    // `to_digit(10)` は '0'..'9' のとき 0..9 を返す（上のマッチで保証）
                    num = num * 10 + (c.to_digit(10).unwrap() as u64);
                    self.pos += 1; // 1 文字前進
                }
                _ => break, // 非数字に遭遇したら打ち切り（非数字は未消費）
            }
        }

        return num;
    }
}

impl Iterator for JsLexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.input.len() {
            return None;
        }

        // ホワイトスペースまたは改行文字が続く限り、次の位置に進める
        while self.input[self.pos] == ' ' || self.input[self.pos] == '\n' {
            self.pos += 1;

            if self.pos >= self.input.len() {
                return None;
            }
        }

        // 予約語が現れたら、Keywordトークンを返す
        if let Some(keyword) = self.check_reserved_word() {
            self.pos += keyword.len();
            let token = Some(Token::Keyword(keyword));
            return token;
        }

        let c = self.input[self.pos];

        let token = match c {
            '+' | '-' | ';' | '=' | '(' | ')' | '{' | '}' | ',' | '.' => {
                let t = Token::Punctuator(c);
                self.pos += 1;
                t
            }
            '0'..='9' => Token::Number(self.consume_number()),
            'a'..='z' | 'A'..='Z' | '_' | '$' => Token::Identifier(self.consume_identifier()),
            '"' => Token::StringLiteral(self.consume_string()),
            _ => unimplemented!("char {:?} is not supported yet", c),
        };

        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // このモジュールの目的
    // - JS 風トークナイザの最小機能を確認します。
    // - 基本パターン: `peek()` で終端チェック → `next()` で 1 トークン消費。
    // - 空白と改行はスキップされ、トークン列には現れません。

    // 空入力を与えたときにトークンが一切出てこないことを確認するテスト
    // - `peekable()` を使うと、次のトークンを消費せずに覗けます（反復終了の判定に便利）。
    #[test]
    fn test_empty() {
        let input = "".to_string();
        let mut lexer = JsLexer::new(input).peekable();
        assert!(lexer.peek().is_none());
    }

    // 数字のみの入力を 1 個の数値トークンとして読めることを確認
    // - 期待値: [Number(42)]
    // - ループで `peek()` が Some の間だけ `next()` で順に消費し、最後に None になることも確認
    #[test]
    fn test_num() {
        let input = "42".to_string();
        let mut lexer = JsLexer::new(input).peekable();
        let expected = [Token::Number(42)].to_vec();
        let mut i = 0;
        while lexer.peek().is_some() {
            assert_eq!(Some(expected[i].clone()), lexer.next());
            i += 1;
        }
        assert!(lexer.peek().is_none());
    }

    // 足し算のような式: "1 + 2" をトークン列 [Number(1), '+', Number(2)] に分解できることを確認
    // - 空白はスキップされる実装のため、トークンには含まれません
    // - `peek()` で終端チェック → `next()` で消費 の繰り返しパターンを使用
    #[test]
    fn test_add_nums() {
        let input = "1 + 2".to_string();
        let mut lexer = JsLexer::new(input).peekable();
        let expected = [Token::Number(1), Token::Punctuator('+'), Token::Number(2)].to_vec();
        let mut i = 0;
        while lexer.peek().is_some() {
            assert_eq!(Some(expected[i].clone()), lexer.next());
            i += 1;
        }
        assert!(lexer.peek().is_none());
    }

    #[test]
    fn test_assign_variable() {
        // 代入: var foo = "bar";
        // - RESERVED_WORDS により "var" は Keyword として読まれる
        // - 識別子 "foo"、記号 '='、文字列リテラル "bar"、セミコロン ';'
        let input = "var foo=\"bar\";".to_string();
        let mut lexer = JsLexer::new(input).peekable();
        let expected = [
            Token::Keyword("var".to_string()),
            Token::Identifier("foo".to_string()),
            Token::Punctuator('='),
            Token::StringLiteral("bar".to_string()),
            Token::Punctuator(';'),
        ]
        .to_vec();
        let mut i = 0;
        while lexer.peek().is_some() {
            assert_eq!(Some(expected[i].clone()), lexer.next());
            i += 1;
        }
        assert!(lexer.peek().is_none());
    }

    #[test]
    fn test_add_variable_and_num() {
        // 2 文を連結: "var foo=42; var result=foo+1;"
        // - 1 文目: var, foo, '=', 42, ';'
        // - 2 文目: var, result, '=', foo, '+', 1, ';'
        // - 識別子と数値・演算子が交互に現れることを確認
        let input = "var foo=42; var result=foo+1;".to_string();
        let mut lexer = JsLexer::new(input).peekable();
        let expected = [
            Token::Keyword("var".to_string()),
            Token::Identifier("foo".to_string()),
            Token::Punctuator('='),
            Token::Number(42),
            Token::Punctuator(';'),
            Token::Keyword("var".to_string()),
            Token::Identifier("result".to_string()),
            Token::Punctuator('='),
            Token::Identifier("foo".to_string()),
            Token::Punctuator('+'),
            Token::Number(1),
            Token::Punctuator(';'),
        ]
        .to_vec();
        let mut i = 0;
        while lexer.peek().is_some() {
            assert_eq!(Some(expected[i].clone()), lexer.next());
            i += 1;
        }
        assert!(lexer.peek().is_none());
    }

    #[test]
    fn test_add_local_variable_and_num() {
        // 関数定義 + 呼び出し: "function foo() { var a=42; return a; } var result = foo() + 1;"
        // - function/var/return は Keyword
        // - 括弧 () と波括弧 {} は Punctuator
        // - 後半で関数呼び出し `foo()` のあとに `+ 1` が続く
        let input = "function foo() { var a=42; return a; } var result = foo() + 1;".to_string();
        let mut lexer = JsLexer::new(input).peekable();
        let expected = [
            Token::Keyword("function".to_string()),
            Token::Identifier("foo".to_string()),
            Token::Punctuator('('),
            Token::Punctuator(')'),
            Token::Punctuator('{'),
            Token::Keyword("var".to_string()),
            Token::Identifier("a".to_string()),
            Token::Punctuator('='),
            Token::Number(42),
            Token::Punctuator(';'),
            Token::Keyword("return".to_string()),
            Token::Identifier("a".to_string()),
            Token::Punctuator(';'),
            Token::Punctuator('}'),
            Token::Keyword("var".to_string()),
            Token::Identifier("result".to_string()),
            Token::Punctuator('='),
            Token::Identifier("foo".to_string()),
            Token::Punctuator('('),
            Token::Punctuator(')'),
            Token::Punctuator('+'),
            Token::Number(1),
            Token::Punctuator(';'),
        ]
        .to_vec();
        let mut i = 0;
        while lexer.peek().is_some() {
            assert_eq!(Some(expected[i].clone()), lexer.next());
            i += 1;
        }
        assert!(lexer.peek().is_none());
    }
}
