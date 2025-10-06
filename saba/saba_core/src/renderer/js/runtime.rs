//! saba_core::renderer::js::runtime — 最小構成の JS 風ランタイム
//!
//! 役割
//! - パーサが生成した AST（`Program`/`Node`）を走査して評価します。
//! - 値は `RuntimeValue`（`Number(u64)` / `StringLiteral(String)`）。
//! - 演算は `+`（数値加算または文字列連結）と `-`（数値減算）。
//! - 変数は `Environment` に保持します（`var` 宣言、再代入、識別子参照）。
//! - `execute()` は `Program.body` を順に評価します。
//!
//! 実装メモ（用語ブリッジ）
//! - TS/Python の感覚: `RuntimeValue` は実行時値の共用体、`eval(node)` は再帰評価。
//! - 環境は `Rc<RefCell<Environment>>` でリンクし、外側の環境へ参照できます。
//! - no_std 前提のため、動的確保は `alloc` クレートに依存します。

use crate::renderer::js::ast::Node;
use crate::renderer::js::ast::Program;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::cell::RefCell;
use core::fmt::Display;
use core::fmt::Formatter;
use core::ops::Add;
use core::ops::Sub;

type VariableMap = Vec<(String, Option<RuntimeValue>)>;

/// https://262.ecma-international.org/#sec-ecmascript-language-types
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Number(u64),
    StringLiteral(String),
}

/// `+` 演算の定義（`RuntimeValue + RuntimeValue`）
///
/// ルール
/// - 両方が `Number` → 数値同士を加算して `Number` を返す。
/// - それ以外 → 文字列として連結し、`StringLiteral` を返す。
///   （TS/Python の「+ は数値加算/文字列結合」の感覚に近い）
impl Add<RuntimeValue> for RuntimeValue {
    type Output = RuntimeValue;

    fn add(self, rhs: RuntimeValue) -> RuntimeValue {
        // 数値 + 数値 → 数値加算
        if let (RuntimeValue::Number(left_num), RuntimeValue::Number(right_num)) = (&self, &rhs) {
            return RuntimeValue::Number(left_num + right_num);
        }

        // どちらかが文字列の場合は、両辺を文字列化して結合
        RuntimeValue::StringLiteral(self.to_string() + &rhs.to_string())
    }
}

/// `-` 演算の定義（`RuntimeValue - RuntimeValue`）
///
/// ルール
/// - 両方が `Number` → 数値同士を減算して `Number` を返す。
/// - それ以外 → ここでは「数値ではない」という意味合いで `u64::MIN` を返す（簡易な表現）。
impl Sub<RuntimeValue> for RuntimeValue {
    type Output = RuntimeValue;

    fn sub(self, rhs: RuntimeValue) -> RuntimeValue {
        // 数値 - 数値 → 数値減算
        if let (RuntimeValue::Number(left_num), RuntimeValue::Number(right_num)) = (&self, &rhs) {
            return RuntimeValue::Number(left_num - right_num);
        }

        // 数値以外の減算は「数ではない」を示す値として `u64::MIN` を返す（ここでの簡易な NaN 表現）
        RuntimeValue::Number(u64::MIN)
    }
}

/// `Display` の実装（`println!("{}", value)` や `format!(...)` 用の見た目）
impl Display for RuntimeValue {
    fn fmt(&self, f: &mut Formatter) -> core::fmt::Result {
        // まず `self` の中身に応じて表示用の文字列 `s` を用意します。
        // - Number(u64)        → 数値を文字列に変換
        // - StringLiteral(...) → その文字列をそのまま使う
        let s = match self {
            RuntimeValue::Number(value) => format!("{}", value),
            RuntimeValue::StringLiteral(value) => value.to_string(),
        };
        // 最後にフォーマッタ `f` に書き込みます。OK/Err を caller に返します。
        write!(f, "{}", s)
    }
}

/// 登録済みの関数（関数テーブルの 1 エントリ）
///
/// 役割
/// - `function foo(a, b) { ... }` の情報を保持します。
/// - ランタイムはここに溜めた関数を、`CallExpression` の評価時に名前で検索して実行します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    /// 関数名（例: "foo"）
    id: String,
    /// 仮引数（識別子ノードの列）。TS なら `Identifier[]` のイメージ。
    params: Vec<Option<Rc<Node>>>,
    /// 本体（`BlockStatement` ノード）。
    body: Option<Rc<Node>>,
}

impl Function {
    /// 関数エントリを作成します。
    ///
    /// 例
    /// - `function add(a, b) { return a + b; }` →
    ///   `Function::new("add".into(), vec![Identifier("a"), Identifier("b")], Some(BlockStatement{...}))`
    fn new(id: String, params: Vec<Option<Rc<Node>>>, body: Option<Rc<Node>>) -> Self {
        Self { id, params, body }
    }
}

