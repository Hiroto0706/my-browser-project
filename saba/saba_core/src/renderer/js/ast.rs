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

    /// ブロック文。波括弧 `{ ... }` 内の文の並びをまとめたコンテナ。
    /// - 例: `{ var a = 1; a; }` → `BlockStatement { body: [VariableDeclaration(..), ExpressionStatement(..)] }`
    /// - TS/Python: `BlockStatement` / 複数文の並び（関数本体など）。
    BlockStatement { body: Vec<Option<Rc<Node>>> },

    /// return 文。`return` に続く式（省略可）を `argument` に保持します。
    /// - 例: `return 1;` → `ReturnStatement { argument: NumericLiteral(1) }`
    /// - 例: `return;`   → `ReturnStatement { argument: None }`
    ReturnStatement { argument: Option<Rc<Node>> },

    /// 関数宣言。
    /// - `id`: 関数名（Identifier）。
    /// - `params`: 仮引数の並び（Identifier の列）。
    /// - `body`: 本体ブロック（`BlockStatement`）。
    /// 例: `function add(a, b) { return a + b; }`
    FunctionDeclaration {
        id: Option<Rc<Node>>,
        params: Vec<Option<Rc<Node>>>,
        body: Option<Rc<Node>>,
    },

    /// 関数呼び出し式。
    /// - `callee`: 呼び出し先（例: `Identifier("foo")`）
    /// - `arguments`: 実引数の並び（式の列）
    /// 例: `foo(1, 2)` → `CallExpression { callee: Identifier("foo"), arguments: [1, 2] }`
    CallExpression {
        callee: Option<Rc<Node>>,
        arguments: Vec<Option<Rc<Node>>>,
    },
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

    /// `{ ... }` のブロック文を作ります。
    /// - 例: `{ var a = 1; a; }` → `new_block_statement(vec![VarDecl(..), Expr(..)])`
    /// - TS/Python: `BlockStatement` / 複数文の並び
    pub fn new_block_statement(body: Vec<Option<Rc<Self>>>) -> Option<Rc<Self>> {
        Some(Rc::new(Node::BlockStatement { body }))
    }

    /// `return` 文を作ります。`argument` は戻り値の式（省略可）。
    /// - 例: `return 1;` → `new_return_statement(NumericLiteral(1))`
    /// - 例: `return;`   → `new_return_statement(None)`
    pub fn new_return_statement(argument: Option<Rc<Self>>) -> Option<Rc<Self>> {
        Some(Rc::new(Node::ReturnStatement { argument }))
    }

    /// 関数宣言を作ります。
    /// - `id`: 関数名（Identifier）
    /// - `params`: 仮引数の並び（Identifier の列）
    /// - `body`: 本体ブロック（BlockStatement）
    /// 例: `function add(a, b) { return a + b; }`
    pub fn new_function_declaration(
        id: Option<Rc<Self>>,
        params: Vec<Option<Rc<Self>>>,
        body: Option<Rc<Self>>,
    ) -> Option<Rc<Self>> {
        Some(Rc::new(Node::FunctionDeclaration { id, params, body }))
    }

    /// 関数呼び出し式を作ります。
    /// - `callee`: 呼び出し先（例: Identifier("foo"))
    /// - `arguments`: 実引数の並び
    /// 例: `foo(1, 2)` → `new_call_expression(Identifier("foo"), vec![1, 2])`
    pub fn new_call_expression(
        callee: Option<Rc<Self>>,
        arguments: Vec<Option<Rc<Self>>>,
    ) -> Option<Rc<Self>> {
        Some(Rc::new(Node::CallExpression { callee, arguments }))
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

    /// メンバ式（プロパティアクセス）を読み取ります。
    ///
    /// 役割
    /// - まず基底となる式（`Primary`）を読み、直後が `.` のときに `Identifier` をプロパティ名として
    ///   取り出し、`MemberExpression { object, property }` を構築します。
    ///
    /// 例
    /// - `foo.bar` → `MemberExpression { object: Identifier("foo"), property: Identifier("bar") }`
    fn member_expression(&mut self) -> Option<Rc<Node>> {
        // オブジェクト側（左側）の式を読む
        let expr = self.primary_expression();

        let t = match self.t.peek() {
            Some(token) => token,
            None => return expr,
        };

        match t {
            Token::Punctuator(c) => {
                if c == &'.' {
                    // '.'を消費する
                    assert!(self.t.next().is_some());
                    // プロパティ名（識別子）を読み、MemberExpression を作る
                    return Node::new_member_expression(expr, self.identifier());
                }

                expr
            }
            _ => expr,
        }
    }

    /// 呼び出しの実引数を読み取り、式ノードの配列として返します。
    ///
    /// 形式
    /// - `)` までを対象に、`,` 区切りで `assignment_expression()` を順に収集します。
    /// 例: `(1, x+2)` → `[NumericLiteral(1), AdditiveExpression('+', Identifier("x"), NumericLiteral(2))]`
    fn arguments(&mut self) -> Vec<Option<Rc<Node>>> {
        let mut arguments = Vec::new();

        loop {
            // ')'に到達するまで、解釈した値を`arguments`ベクタに追加する
            match self.t.peek() {
                Some(t) => match t {
                    Token::Punctuator(c) => {
                        if c == &')' {
                            // ')'を消費する
                            assert!(self.t.next().is_some());
                            return arguments;
                        }
                        if c == &',' {
                            // ','を消費する
                            assert!(self.t.next().is_some());
                        }
                    }
                    _ => arguments.push(self.assignment_expression()),
                },
                None => return arguments,
            }
        }
    }

    /// 左辺値式。
    ///
    /// 役割
    /// - `MemberExpression` を基に、直後に `(` が続く場合は関数呼び出しとして `CallExpression`
    ///   を構築します。
    /// - つまり簡略化すると: `LeftHandSide → Member | Call(Member, Arguments)`。
    ///
    /// 例
    /// - `foo.bar(1, 2)` → `CallExpression { callee: MemberExpression(foo, bar), arguments: [1, 2] }`
    fn left_hand_side_expression(&mut self) -> Option<Rc<Node>> {
        // まずメンバ式を読み、呼び出し対象（callee）の候補とする
        let expr = self.member_expression();

        let t = match self.t.peek() {
            Some(token) => token,
            None => return expr,
        };

        match t {
            Token::Punctuator(c) => {
                if c == &'(' {
                    // '('を消費する
                    assert!(self.t.next().is_some());
                    // 関数呼び出しのため、CallExpressionノードを返す
                    return Node::new_call_expression(expr, self.arguments());
                }

                // それ以外の記号なら、単なる MemberExpression として返す
                expr
            }
            // 先頭が記号以外（識別子・数値など）の場合も、そのまま返す
            _ => expr,
        }
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

    /// 文（Statement）を 1 つ読み取り、対応する AST ノードを返します。
    ///
    /// 対応（簡易版）
    /// - 変数宣言: 先頭が `var` → `variable_declaration()` を呼ぶ。
    /// - return 文: 先頭が `return` → 後続の式（任意）を読み、`ReturnStatement` に包む。
    /// - 式文: 上記以外は `ExpressionStatement(assignment_expression)` にする。
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
                } else if keyword == "return" {
                    // "return"の予約語を消費する
                    assert!(self.t.next().is_some());

                    Node::new_return_statement(self.assignment_expression())
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

    /// 関数本体 `{ ... }` を読み取り、`BlockStatement` を返します。
    ///
    /// 手順
    /// - 開き波括弧 `{` を 1 つ消費。
    /// - `}` に達するまで、`source_element()` を使って文/宣言を順に読み、`body` に蓄える。
    /// - 閉じ波括弧 `}` を消費して、`new_block_statement(body)` を返す。
    fn function_body(&mut self) -> Option<Rc<Node>> {
        // '{'を消費する
        match self.t.next() {
            Some(t) => match t {
                Token::Punctuator(c) => assert!(c == '{'),
                _ => unimplemented!("function should have open curly blacket but got {:?}", t),
            },
            None => unimplemented!("function should have open curly blacket but got None"),
        }

        let mut body = Vec::new(); // 本体の文をここに集める
        loop {
            // '}'に到達するまで、関数内のコードとして解釈する
            match self.t.peek() {
                Some(t) => match t {
                    Token::Punctuator(c) => {
                        if c == &'}' {
                            // '}'を消費し、BlockStatementノードを返す
                            assert!(self.t.next().is_some());
                            return Node::new_block_statement(body);
                        }
                    }
                    _ => {}
                },
                None => {}
            }

            // 関数内の文・宣言を 1 つ読み取り、`body` に追加
            body.push(self.source_element());
        }
    }

    /// 仮引数リストを読み取り、識別子ノードの配列として返します。
    ///
    /// 形式
    /// - `(` identifier (`,` identifier)* `)`
    /// 例: `(a, b, c)` → `[Identifier("a"), Identifier("b"), Identifier("c")]`
    fn parameter_list(&mut self) -> Vec<Option<Rc<Node>>> {
        let mut params = Vec::new();

        // 1) 開き括弧 `(` を 1 つ消費
        match self.t.next() {
            Some(t) => match t {
                Token::Punctuator(c) => assert!(c == '('),
                _ => unimplemented!("function should have `(` but got {:?}", t),
            },
            None => unimplemented!("function should have `(` but got None"),
        }

        // 2) `)` に出会うまで、`,` 区切りで識別子を読み続ける
        loop {
            match self.t.peek() {
                Some(t) => match t {
                    Token::Punctuator(c) => {
                        if c == &')' {
                            // 閉じ括弧 `)` を消費して終了
                            assert!(self.t.next().is_some());
                            return params;
                        }
                        if c == &',' {
                            // 区切りカンマを 1 つ消費して次の識別子へ
                            assert!(self.t.next().is_some());
                        }
                    }
                    _ => {
                        // 識別子を 1 つ読み、パラメータ配列に追加
                        params.push(self.identifier());
                    }
                },
                None => return params,
            }
        }
    }

    /// 関数宣言を読み取り、`FunctionDeclaration` ノードを作ります。
    ///
    /// 前提
    /// - 直前で `function` キーワードは消費済み（`source_element` 側で処理）。
    ///
    /// 手順
    /// 1) 関数名を `identifier()` で読む（例: `function foo(...)` の `foo`）。
    /// 2) 仮引数リストを `parameter_list()` で読む（丸括弧内の識別子の並び）。
    /// 3) 本体ブロックを `function_body()` で読む（`{ ... }`）。
    /// 4) 以上を `new_function_declaration(..)` で 1 つのノードにまとめて返す。
    fn function_declaration(&mut self) -> Option<Rc<Node>> {
        // 1) 関数名
        let id = self.identifier();
        // 2) 引数一覧
        let params = self.parameter_list();
        // 3) 本体ブロックを読み、ノードを構築
        Node::new_function_declaration(id, params, self.function_body())
    }

    /// トップレベル要素を読み取ります（関数宣言または通常の文）。
    ///
    /// 流れ
    /// - 先頭トークンを `peek()`。
    /// - `function` キーワードなら 1 つ消費して `function_declaration()` を呼ぶ。
    /// - それ以外は `statement()` に委譲。
    /// - トークンが無ければ `None`。
    fn source_element(&mut self) -> Option<Rc<Node>> {
        let t = match self.t.peek() {
            Some(t) => t,
            None => return None,
        };

        match t {
            Token::Keyword(keyword) => {
                if keyword == "function" {
                    // 先頭が `function` のとき、キーワードを 1 つ消費してから関数宣言を読む
                    assert!(self.t.next().is_some());
                    self.function_declaration()
                } else {
                    // `function` 以外のキーワード（例: var, return など）は文として扱う
                    self.statement()
                }
            }
            _ => self.statement(), // キーワード以外も文として扱う
        }
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

    #[test]
    fn test_define_function() {
        // 関数宣言のみ: function foo() { return 42; }
        // 期待:
        // - Program.body = [ FunctionDeclaration ]
        // - FunctionDeclaration.id = Identifier("foo")
        // - params = []（引数なし）
        // - body = BlockStatement { body: [ ReturnStatement(NumericLiteral(42)) ] }
        let input = "function foo() { return 42; }".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let mut expected = Program::new();
        let mut body = Vec::new();
        body.push(Rc::new(Node::FunctionDeclaration {
            id: Some(Rc::new(Node::Identifier("foo".to_string()))),
            params: [].to_vec(),
            body: Some(Rc::new(Node::BlockStatement {
                body: [Some(Rc::new(Node::ReturnStatement {
                    argument: Some(Rc::new(Node::NumericLiteral(42))),
                }))]
                .to_vec(),
            })),
        }));
        expected.set_body(body);
        assert_eq!(expected, parser.parse_ast());
    }

    #[test]
    fn test_add_function_add_num() {
        // 関数宣言 + 変数宣言（関数呼び出し + 加算）
        // 入力: function foo() { return 42; } var result = foo() + 1;
        // 期待:
        // - Program.body[0] は上と同じ FunctionDeclaration(foo)
        // - Program.body[1] は VariableDeclaration:
        //   - id = Identifier("result")
        //   - init = AdditiveExpression('+', CallExpression(callee=Identifier("foo"), arguments=[]), NumericLiteral(1))
        let input = "function foo() { return 42; } var result = foo() + 1;".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let mut expected = Program::new();
        let mut body = Vec::new();
        body.push(Rc::new(Node::FunctionDeclaration {
            id: Some(Rc::new(Node::Identifier("foo".to_string()))),
            params: [].to_vec(),
            body: Some(Rc::new(Node::BlockStatement {
                body: [Some(Rc::new(Node::ReturnStatement {
                    argument: Some(Rc::new(Node::NumericLiteral(42))),
                }))]
                .to_vec(),
            })),
        }));
        body.push(Rc::new(Node::VariableDeclaration {
            declarations: [Some(Rc::new(Node::VariableDeclarator {
                id: Some(Rc::new(Node::Identifier("result".to_string()))),
                init: Some(Rc::new(Node::AdditiveExpression {
                    operator: '+',
                    left: Some(Rc::new(Node::CallExpression {
                        callee: Some(Rc::new(Node::Identifier("foo".to_string()))),
                        arguments: [].to_vec(),
                    })),
                    right: Some(Rc::new(Node::NumericLiteral(1))),
                })),
            }))]
            .to_vec(),
        }));
        expected.set_body(body);
        assert_eq!(expected, parser.parse_ast());
    }

    #[test]
    fn test_define_function_with_args() {
        // 引数ありの関数宣言: function foo(a, b) { return a + b; }
        // 期待:
        // - FunctionDeclaration.id = Identifier("foo")
        // - params = [Identifier("a"), Identifier("b")]
        // - body = BlockStatement { body: [ ReturnStatement(AdditiveExpression('+', Identifier("a"), Identifier("b"))) ] }
        let input = "function foo(a, b) { return a+b; }".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let mut expected = Program::new();
        let mut body = Vec::new();
        body.push(Rc::new(Node::FunctionDeclaration {
            id: Some(Rc::new(Node::Identifier("foo".to_string()))),
            params: [
                Some(Rc::new(Node::Identifier("a".to_string()))),
                Some(Rc::new(Node::Identifier("b".to_string()))),
            ]
            .to_vec(),
            body: Some(Rc::new(Node::BlockStatement {
                body: [Some(Rc::new(Node::ReturnStatement {
                    argument: Some(Rc::new(Node::AdditiveExpression {
                        operator: '+',
                        left: Some(Rc::new(Node::Identifier("a".to_string()))),
                        right: Some(Rc::new(Node::Identifier("b".to_string()))),
                    })),
                }))]
                .to_vec(),
            })),
        }));
        expected.set_body(body);
        assert_eq!(expected, parser.parse_ast());
    }
}
