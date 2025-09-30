//! saba_core::renderer::js::ast — 超最小 AST（抽象構文木）とパーサ（初心者向け）
//!
//! 目的
//! - JavaScript 風の“ごく一部”の構文を AST に変換します。
//! - サポートするのは数値リテラル、`+`/`-` の加算式、式文（末尾の `;` は任意）。
//! - 仕様準拠ではなく学習用に大幅簡略化しています。
//!
//! 簡易文法（イメージ）
//! - `Program      → Statement*`
//! - `Statement    → Expression ';'?`  （`;` はあってもなくても可）
//! - `Expression   → AdditiveExpression`
//! - `Additive     → Primary (('+'|'-') Additive)?` 右再帰（学習用の簡易実装）
//! - `Primary      → Number`
//!
//! 型と用語の橋渡し（TS / Python）
//! - Rust の `trait` ≈ TS の interface / Python の protocol。
//! - `Result<T, E>` ≈ `try/except` の結果。`?` は `await`/`raise` 風の伝播。
//! - 所有権/借用: `Rc<Node>` は“参照カウント付き共有ポインタ”。TS/Python の参照渡しに近い。
//! - AST 形状は ESTree の用語をゆるく取り入れています（`ExpressionStatement` など）。
//!
//! 使い方（イメージ）
//! ```rust,ignore
//! use saba_core::renderer::js::token::JsLexer;
//! use saba_core::renderer::js::ast::JsParser;
//! let input = "1 + 2;".to_string();
//! let lexer = JsLexer::new(input);
//! let mut parser = JsParser::new(lexer);
//! let program = parser.parse_ast(); // Program { body: [ExpressionStatement(AdditiveExpression(...))] }
//! ```
//!
//! 注意
//! - エラーハンドリングは最小で、未対応のトークンに遭遇すると途中までの結果を返すことがあります。
//! - 実運用のパーサでは `Result<..>` とエラー情報（位置など）を返す設計が一般的です。
//! - `no_std` 環境のため、動的確保は `alloc` クレート（`Rc`, `Vec`, `String` 等）に依存します。

use crate::renderer::js::token::JsLexer;
use crate::renderer::js::token::Token;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::iter::Peekable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// `expr;` の形の文。ここでは式しか扱わないため、単純に式を包むだけです。
    /// - TS の `ExpressionStatement { expression: Node }` に相当。
    /// - 末尾のセミコロンは `statement()` 側で任意処理します。
    ExpressionStatement(Option<Rc<Node>>),

    /// 加算/減算の二項演算子（学習用の簡易版）
    /// - `operator`: `'+'` または `'-'`
    /// - `left`/`right`: それぞれのオペランド。
    /// - 右再帰のため結合規則は実装依存（現状は左結合を十分に扱えていません）。
    AdditiveExpression {
        operator: char,
        left: Option<Rc<Node>>,
        right: Option<Rc<Node>>,
    },

    /// 代入式（将来拡張のプレースホルダ）。現時点では実装未満で、
    /// `assignment_expression()` は単に `additive_expression()` を呼び出します。
    AssignmentExpression {
        operator: char,
        left: Option<Rc<Node>>,
        right: Option<Rc<Node>>,
    },

    /// メンバアクセス（`obj.prop` 等）のプレースホルダ。今は Primary をそのまま返します。
    MemberExpression {
        object: Option<Rc<Node>>,
        property: Option<Rc<Node>>,
    },

    /// 数値リテラル。`u64` の範囲で扱います（負数や浮動小数は未対応）。
    NumericLiteral(u64),
}

impl Node {
    /// `ExpressionStatement` を作成します。Option なのは“ない”ケースを簡単に表現するため。
    pub fn new_expression_statement(expression: Option<Rc<Self>>) -> Option<Rc<Self>> {
        Some(Rc::new(Node::ExpressionStatement(expression)))
    }

    /// `AdditiveExpression` を作成します。
    pub fn new_additive_expression(
        operator: char,
        left: Option<Rc<Node>>,
        right: Option<Rc<Node>>,
    ) -> Option<Rc<Self>> {
        Some(Rc::new(Node::AdditiveExpression {
            operator,
            left,
            right,
        }))
    }