/// 実行環境（Environment）
///
/// 役割
/// - 変数名と値を保持します（`variables`）。
/// - `outer` により外側の環境へ参照でき、スコープチェーンを表現します。
///
/// 参考: https://262.ecma-international.org/#sec-environment-records
#[derive(Debug, Clone)]
pub struct Environment {
    variables: VariableMap,
    outer: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    /// 新しい環境を作成します。
    /// - `outer` に親環境を渡すと、見つからない変数を親に委ねられます。
    fn new(outer: Option<Rc<RefCell<Environment>>>) -> Self {
        Self {
            variables: VariableMap::new(),
            outer,
        }
    }

    /// 変数 `name` の値を検索します。
    /// - まず自分の `variables` を線形検索し、見つかればその値を返します。
    /// - 見つからなければ `outer` に委譲して再帰的に探します（スコープチェーン）。
    pub fn get_variable(&self, name: String) -> Option<RuntimeValue> {
        for variable in &self.variables {
            if variable.0 == name {
                return variable.1.clone(); // (d1) 自分の環境で解決
            }
        }
        if let Some(env) = &self.outer {
            env.borrow_mut().get_variable(name) // (d2) 見つからない場合は外側へ委譲
        } else {
            None
        }
    }

    /// 新しい変数を追加します。
    /// - 既存名との重複チェックは行いません（同名を追加すると複数エントリが並ぶことに注意）。
    fn add_variable(&mut self, name: String, value: Option<RuntimeValue>) {
        self.variables.push((name, value));
    }

