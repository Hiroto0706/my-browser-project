//! CSSOM（CSS Object Model）の最小実装（初心者向け）
//!
//! これは CSS の“見た目のオブジェクト表現”です。実ブラウザでは、文字列の CSS を
//! 1) トークナイズ → 2) パース → 3) CSSOM（StyleSheet/Rule/Declaration など）
//! に落とし込み、DOM と組み合わせてスタイル計算を行います。
//!
//! 対応関係（実CSSのどの部分？）
//! - `StyleSheet` … スタイルシート全体。ファイル1枚・`<style>`1つ分。
//!   例: `p { color: red; } h1 { font-size: 40; }`
//! - `QualifiedRule` … 1つのルール。セレクタ + 宣言ブロックの組。
//!   例: `p { color: red; }` が1ルール。
//! - `Selector` … セレクタ。`p`, `.class`, `#id` など（ここでは3種を簡易対応）。
//! - `Declaration` … 宣言1つ。プロパティ名と値のペア。
//!   例: `color: red` は `property="color"`, `value=Ident("red")`。
//!
//! 言語ブリッジ（TS / Python / Go）
//! - `StyleSheet.rules: Vec<QualifiedRule>` … 配列にルールが並ぶ（TS: QualifiedRule[]、Python: list）。
//! - 値（`ComponentValue`）は現状 CSS トークンをそのまま使います（型の最小化）。
//!   実用では `Length(px)`, `Color(Rgb)`, `String`, `Url` などの型に解釈します。
//!
//! 例（文字列→CSSOMのイメージ）
//! ```text
//! 入力:  p { color: red; }  h1 { font-size: 40; }
//! CSSOM:
//! StyleSheet {
//!   rules: [
//!     QualifiedRule { selector: TypeSelector("p"),  declarations: [
//!       Declaration { property: "color", value: Ident("red") }
//!     ] },
//!     QualifiedRule { selector: TypeSelector("h1"), declarations: [
//!       Declaration { property: "font-size", value: Number(40.0) }
//!     ] }
//!   ]
//! }
//! ```

use crate::alloc::string::ToString;
use crate::renderer::css::token::CssToken;
use crate::renderer::css::token::CssTokenizer;
use alloc::string::String;
use alloc::vec::Vec;
use core::iter::Peekable;

#[derive(Debug, Clone)]
pub struct CssParser {
    t: Peekable<CssTokenizer>, // トークナイザを先読み可能に包む（lookahead が必要になるため）
}

impl CssParser {
    pub fn new(t: CssTokenizer) -> Self {
        Self { t: t.peekable() }
    }

    // 次のトークンを消費し、ComponentValueとして返す
    /// 次のトークンを“そのまま”値として受け取る（ComponentValue = CssToken の簡易方針）
    /// 例: `color: red;` の直後に `Ident("red")` が現れる想定。
    /// 本格実装ではここで `CssToken` → `Value`（Color/Length/String 等）に解釈します。
    /// 仕様: https://www.w3.org/TR/css-syntax-3/#consume-component-value
    fn consume_component_value(&mut self) -> ComponentValue {
        self.t
            .next()
            .expect("should have a token in consume_component_value")
    }

    // 識別子トークンを消費し、文字列を取得する
    /// 入力例: `color: red` の `color` 部分や、`font-size` のようなプロパティ名。
    /// 不正（Ident 以外）なら明確なメッセージで失敗させ、解析の不整合に気づけるようにします。
    fn consume_ident(&mut self) -> String {
        let token = match self.t.next() {
            Some(t) => t,
            None => panic!("should have a token but got None"),
        };

        match token {
            CssToken::Ident(ref ident) => ident.to_string(),
            _ => {
                panic!("Parse error: {:?} is an unexpected token.", token);
            }
        }
    }

