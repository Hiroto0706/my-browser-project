use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// https://262.ecma-international.org/#sec-punctuators
    Punctuator(char),
    /// https://262.ecma-international.org/#sec-literals-numeric-literals
    Number(u64),
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

        let c = self.input[self.pos];

        let token = match c {
            '+' | '-' | ';' | '=' | '(' | ')' | '{' | '}' | ',' | '.' => {
                let t = Token::Punctuator(c);
                self.pos += 1;
                t
            }
            '0'..='9' => Token::Number(self.consume_number()),
            _ => unimplemented!("char {:?} is not supported yet", c),
        };

        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
