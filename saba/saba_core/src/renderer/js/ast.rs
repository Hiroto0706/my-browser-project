//! saba_core::renderer::js::ast — 最小構成の JavaScript 風 AST とパーサ
//!
//! 目的
//! - JavaScript 風の基本的な構文を抽象構文木（AST）へ変換します。
//! - 文の並び（`Program`）と、式・宣言（`Node`）をシンプルな形で表現します。
//!
//! 文法スケッチ（学習向けに読みやすく）
//! - `Program        → Statement*`
//! - `Statement      → VariableDeclaration ';'? | ReturnStatement ';'? | ExpressionStatement ';'?`
//! - `Expression     → AssignmentExpression`
//! - `Assignment     → LeftHandSide '=' AssignmentExpression | AdditiveExpression`
//! - `Additive       → LeftHandSide (('+'|'-') AssignmentExpression)?`
//! - `LeftHandSide   → MemberExpression`
//! - `Member         → Primary`
//! - `Primary        → Identifier | StringLiteral | NumericLiteral`
//!
//! 型ブリッジ（TS / Python の感覚）
//! - `Node::VariableDeclaration/VariableDeclarator/Identifier/StringLiteral/NumericLiteral` などは
//!   ESTree の用語と概念的に対応します。
//! - `Program { body: Vec<Rc<Node>> }` は TS の `Program { body: Node[] }`／Python の `ast.Module(body)` に相当。
//! - 共有には `Rc<Node>` を使います（所有権の移動なしで参照共有）。
//!
//! 環境メモ
//! - `no_std` 前提のため、`alloc` クレート（`Rc`, `Vec`, `String` 等）を利用します。

use crate::renderer::js::token::JsLexer;
use crate::renderer::js::token::Token;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::iter::Peekable;