    // ひとつの宣言を解釈する
    /// 入力例: `color: red;` → Declaration { property: "color", value: Ident("red") }
    /// 仕様: https://www.w3.org/TR/css-syntax-3/#consume-a-declaration
    fn consume_declaration(&mut self) -> Option<Declaration> {
        if self.t.peek().is_none() {
            return None;
        }

        // 1) Declaration 構造体を初期化する
        let mut declaration = Declaration::new();
        // 2) Declaration構造体のプロパティに識別子を設定する
        declaration.set_property(self.consume_ident());

        // 3) もし次のトークンがコロンでない場合、パースエラーなので、Noneを返す
        match self.t.next() {
            Some(token) => match token {
                CssToken::Colon => {}
                _ => return None,
            },
            None => return None,
        }

        // 4) Declaration構造体の値にコンポーネント値を設定する
        declaration.set_value(self.consume_component_value());

        Some(declaration)
    }

    // consume_list_of_declarationsでは、複数の宣言を解釈する
    /// 入力例: `{ color: red; font-size: 40; }` → [Declaration("color", Ident("red")), Declaration("font-size", Number(40))]
    /// 仕様: https://www.w3.org/TR/css-syntax-3/#consume-a-list-of-declarations
    fn consume_list_of_declarations(&mut self) -> Vec<Declaration> {
        let mut declarations = Vec::new(); // 宣言ベクタを初期化する

        loop {
            let token = match self.t.peek() {
                Some(t) => t,
                None => return declarations,
            };

            match token {
                // 閉じ波括弧が現れるまで宣言を作成しベクタに追加する
                CssToken::CloseCurly => {
                    assert_eq!(self.t.next(), Some(CssToken::CloseCurly));
                    return declarations;
                }
                // セミコロンのとき一つの宣言が終了したことを表す
                CssToken::SemiColon => {
                    assert_eq!(self.t.next(), Some(CssToken::SemiColon));
                    // 一つの宣言が終了。何もしない（次のプロパティへ）
                }
                // 識別子トークンの時、一つの宣言を解釈し、ベクタに追加する
                CssToken::Ident(ref _ident) => {
                    if let Some(declaration) = self.consume_declaration() {
                        declarations.push(declaration);
                    }
                }
                _ => {
                    self.t.next();
                }
            }
        }
    }

    /// セレクタを 1 つ解釈する（Type / Class / ID の簡易版）
    /// 入力例:
    /// - `#main {` → IdSelector("main")
    /// - `.note {` → ClassSelector("note")
    /// - `a:hover {` → TypeSelector("a") として扱い、`:` 以降は `{` まで読み飛ばす（簡易）
    fn consume_selector(&mut self) -> Selector {
        let token = match self.t.next() {
            Some(t) => t,
            None => panic!("should have a token but got None"),
        };

        match token {
            CssToken::HashToken(value) => Selector::IdSelector(value[1..].to_string()), // ハッシュトークンの時、IDセレクタを作成して返す（先頭の # を落とす）
            CssToken::Delim(delim) => {
                if delim == '.' {
                    // ピリオドの時、クラスセレクタを作成して返す
                    return Selector::ClassSelector(self.consume_ident());
                }
                panic!("Parse error: {:?} is an unexpected token.", token);
            }
            CssToken::Ident(ident) => {
                // 識別子の時、タイプセレクタを作成して返す
                // a:hoverのようなセレクタはタグ名のセレクタとして扱うため、
                // もしコロン（:）が出てきた場合は宣言ブロックの開始直前まで
                // トークンを進める
                if self.t.peek() == Some(&CssToken::Colon) {
                    while self.t.peek() != Some(&CssToken::OpenCurly) {
                        self.t.next();
                    }
                }
                Selector::TypeSelector(ident.to_string())
            }
            CssToken::AtKeyword(_keyword) => {
                // @から始まるルールを無視するために、宣言ブロックの開始直前まで
                // トークンを進める
                while self.t.peek() != Some(&CssToken::OpenCurly) {
                    self.t.next();
                }
                Selector::UnknownSelector
            }
            _ => {
                self.t.next();
                Selector::UnknownSelector
            }
        }
    }

