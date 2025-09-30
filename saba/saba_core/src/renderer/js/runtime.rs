//! saba_core::renderer::js::runtime — とても小さな JS 実行器（初心者向け）
//!
//! 役割
//! - `JsParser` が作った AST（`Program` と `Node`）を辿って、数値演算の結果を計算します。
//! - 本実装は学習用の最小版で、数値（`u64`）と `+` / `-` のみをサポートします。
//!
//! TS / Python の感覚にたとえると
//! - `RuntimeValue` は「実行時の型の和集合」≈ TS の `number | string | ...`、Python の「値オブジェクト」。
//! - `eval(node)` は再帰評価。Python で `ast` を辿って計算するイメージです。
//! - `Rc<Node>` は参照カウント付きの共有ポインタ（所有権を気にせず複数箇所から参照）。
//!
//! 注意
//! - エラー処理は簡素化しています。未対応の構文は `None` を返すだけです。実運用では `Result<T, E>` で
//!   位置情報・原因などを返す設計を推奨します。
//! - `no_std` 前提のため、ヒープ確保は `alloc` クレート（`Rc` など）に依存します。

use crate::renderer::js::ast::Node;
use crate::renderer::js::ast::Program;
use alloc::rc::Rc;
use core::borrow::Borrow;
use core::ops::Add;
use core::ops::Sub;

/// https://262.ecma-international.org/#sec-ecmascript-language-types
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    /// https://262.ecma-international.org/#sec-numeric-types
    /// 数値（このミニ実装では `u64` のみ。負数や浮動小数は未対応）
    Number(u64),
}

impl Add<RuntimeValue> for RuntimeValue {
    type Output = RuntimeValue;

    fn add(self, rhs: RuntimeValue) -> RuntimeValue {
        // 学習用に「両辺が Number のときだけ」足し算する簡易版です。
        // 実装を簡単にするためパターンマッチで取り出しています。
        let (RuntimeValue::Number(left_num), RuntimeValue::Number(right_num)) = (&self, &rhs);
        return RuntimeValue::Number(left_num + right_num);
    }
}

impl Sub<RuntimeValue> for RuntimeValue {
    type Output = RuntimeValue;

    fn sub(self, rhs: RuntimeValue) -> RuntimeValue {
        // 減算も同様に Number 前提の簡易実装です。
        let (RuntimeValue::Number(left_num), RuntimeValue::Number(right_num)) = (&self, &rhs);
        return RuntimeValue::Number(left_num - right_num);
    }
}

/// JS の実行コンテキスト（超最小）
/// - いまは状態（変数環境など）を持ちません。将来 `env` や `this` などをここに追加できます。
#[derive(Debug, Clone)]
pub struct JsRuntime {}

impl JsRuntime {
    /// 空のランタイムを作成します。
    pub fn new() -> Self {
        Self {}
    }

    /// 単一ノードを評価し、`RuntimeValue` を返す（無ければ `None`）。
    /// - `ExpressionStatement(expr)` → 中の式をそのまま評価
    /// - `AdditiveExpression` → 左右を評価し、`+` / `-` を適用
    /// - `NumericLiteral(n)` → `Number(n)` に変換
    /// - `Assignment/Member` → まだ未実装のため `None`
    fn eval(&mut self, node: &Option<Rc<Node>>) -> Option<RuntimeValue> {
        let node = match node {
            Some(n) => n,
            None => return None,
        };

        // `Rc<Node>` を借用して `&Node` にし、分岐します（clone を避けて効率的）。
        match node.borrow() {
            Node::ExpressionStatement(expr) => return self.eval(&expr),
            Node::AdditiveExpression {
                operator,
                left,
                right,
            } => {
                // 左右の部分式を再帰的に評価。
                let left_value = match self.eval(&left) {
                    Some(value) => value,
                    None => return None,
                };
                let right_value = match self.eval(&right) {
                    Some(value) => value,
                    None => return None,
                };

                // `+` / `-` に応じて `Add` / `Sub` 実装を利用。
                if operator == &'+' {
                    Some(left_value + right_value)
                } else if operator == &'-' {
                    Some(left_value - right_value)
                } else {
                    None
                }
            }
            Node::AssignmentExpression {
                operator: _,
                left: _,
                right: _,
            } => {
                // 後ほど実装
                None
            }
            Node::MemberExpression {
                object: _,
                property: _,
            } => {
                // 後ほど実装
                None
            }
            Node::NumericLiteral(value) => Some(RuntimeValue::Number(*value)),
        }
    }

    /// `Program`（複数の文）を順に評価します。戻り値は捨てています。
    /// REPL のように「最後の値を返す」仕様にはしていません。
    pub fn execute(&mut self, program: &Program) {
        for node in program.body() {
            self.eval(&Some(node.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::js::ast::JsParser;
    use crate::renderer::js::token::JsLexer;
    use alloc::string::ToString; // `"...".to_string()` を使うためのトレイト（no_std では自動導入されない）

    #[test]
    fn test_num() {
        // 単一の数値リテラルを評価 → Number(42)
        let input = "42".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [Some(RuntimeValue::Number(42))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()));
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_add_nums() {
        // 1 + 2 → 3
        let input = "1 + 2".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [Some(RuntimeValue::Number(3))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()));
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_sub_nums() {
        // 2 - 1 → 1
        let input = "2 - 1".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [Some(RuntimeValue::Number(1))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()));
            assert_eq!(expected[i], result);
            i += 1;
        }
    }
}