#[derive(Debug, Clone, PartialEq, Eq)]
/// ESTree 風のノード種類（学習用に大幅簡略化）
///
/// ざっくり対応関係（TS/Python イメージ）
/// - `ExpressionStatement(expr)`  … `type: 'ExpressionStatement'` / Python の `ast.Expr`
/// - `AdditiveExpression{..}`    … `BinaryExpression`(operator: '+'|'-') / `ast.BinOp`
/// - `AssignmentExpression{..}`  … `AssignmentExpression` / `ast.Assign`
/// - `MemberExpression{..}`      … `MemberExpression` / `ast.Attribute`
/// - `NumericLiteral(n)`         … `Literal<number>` / `ast.Constant`
/// - `VariableDeclaration{..}`   … `VariableDeclaration(kind: 'var')`
/// - `VariableDeclarator{..}`    … `VariableDeclarator(id, init)`
/// - `Identifier(name)`          … `Identifier`
/// - `StringLiteral(value)`      … `Literal<string>`
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

    /// 代入式。
    AssignmentExpression {
        operator: char,
        left: Option<Rc<Node>>,
        right: Option<Rc<Node>>,
    },

    /// メンバアクセス（`obj.prop` 等）。
    MemberExpression {
        object: Option<Rc<Node>>,
        property: Option<Rc<Node>>,
    },

    /// 数値リテラル。`u64` を保持します。
    NumericLiteral(u64),

    /// 変数宣言。学習用に `var` のみ想定し、`declarations` の列を持ちます。
    /// - 例: `var a = 1, b;` → `VariableDeclaration { declarations: [VarDecl(a=1), VarDecl(b=None)] }`
    /// - 厳密には `kind: 'var'|'let'|'const'` などを持ちますが簡略化しています。
    VariableDeclaration { declarations: Vec<Option<Rc<Node>>> },

    /// 単一の宣言子。名前（`id`）と初期化子（`init`）のペア。
    /// - 例: `var a = 1;` → `VariableDeclarator { id: Identifier("a"), init: NumericLiteral(1) }`
    VariableDeclarator {
        id: Option<Rc<Node>>,
        init: Option<Rc<Node>>,
    },

    /// 識別子。変数名や関数名などのシンボル名を表します。例: `Identifier("foo")`
    Identifier(String),

    /// 文字列リテラル。二重引用符で囲まれた文字列の内容。例: `StringLiteral("bar")`
    StringLiteral(String),
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

    /// `AssignmentExpression` を作成します。
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

    /// `MemberExpression` を作成します。
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

    /// 単一の宣言子（`VariableDeclarator`）を作ります。
    ///
    /// 例
    /// - `var a = 1;` → `new_variable_declarator(Identifier("a"), NumericLiteral(1))`
    /// - `var b;`     → `new_variable_declarator(Identifier("b"), None)`（初期化子なし）
    ///
    /// 補足（TS/Python）
    /// - TS: `{ id: Identifier, init?: Expression }`
    /// - Python: `ast.Assign(targets=[Name("a")], value=Constant(1))` に相当（かなり簡略化）
    pub fn new_variable_declarator(
        id: Option<Rc<Self>>,
        init: Option<Rc<Self>>,
    ) -> Option<Rc<Self>> {
        Some(Rc::new(Node::VariableDeclarator { id, init }))
    }

    /// 複数の宣言子から `VariableDeclaration` を作ります。
    ///
    /// 例
    /// - `var a = 1, b;` → `declarations = [VarDecl(a=1), VarDecl(b=None)]`
    ///
    /// メモ
    /// - 本来は `kind: 'var'|'let'|'const'` を持ちますが学習用のため省略しています。
    pub fn new_variable_declaration(declarations: Vec<Option<Rc<Self>>>) -> Option<Rc<Self>> {
        Some(Rc::new(Node::VariableDeclaration { declarations }))
    }

    /// `Identifier(name)` を作ります。変数名・関数名などの“名前”用ノード。
    /// - 例: `new_identifier("result".into())`
    pub fn new_identifier(name: String) -> Option<Rc<Self>> {
        Some(Rc::new(Node::Identifier(name)))
    }

    /// `StringLiteral(value)` を作ります。二重引用符の中身だけを保持します。
    /// - 例: 入力 `"bar"` → `new_string_literal("bar".into())`
    pub fn new_string_literal(value: String) -> Option<Rc<Self>> {
        Some(Rc::new(Node::StringLiteral(value)))
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

    /// 本文（文の並び）をまとめてセットします。
    /// - 既存の `body` は置き換えます。追記したい場合は呼び出し側で `push` してください。
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

    /// Primary（最小単位の式）を読みます。対応: 識別子 / 文字列 / 数値。
    ///
    /// 例
    /// - 数値: `42` → `NumericLiteral(42)`
    /// - 文字列: `"hi"` → `StringLiteral("hi")`
    /// - 識別子: `foo` → `Identifier("foo")`
    ///
    /// 実装メモ
    /// - `next()` で 1 トークン読み取り、種別に応じてノードを生成します。
    fn primary_expression(&mut self) -> Option<Rc<Node>> {
        let t = match self.t.next() {
            Some(token) => token,
            None => return None,
        };

        match t {
            Token::Identifier(value) => Node::new_identifier(value),
            Token::StringLiteral(value) => Node::new_string_literal(value),
            Token::Number(value) => Node::new_numeric_literal(value),
            _ => None,
        }
    }

    /// メンバ式。
    fn member_expression(&mut self) -> Option<Rc<Node>> {
        self.primary_expression()
    }

    /// 左辺値式。
    fn left_hand_side_expression(&mut self) -> Option<Rc<Node>> {
        self.member_expression()
    }

    /// `+` / `-` を扱う加算式。
    /// - 例: `1 + 2` → `AdditiveExpression('+', 1, 2)`
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

    /// 代入式（`=`）を読みます。
    ///
    /// 仕様と実装ポイント
    /// - まず加算式（`additive_expression`）を読み、これを左辺候補 `expr` とします。
    /// - 次トークンを `peek()` し、`'='` なら 1 つ消費して右辺を“再帰的に” `assignment_expression()` で読みます。
    ///   これにより `a = b = 1` のような式は右結合（`a = (b = 1)`）として扱われます。
    /// - `=` でなければ、単なる加算式として `expr` を返します（代入ではない）。
    fn assignment_expression(&mut self) -> Option<Rc<Node>> {
        let expr = self.additive_expression();

        let t = match self.t.peek() {
            Some(token) => token,
            None => return expr,
        };

        match t {
            Token::Punctuator('=') => {
                // '='を消費する
                assert!(self.t.next().is_some());
                Node::new_assignment_expression('=', expr, self.assignment_expression())
            }
            _ => expr,
        }
    }

    /// 変数宣言子の“初期化子”部分（`= 式`）を読み取ります（任意）。
    ///
    /// 仕様（この簡易実装）
    /// - 次の 1 トークンを読み、`'='` なら後続を `assignment_expression()` に委譲して返します。
    /// - `'='` 以外なら初期化子なしとして `None` を返します。
    ///
    /// TS/Python イメージ
    /// - TS: `init?: Expression`（`=` があれば存在、無ければ `undefined`）
    /// - Python: `ast.Assign(..., value?)`（ここでは `value` の有無に相当）
    fn initialiser(&mut self) -> Option<Rc<Node>> {
        // 次のトークンを 1 つ取り出して、`=` かどうかを判定
        let t = match self.t.next() {
            Some(token) => token,
            None => return None,
        };

        match t {
            Token::Punctuator(c) => match c {
                '=' => self.assignment_expression(), // `=` の後ろは代入式として読む
                _ => None,
            },
            _ => None,
        }
    }

    /// 次のトークンが識別子（Identifier）ならそれを `Node::Identifier` にして返します。
    /// - 想定位置: 直前で `var` を読み終えた直後など。
    /// - それ以外（数値や記号など）のトークンなら `None` を返します（学習用の簡易実装）。
    fn identifier(&mut self) -> Option<Rc<Node>> {
        let t = match self.t.next() {
            Some(token) => token,
            None => return None,
        };

        match t {
            Token::Identifier(name) => Node::new_identifier(name),
            _ => None,
        }
    }

    /// 変数宣言（`var` の後ろ）を 1 件だけ読み取り、`VariableDeclaration` を作ります。
    ///
    /// 振る舞い
    /// - まず識別子名を `identifier()` で読む（例: `var foo ...` の `foo`）。
    /// - 続けて初期化子を `initialiser()` で読む想定（例: `= 42`）。なければ `None`。
    /// - 1 つの `VariableDeclarator` を `declarations` ベクタに入れて `VariableDeclaration` を返す。
    ///
    /// 補足
    /// - 文末の `;` は呼び出し側（`statement()`）で処理します。
    ///
    /// TS/Python のイメージ
    /// - TS: `VariableDeclaration { declarations: [ { id, init? } ] }`
    /// - Python: `ast.Assign(targets=[Name(id)], value?)` に相当（かなり簡略化）
    fn variable_declaration(&mut self) -> Option<Rc<Node>> {
        let ident = self.identifier();

        let declarator = Node::new_variable_declarator(ident, self.initialiser());

        let mut declarations = Vec::new();
        declarations.push(declarator);

        Node::new_variable_declaration(declarations)
    }

    /// 文（Statement）を 1 つ読み取って AST ノードにします。
    ///
    /// 対応（学習用の簡易版）
    /// - 変数宣言: 先頭が `var` → `variable_declaration()` を呼んで `VariableDeclaration` を作る。
    /// - 上記以外: それ以外は式文として `ExpressionStatement(assignment_expression)` にする。
    ///
    /// セミコロン
    /// - 末尾に `;` があれば 1 つだけ消費（任意セミコロン）。
    fn statement(&mut self) -> Option<Rc<Node>> {
        let t = match self.t.peek() {
            Some(t) => t,
            None => return None,
        };

        // 先頭トークンの種類で文の種別を判定
        let node = match t {
            Token::Keyword(keyword) => {
                if keyword == "var" {
                    // "var" を 1 つ消費してから、宣言の本体を読む
                    assert!(self.t.next().is_some());

                    self.variable_declaration()
                } else {
                    None
                }
            }
            _ => Node::new_expression_statement(self.assignment_expression()),
        };

        if let Some(Token::Punctuator(c)) = self.t.peek() {
            // 文末の `;` を 1 つだけ消費（任意）
            if c == &';' {
                assert!(self.t.next().is_some());
            }
        }

        node
    }

    /// トップレベルの“要素”を読みます。トークンが無ければ `None` を返します。
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
    // このモジュールでは AST パーサ `JsParser::parse_ast()` の出力形を確認します。
    // - `Program.body` に文が順に入ることを前提に、各ケースのノード形状を比較します。
    // - 検証対象: NumericLiteral / AdditiveExpression / VariableDeclaration / VariableDeclarator /
    //             Identifier / StringLiteral など。
    // - TS/Python イメージ: `Program { body: Node[] }` / `ast.Module(body=[...])`。

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

    #[test]
    fn test_assign_variable() {
        // 変数宣言（文字列初期化）: var foo = "bar";
        // - VariableDeclaration の中に 1 件の VariableDeclarator
        // - id: Identifier("foo") / init: StringLiteral("bar")
        let input = "var foo=\"bar\";".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let mut expected = Program::new();
        let mut body = Vec::new();
        body.push(Rc::new(Node::VariableDeclaration {
            declarations: [Some(Rc::new(Node::VariableDeclarator {
                id: Some(Rc::new(Node::Identifier("foo".to_string()))),
                init: Some(Rc::new(Node::StringLiteral("bar".to_string()))),
            }))]
            .to_vec(),
        }));
        expected.set_body(body);
        assert_eq!(expected, parser.parse_ast());
    }

    #[test]
    fn test_add_variable_and_num() {
        // 2 文の連続: `var foo=42;` と `var result=foo+1;`
        // - 1 文目: id=Identifier("foo"), init=NumericLiteral(42)
        // - 2 文目: id=Identifier("result"), init=AdditiveExpression('+', Identifier("foo"), NumericLiteral(1))
        let input = "var foo=42; var result=foo+1;".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let mut expected = Program::new();
        let mut body = Vec::new();
        body.push(Rc::new(Node::VariableDeclaration {
            declarations: [Some(Rc::new(Node::VariableDeclarator {
                id: Some(Rc::new(Node::Identifier("foo".to_string()))),
                init: Some(Rc::new(Node::NumericLiteral(42))),
            }))]
            .to_vec(),
        }));
        body.push(Rc::new(Node::VariableDeclaration {
            declarations: [Some(Rc::new(Node::VariableDeclarator {
                id: Some(Rc::new(Node::Identifier("result".to_string()))),
                init: Some(Rc::new(Node::AdditiveExpression {
                    operator: '+',
                    left: Some(Rc::new(Node::Identifier("foo".to_string()))),
                    right: Some(Rc::new(Node::NumericLiteral(1))),
                })),
            }))]
            .to_vec(),
        }));
        expected.set_body(body);
        assert_eq!(expected, parser.parse_ast());
    }
}