    /// 既存の変数 `name` の値を更新します。
    /// - 線形探索で見つけ、いったん削除してから新しい `(name, value)` を末尾へ追加します。
    fn update_variable(&mut self, name: String, value: Option<RuntimeValue>) {
        for i in 0..self.variables.len() {
            // もし変数を見つけた場合、今までの名前と値のペアを削除し、新しい値とのペアを追加する
            if self.variables[i].0 == name {
                self.variables.remove(i);
                self.variables.push((name, value));
                return;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsRuntime {
    functions: Vec<Function>,
    env: Rc<RefCell<Environment>>,
}

impl JsRuntime {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
            env: Rc::new(RefCell::new(Environment::new(None))),
        }
    }

    /// AST ノードを評価して値（`RuntimeValue`）を得ます。
    ///
    /// - 文は中の式や宣言へ委譲し、必要に応じて環境 `env` を更新します。
    /// - 式は部分式を再帰的に評価し、演算・参照・更新を行います。
    fn eval(
        &mut self,
        node: &Option<Rc<Node>>,
        env: Rc<RefCell<Environment>>,
    ) -> Option<RuntimeValue> {
        // `Option<Rc<Node>>` から中身を取り出し、無ければそのまま終了
        let node = match node {
            Some(n) => n,
            None => return None,
        };

        // `Rc<Node>` を借用して `&Node` にし、種類ごとに処理を分けます
        match node.borrow() {
            Node::ExpressionStatement(expr) => return self.eval(&expr, env.clone()),
            Node::AdditiveExpression {
                operator,
                left,
                right,
            } => {
                let left_value = match self.eval(&left, env.clone()) {
                    Some(value) => value,
                    None => return None,
                };
                let right_value = match self.eval(&right, env.clone()) {
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
                operator,
                left,
                right,
            } => {
                if operator != &'=' {
                    return None;
                }
                // 変数の再割り当て
                if let Some(node) = left {
                    if let Node::Identifier(id) = node.borrow() {
                        let new_value = self.eval(right, env.clone());
                        env.borrow_mut().update_variable(id.to_string(), new_value);
                        return None;
                    }
                }

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
            Node::VariableDeclaration { declarations } => {
                for declaration in declarations {
                    self.eval(&declaration, env.clone());
                }
                None
            }
            Node::VariableDeclarator { id, init } => {
                if let Some(node) = id {
                    if let Node::Identifier(id) = node.borrow() {
                        let init = self.eval(&init, env.clone());
                        env.borrow_mut().add_variable(id.to_string(), init);
                    }
                }
                None
            }
            Node::Identifier(name) => {
                match env.borrow_mut().get_variable(name.to_string()) {
                    Some(v) => Some(v),
                    // 変数名が初めて使用される場合は、まだ値は保存されていないので、文字列として扱う
                    // たとえば、var a = 42; のようなコードの場合、aはStringLiteralとして扱われる
                    None => Some(RuntimeValue::StringLiteral(name.to_string())),
                }
            }
            Node::StringLiteral(value) => Some(RuntimeValue::StringLiteral(value.to_string())),
            // ブロック: 中の文を順に評価し、最後に評価した値を返す
            Node::BlockStatement { body } => {
                let mut result: Option<RuntimeValue> = None;
                for stmt in body {
                    result = self.eval(&stmt, env.clone());
                }
                result
            }
            // return 文: 引数の式を評価し、その値をそのまま返す
            Node::ReturnStatement { argument } => {
                return self.eval(&argument, env.clone());
            }
            // 関数宣言: 関数名・引数リスト・本体を Function として登録（実行はしない）
            Node::FunctionDeclaration { id, params, body } => {
                if let Some(RuntimeValue::StringLiteral(id)) = self.eval(&id, env.clone()) {
                    // 本体は Rc<Node> を clone して保持（後で呼び出し時に評価）
                    let cloned_body = match body {
                        Some(b) => Some(b.clone()),
                        None => None,
                    };
                    self.functions
                        .push(Function::new(id, params.to_vec(), cloned_body));
                };
                None
            }
            Node::CallExpression { callee, arguments } => {
                // 呼び出しごとに新しいスコープ（環境）を作成し、外側に現在の env をリンク
                let new_env = Rc::new(RefCell::new(Environment::new(Some(env))));

                let callee_value = match self.eval(callee, new_env.clone()) {
                    Some(value) => value,
                    None => return None,
                };

                // 事前に登録された関数群から、名前が一致するものを探す
                let function = {
                    let mut f: Option<Function> = None;

                    for func in &self.functions {
                        if callee_value == RuntimeValue::StringLiteral(func.id.to_string()) {
                            f = Some(func.clone());
                        }
                    }

                    match f {
                        Some(f) => f,
                        None => panic!("function {:?} doesn't exist", callee),
                    }
                };

                // 実引数を、新スコープのローカル変数としてパラメータ名に束縛
                assert!(arguments.len() == function.params.len());
                for (i, item) in arguments.iter().enumerate() {
                    if let Some(RuntimeValue::StringLiteral(name)) =
                        self.eval(&function.params[i], new_env.clone())
                    {
                        new_env
                            .borrow_mut()
                            .add_variable(name, self.eval(item, new_env.clone()));
                    }
                }

                // 関数本体を新しいスコープで評価して返す
                self.eval(&function.body.clone(), new_env.clone())
            }
        }
    }

    /// `Program`（複数の文）を順に評価します。戻り値は捨てています。
    /// REPL のように「最後の値を返す」仕様にはしていません。
    pub fn execute(&mut self, program: &Program) {
        for node in program.body() {
            self.eval(&Some(node.clone()), self.env.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::js::ast::JsParser;
    use crate::renderer::js::token::JsLexer;
    use alloc::string::ToString; // `"...".to_string()` を使うためのトレイト（no_std では自動導入されない）

    // このテストではランタイムの「評価(eval)」を通じて振る舞いを確認します。
    // - リテラル評価（数値/文字列）
    // - 加算/減算の演算結果
    // - 変数宣言 `var` と識別子参照、再代入
    // - 複数文を順に評価する流れ

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
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
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
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
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
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_assign_variable() {
        // 変数宣言: var foo = 42;
        // - 宣言は環境を更新する副作用のみで、戻り値は None を期待
        let input = "var foo=42;".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [None];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_add_variable_and_num() {
        // 変数の利用と演算: var foo=42; foo + 1
        // - 1 文目で foo を 42 に束縛（戻り値 None）
        // - 2 文目で foo を参照して 42 + 1 → 43 を得る
        let input = "var foo=42; foo+1".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [None, Some(RuntimeValue::Number(43))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_reassign_variable() {
        // 再代入と参照: var foo=42; foo=1; foo
        // - 1 文目: 宣言（環境に foo=42 を記録）→ None
        // - 2 文目: 再代入（foo=1）→ None（評価値は返さない設計）
        // - 3 文目: 参照（foo）→ 1
        let input = "var foo=42; foo=1; foo".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [None, None, Some(RuntimeValue::Number(1))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_add_function_and_num() {
        // 関数定義 + 呼び出し + 加算
        // 入力: function foo() { return 42; } foo() + 1
        // 期待: [None, Some(Number(43))]
        // - 1 文目: 関数宣言（副作用のみ）
        // - 2 文目: 呼び出し foo() が 42 を返し、42 + 1 → 43
        let input = "function foo() { return 42; } foo()+1".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [None, Some(RuntimeValue::Number(43))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_define_function_with_args() {
        // 引数付き関数の呼び出しと加算
        // 入力: function foo(a, b) { return a + b; } foo(1, 2) + 3;
        // 期待: [None, Some(Number(6))]
        // - 1 文目: 関数宣言（副作用のみ）
        // - 2 文目: foo(1, 2) が 3 を返し、3 + 3 → 6
        let input = "function foo(a, b) { return a + b; } foo(1, 2) + 3;".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [None, Some(RuntimeValue::Number(6))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
            assert_eq!(expected[i], result);
            i += 1;
        }
    }

    #[test]
    fn test_local_variable() {
        // ローカル変数とグローバル変数のスコープ確認
        // 入力: var a=42; function foo() { var a=1; return a; } foo() + a
        // 期待: [None, None, Some(Number(43))]
        // - 1 文目: グローバル a=42 を束縛
        // - 2 文目: 関数宣言（関数内にローカル a=1）
        // - 3 文目: foo() は 1 を返し、1 + 42 → 43（スコープの区別）
        let input = "var a=42; function foo() { var a=1; return a; } foo()+a".to_string();
        let lexer = JsLexer::new(input);
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();
        let mut runtime = JsRuntime::new();
        let expected = [None, None, Some(RuntimeValue::Number(43))];
        let mut i = 0;

        for node in ast.body() {
            let result = runtime.eval(&Some(node.clone()), runtime.env.clone());
            assert_eq!(expected[i], result);
            i += 1;
        }
    }
}