    /// `AssignmentExpression` を作成します（現状は未使用の将来拡張）。
    pub fn new_assignment_expression(
        operator: char,
        left: Option<Rc<Node>>,
        right: Option<Rc<Node>>,
    ) -> Option<Rc<Self>> {
        Some(Rc::new(Node::AssignmentExpression {
            operator,
            left,
            right,
        }))
    }

    /// `MemberExpression` を作成します（現状は未使用の将来拡張）。
    pub fn new_member_expression(
        object: Option<Rc<Self>>,
        property: Option<Rc<Self>>,
    ) -> Option<Rc<Self>> {
        Some(Rc::new(Node::MemberExpression { object, property }))
    }

    /// 数値リテラルを作成します。
    pub fn new_numeric_literal(value: u64) -> Option<Rc<Self>> {
        Some(Rc::new(Node::NumericLiteral(value)))
    }
}

/// JavaScript の最上位ノード（ESTree の `Program` 相当、初心者向け）
///
/// 役割
/// - ソース全体を表す AST ルート。中に文（Statement）や宣言が順に入ります。
/// - シンプル化のため、ここでは `body: Vec<Rc<Node>>` に各ノードを平坦に格納します。
///
/// TS/Python にたとえると
/// - `interface Program { body: Node[] }` のイメージ。Python ならモジュール直下の文リスト。
///
/// 設計メモ
/// - `Rc<Node>` を使うのは、解析/最適化など後工程でノード共有が発生しても所有権の制約を緩めるため。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    body: Vec<Rc<Node>>,
}

impl Program {
    /// 空の `Program` を作る（まだ文は持たない）
    pub fn new() -> Self {
        Self { body: Vec::new() }
    }

    /// 本文（文の並び）をまとめてセットする
    ///
    /// 注意
    /// - 既存の `body` は置き換えられます。追記したい場合は呼び出し側で `push` してください。
    pub fn set_body(&mut self, body: Vec<Rc<Node>>) {
        self.body = body;
    }

    /// 本文（文の並び）への参照を返す
    pub fn body(&self) -> &Vec<Rc<Node>> {
        &self.body
    }
}

pub struct JsParser {
    t: Peekable<JsLexer>,
}

impl JsParser {
    /// 文字列 → 字句解析器 `JsLexer` → `Peekable` に包んでパーサを用意。
    /// - `Peekable` は“次のトークンを消費せずに覗く”ために使います（構文判断に便利）。
    pub fn new(t: JsLexer) -> Self {
        Self { t: t.peekable() }
    }

    /// Primary（最小単位の式）を読む。いまは数値のみ対応。
    /// - `42` → `NumericLiteral(42)`
    /// - それ以外は未対応のため `None`。
    fn primary_expression(&mut self) -> Option<Rc<Node>> {
        let t = match self.t.next() {
            Some(token) => token,
            None => return None,
        };

        match t {
            Token::Number(value) => Node::new_numeric_literal(value),
            _ => None,
        }
    }

    /// メンバ式。現状は Primary をそのまま返す（将来 `obj.prop` 等をここで扱う想定）。
    fn member_expression(&mut self) -> Option<Rc<Node>> {
        self.primary_expression()
    }

    /// 左辺値式。現状はメンバ式＝Primary と同義（将来 `a.b = 1` などで利用）。
    fn left_hand_side_expression(&mut self) -> Option<Rc<Node>> {
        self.member_expression()
    }

    /// `+` / `-` を 1 回だけ見る簡易版の加算式。
    /// - 入力: `1 + 2` → `AdditiveExpression('+', 1, 2)`
    /// - 未対応: 連鎖（`1 + 2 + 3`）や優先順位、左結合の厳密性など。
    fn additive_expression(&mut self) -> Option<Rc<Node>> {
        // 足し算または引き算の左辺を作る
        let left = self.left_hand_side_expression();

        let t = match self.t.peek() {
            Some(token) => token.clone(),
            None => return left, // トークンが無ければそのまま返す
        };

        match t {
            Token::Punctuator(c) => match c {
                // 次のトークンが `+` または `-` なら、それを消費して右辺を読む
                '+' | '-' => {
                    // 記号を 1 つ消費
                    assert!(self.t.next().is_some());
                    Node::new_additive_expression(c, left, self.assignment_expression())
                }
                _ => left,
            },
            _ => left,
        }
    }