    /// Qualified Rule（セレクタ + 宣言ブロック）を 1 つ解釈する
    ///
    /// 役割
    /// - セレクタ部分を読み（例: `p`, `.note`, `#main`）、`{` が来たら宣言ブロックを `}` まで解釈します。
    /// - 読み終えたら `Some(QualifiedRule)` を返し、入力が尽きたら `None` を返します。
    ///
    /// 入力例 → 出力イメージ
    /// - `p { color: red; }` → selector=TypeSelector("p"), declarations=[ Declaration("color", Ident("red")) ]
    /// - `.note { font-size: 40; }` → selector=ClassSelector("note"), declarations=[ Declaration("font-size", Number(40.0)) ]
    ///
    /// 関連仕様
    /// - consume-qualified-rule: https://www.w3.org/TR/css-syntax-3/#consume-qualified-rule
    /// - qualified-rule:         https://www.w3.org/TR/css-syntax-3/#qualified-rule
    /// - style rules:            https://www.w3.org/TR/css-syntax-3/#style-rules
    fn consume_qualified_rule(&mut self) -> Option<QualifiedRule> {
        let mut rule = QualifiedRule::new();

        loop {
            let token = match self.t.peek() {
                Some(t) => t,
                None => return None,
            };

            match token {
                CssToken::OpenCurly => {
                    // `{` に到達 → 宣言ブロック開始。中身（declarations）を読み切って返す。
                    assert_eq!(self.t.next(), Some(CssToken::OpenCurly));
                    rule.set_declarations(self.consume_list_of_declarations());
                    return Some(rule);
                }
                _ => {
                    // それ以外の時、ルールのセレクタとして扱うためセレクタを解釈し、
                    // ルールの selector フィールドに設定する。
                    rule.set_selector(self.consume_selector());
                }
            }
        }
    }

    /// スタイルルールの並びを EOF まで解釈する
    ///
    /// 役割
    /// - 通常の style rule を次々に `consume_qualified_rule` で読み取り、ベクタに集めます。
    /// - `@` で始まる at-rule（@import/@media 等）は本書の簡易実装では無視（読み飛ばし）の方針です。
    ///
    /// 入力例 → 出力イメージ
    /// - `p{...} h1{...}` → vec![ QualifiedRule(p, ...), QualifiedRule(h1, ...) ]
    ///
    /// 仕様: https://www.w3.org/TR/css-syntax-3/#consume-a-list-of-rules
    fn consume_list_of_rules(&mut self) -> Vec<QualifiedRule> {
        // 空のベクタを作成する
        let mut rules = Vec::new();

        loop {
            let token = match self.t.peek() {
                Some(t) => t,
                None => return rules,
            };
            match token {
                // AtKeywordトークンが出てきた場合、他のCSSをインポートする@import、
                // メディアクエリを表す@mediaなどのルールが始まることを表す
                CssToken::AtKeyword(_keyword) => {
                    let _rule = self.consume_qualified_rule();
                    // しかし、本書のブラウザでは@から始まるルールはサポート
                    // しないので、無視をする
                }
                _ => {
                    // 1つの style rule を解釈し、成功したらベクタに追加する
                    let rule = self.consume_qualified_rule();
                    match rule {
                        Some(r) => rules.push(r),
                        None => return rules,
                    }
                }
            }
        }
    }

    /// スタイルシート全体を解釈し、StyleSheet を返す
    ///
    /// 役割
    /// - 見出し（ルールの列）を `consume_list_of_rules` で構築し、`StyleSheet.rules` に設定します。
    /// - この戻り値（CSSOM）は、後工程の「スタイル計算」（DOMとマッチングして計算）で用います。
    ///
    /// 仕様: https://www.w3.org/TR/css-syntax-3/#parse-stylesheet
    pub fn parse_stylesheet(&mut self) -> StyleSheet {
        // StyleSheet構造体のインスタンスを作成する
        let mut sheet = StyleSheet::new();

        // トークン列からルールのリストを作成し、StyleSheetのフィールドに設定する
        sheet.set_rules(self.consume_list_of_rules());
        sheet
    }
}

/// https://www.w3.org/TR/cssom-1/#cssstylesheet
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSheet {
    /// https://drafts.csswg.org/cssom/#dom-cssstylesheet-cssrules
    pub rules: Vec<QualifiedRule>, // スタイルシートに含まれるルール列（読み順）
}

impl StyleSheet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn set_rules(&mut self, rules: Vec<QualifiedRule>) {
        self.rules = rules;
    }
}

/// https://www.w3.org/TR/css-syntax-3/#qualified-rule
#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedRule {
    /// https://www.w3.org/TR/selectors-4/#typedef-selector-list
    /// The prelude of the qualified rule is parsed as a <selector-list>.
    pub selector: Selector, // セレクタ（簡易: 1つだけを保持。複合/コンビネータは未対応）
    /// https://www.w3.org/TR/css-syntax-3/#parse-a-list-of-declarations
    /// The content of the qualified rule’s block is parsed as a list of declarations.
    pub declarations: Vec<Declaration>, // ブロック `{ ... }` 内の宣言一覧
}

impl QualifiedRule {
    pub fn new() -> Self {
        Self {
            selector: Selector::TypeSelector("".to_string()),
            declarations: Vec::new(),
        }
    }

    pub fn set_selector(&mut self, selector: Selector) {
        self.selector = selector;
    }

    pub fn set_declarations(&mut self, declarations: Vec<Declaration>) {
        self.declarations = declarations;
    }
}

/// https://www.w3.org/TR/selectors-4/
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// https://www.w3.org/TR/selectors-4/#type-selectors
    TypeSelector(String), // 例: "p", "h1"
    /// https://www.w3.org/TR/selectors-4/#class-html
    ClassSelector(String), // 例: ".note" → ClassSelector("note") として保持
    /// https://www.w3.org/TR/selectors-4/#id-selectors
    IdSelector(String), // 例: "#main" → IdSelector("main") として保持
    /// パース中にエラーが起こったときに使用されるセレクタ
    UnknownSelector,
}

/// https://www.w3.org/TR/css-syntax-3/#declaration
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub property: String,      // 例: "color", "font-size"
    pub value: ComponentValue, // 例: Ident("red"), Number(40.0)
}

impl Declaration {
    pub fn new() -> Self {
        Self {
            property: String::new(),
            value: ComponentValue::Ident(String::new()),
        }
    }

    pub fn set_property(&mut self, property: String) {
        self.property = property;
    }

    pub fn set_value(&mut self, value: ComponentValue) {
        self.value = value;
    }
}