    /// 代入式の読み取り。現状は加算式にフォールバック（未実装の雛形）。
    fn assignment_expression(&mut self) -> Option<Rc<Node>> {
        self.additive_expression()
    }

    /// 文（Statement）を 1 つ読む。式文のみ対応。
    /// - 末尾に `;` があれば消費（任意）。
    fn statement(&mut self) -> Option<Rc<Node>> {
        let node = Node::new_expression_statement(self.assignment_expression());

        if let Some(Token::Punctuator(c)) = self.t.peek() {
            // `;` を消費（あれば）
            if c == &';' {
                assert!(self.t.next().is_some());
            }
        }

        node
    }

    /// トップレベルの“要素”を読む。未対応の構文なら `None` を返す想定。
    fn source_element(&mut self) -> Option<Rc<Node>> {
        match self.t.peek() {
            Some(t) => t,
            None => return None,
        };

        self.statement()
    }

    /// 入力全体を走査して `Program` を作るメイン関数。
    /// - 反復的に `source_element()` を呼び、`None` になったら終了。
    /// - できたノード列を `Program.body` に設定して返します。
    pub fn parse_ast(&mut self) -> Program {
        let mut program = Program::new();

        let mut body = Vec::new();

        loop {
            // ノードが作成できなくなるまで生成を繰り返す
            let node = self.source_element();

            match node {
                Some(n) => body.push(n),
                None => {
                    // 生成済みノードを body にセットして AST を返す
                    program.set_body(body);
                    return program;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    // このモジュールでは JS の“超最小”AST パーサの挙動を確認します。
    // - 入口は `JsParser::parse_ast()`。戻り値は最上位ノード `Program`。
    // - `Program.body` には文（Statement）が並びます。ここでは主に
    //   `ExpressionStatement` とその中の式（数値リテラル / 加算式）を検証します。
    // - TS/Python イメージ: `Program { body: Node[] }`、`ExpressionStatement { expression: Node }`。

    #[test]
    fn test_empty() {
        // 空入力 → `Program.body` が空の AST を返す
        // 手順
        // 1) 入力を字句解析器 `JsLexer` に渡す
        // 2) 構文解析器 `JsParser` で AST（Program）を構築
        // 3) 何も無いので `Program::new()` と等しいはず
        let input = "".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let expected = Program::new();
        assert_eq!(expected, parser.parse_ast());
    }

    #[test]
    fn test_num() {
        // 単一の数値 "42" → `ExpressionStatement(NumericLiteral(42))` が 1 つ入る
        // - ESTree 風に、式は文として `ExpressionStatement` でラップされます
        // - TS 表現のイメージ: Program.body = [ { type: 'ExpressionStatement', expression: { type: 'NumericLiteral', value: 42 } } ]
        let input = "42".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let mut expected = Program::new();
        let mut body = Vec::new();
        body.push(Rc::new(Node::ExpressionStatement(Some(Rc::new(
            Node::NumericLiteral(42),
        )))));
        expected.set_body(body);
        assert_eq!(expected, parser.parse_ast());
    }

    #[test]
    fn test_add_nums() {
        // 二項加算 "1 + 2" → `AdditiveExpression` ノードを生成
        // - `operator: '+'`
        // - `left`: NumericLiteral(1)
        // - `right`: NumericLiteral(2)
        // - 式全体は文として `ExpressionStatement` で包まれ、`Program.body` に 1 要素として入る
        // Python 風に言うと: ちょうど `ast.BinOp(Num(1), Add(), Num(2))` を `ast.Expr(...)` で包んだイメージ
        let input = "1 + 2".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let mut expected = Program::new();
        let mut body = Vec::new();
        body.push(Rc::new(Node::ExpressionStatement(Some(Rc::new(
            Node::AdditiveExpression {
                operator: '+',
                left: Some(Rc::new(Node::NumericLiteral(1))),
                right: Some(Rc::new(Node::NumericLiteral(2))),
            },
        )))));
        expected.set_body(body);
        assert_eq!(expected, parser.parse_ast());
    }
}