// 値（component value）を、まずは「CSSトークンそのもの」で持つ簡易版。
// 例: Ident("red"), Number(40.0), StringToken("Hey").
// 本格的には `enum Value { Color(Color), Length(Length), String(String), ... }`
// のように意味のある型へ解釈していきます。
pub type ComponentValue = CssToken;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_empty() {
        // 準備: 空のスタイル文字列
        let style = "".to_string();
        // トークナイズ→パース（CSSOM化）
        let t = CssTokenizer::new(style);
        let cssom = CssParser::new(t).parse_stylesheet();

        // 期待: ルールは 0 件（rules は空）
        assert_eq!(cssom.rules.len(), 0);
    }

    #[test]
    fn test_one_rule() {
        // 準備: 単一ルール（要素セレクタ + 宣言1つ）
        // 入力: p { color: red; }
        let style = "p { color: red; }".to_string();
        let t = CssTokenizer::new(style);
        let cssom = CssParser::new(t).parse_stylesheet();

        // 期待CSSOMを手で組み立てる
        let mut rule = QualifiedRule::new();
        rule.set_selector(Selector::TypeSelector("p".to_string()));
        let mut declaration = Declaration::new();
        declaration.set_property("color".to_string());
        declaration.set_value(ComponentValue::Ident("red".to_string()));
        rule.set_declarations(vec![declaration]);

        let expected = [rule];
        assert_eq!(cssom.rules.len(), expected.len());
        for (got, exp) in cssom.rules.iter().zip(expected.iter()) {
            assert_eq!(exp, got);
        }
    }

    #[test]
    fn test_id_selector() {
        // 準備: ID セレクタ
        // 入力: #id { color: red; }
        // 期待: selector=IdSelector("id")（先頭の # を除いた値）
        let style = "#id { color: red; }".to_string();
        let t = CssTokenizer::new(style);
        let cssom = CssParser::new(t).parse_stylesheet();

        let mut rule = QualifiedRule::new();
        rule.set_selector(Selector::IdSelector("id".to_string()));
        let mut declaration = Declaration::new();
        declaration.set_property("color".to_string());
        declaration.set_value(ComponentValue::Ident("red".to_string()));
        rule.set_declarations(vec![declaration]);

        let expected = [rule];
        assert_eq!(cssom.rules.len(), expected.len());
        for (got, exp) in cssom.rules.iter().zip(expected.iter()) {
            assert_eq!(exp, got);
        }
    }

    #[test]
    fn test_class_selector() {
        // 準備: クラスセレクタ
        // 入力: .class { color: red; }
        // 期待: selector=ClassSelector("class")（先頭の '.' を除いた値）
        let style = ".class { color: red; }".to_string();
        let t = CssTokenizer::new(style);
        let cssom = CssParser::new(t).parse_stylesheet();

        let mut rule = QualifiedRule::new();
        rule.set_selector(Selector::ClassSelector("class".to_string()));
        let mut declaration = Declaration::new();
        declaration.set_property("color".to_string());
        declaration.set_value(ComponentValue::Ident("red".to_string()));
        rule.set_declarations(vec![declaration]);

        let expected = [rule];
        assert_eq!(cssom.rules.len(), expected.len());
        for (got, exp) in cssom.rules.iter().zip(expected.iter()) {
            assert_eq!(exp, got);
        }
    }

    #[test]
    fn test_multiple_rules() {
        // 準備: 複数ルールと異なる値型（StringToken/Number/Ident）の混在を検証
        // 入力: p { content: "Hey"; } h1 { font-size: 40; color: blue; }
        let style = "p { content: \"Hey\"; } h1 { font-size: 40; color: blue; }".to_string();
        let t = CssTokenizer::new(style);
        let cssom = CssParser::new(t).parse_stylesheet();

        // 期待1: p { content: "Hey"; }
        let mut rule1 = QualifiedRule::new();
        rule1.set_selector(Selector::TypeSelector("p".to_string()));
        let mut declaration1 = Declaration::new();
        declaration1.set_property("content".to_string());
        declaration1.set_value(ComponentValue::StringToken("Hey".to_string()));
        rule1.set_declarations(vec![declaration1]);

        // 期待2: h1 { font-size: 40; color: blue; }
        let mut rule2 = QualifiedRule::new();
        rule2.set_selector(Selector::TypeSelector("h1".to_string()));
        let mut declaration2 = Declaration::new();
        declaration2.set_property("font-size".to_string());
        declaration2.set_value(ComponentValue::Number(40.0));
        let mut declaration3 = Declaration::new();
        declaration3.set_property("color".to_string());
        declaration3.set_value(ComponentValue::Ident("blue".to_string()));
        rule2.set_declarations(vec![declaration2, declaration3]);

        let expected = [rule1, rule2];
        assert_eq!(cssom.rules.len(), expected.len());
        for (got, exp) in cssom.rules.iter().zip(expected.iter()) {
            assert_eq!(exp, got);
        }
    }
}
