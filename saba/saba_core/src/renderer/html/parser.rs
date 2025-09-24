use crate::renderer::dom::node::Element;
use crate::renderer::dom::node::ElementKind;
use crate::renderer::dom::node::Node;
use crate::renderer::dom::node::NodeKind;
use crate::renderer::dom::node::Window;
use crate::renderer::html::attribute::Attribute;
use crate::renderer::html::token::HtmlToken;
use crate::renderer::html::token::HtmlTokenizer;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::str::FromStr;

/// https://html.spec.whatwg.org/multipage/parsing.html#the-insertion-mode
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    AfterHead,
    InBody,
    Text,
    AfterBody,
    AfterAfterBody,
}

#[derive(Debug, Clone)]
pub struct HtmlParser {
    window: Rc<RefCell<Window>>,
    mode: InsertionMode,
    /// https://html.spec.whatwg.org/multipage/parsing.html#original-insertion-mode
    original_insertion_mode: InsertionMode,
    /// https://html.spec.whatwg.org/multipage/parsing.html#the-stack-of-open-elements
    stack_of_open_elements: Vec<Rc<RefCell<Node>>>, // ブラウザが使用するスタック(final-in-last-out)
    t: HtmlTokenizer,
}

impl HtmlParser {
    pub fn new(t: HtmlTokenizer) -> Self {
        Self {
            window: Rc::new(RefCell::new(Window::new())),
            mode: InsertionMode::Initial,
            original_insertion_mode: InsertionMode::Initial,
            stack_of_open_elements: Vec::new(),
            t,
        }
    }

    // stack_of_open_elementsスタックに存在する全ての要素を確認し、特定の種類がある場合にtrueを返す
    fn contain_in_stack(&mut self, element_kind: ElementKind) -> bool {
        for i in 0..self.stack_of_open_elements.len() {
            if self.stack_of_open_elements[i].borrow().element_kind() == Some(element_kind) {
                return true;
            }
        }

        false
    }

    // stack_of_open_elementsスタックから特定の種類の要素が現れるまでノードを取り出し続ける
    fn pop_until(&mut self, element_kind: ElementKind) {
        assert!(
            self.contain_in_stack(element_kind),
            "stack doesn't have an element {:?}",
            element_kind,
        );

        loop {
            let current = match self.stack_of_open_elements.pop() {
                Some(n) => n,
                None => return,
            };

            if current.borrow().element_kind() == Some(element_kind) {
                return;
            }
        }
    }

    // stack_of_open_elementsスタックから1つのノードを取り出し、そのノードが特定の種類と一致する場合にtrueを返す。異なる種類の場合はfalseを返す。
    fn pop_current_node(&mut self, element_kind: ElementKind) -> bool {
        let current = match self.stack_of_open_elements.last() {
            Some(n) => n,
            None => return false,
        };

        if current.borrow().element_kind() == Some(element_kind) {
            self.stack_of_open_elements.pop();
            return true;
        }

        false
    }

    fn create_char(&self, c: char) -> Node {
        let mut s = String::new();
        s.push(c);
        Node::new(NodeKind::Text(s))
    }

    /// 文字トークンを DOM に反映する（テキストノードの生成/追記）
    ///
    /// 実ブラウザにおける挙動に対応
    /// - HTML パーサは「テキストノードの連結（coalescing）」を行い、
    ///   連続する文字は同じ Text ノードへまとめます。
    /// - 空白や改行の扱いは挿入モードや要素種別で変わりますが、ここでは最小仕様として
    ///   改行/スペースはスキップしています（簡易ホワイトスペース抑制）。
    ///
    /// 実装の要点
    /// - `stack_of_open_elements` の末尾（現在の挿入先）に対して処理します。
    /// - すでに直近が `Text` ならその文字列へ `push`（coalescing）。
    /// - そうでなければ新しい `Text` ノードを作って親子/兄弟リンクを張ります。
    fn insert_char(&mut self, c: char) {
        let current = match self.stack_of_open_elements.last() {
            Some(n) => n.clone(),
            None => return,
        };

        // 1) 直近ノードが Text なら、そこへ追記（テキストの連結）。
        //    実ブラウザも「連続する文字トークンは同一の Text ノードにまとめる」動きをします。
        if let NodeKind::Text(ref mut s) = current.borrow_mut().kind {
            s.push(c);
            return;
        }

        // 2) 簡易ホワイトスペース制御: 改行/スペースはスキップ。
        //    （本来はインサーションモードや CSS の空白折り畳み規則に依存。最小実装として抑制。）
        if c == '\n' || c == ' ' {
            return;
        }

        // 3) それ以外の文字は、新しい Text ノードを生成。
        let node = Rc::new(RefCell::new(self.create_char(c)));

        // 4) 親（current）に子がすでに居る場合は、先頭子の `next_sibling` にぶら下げる。
        //    NOTE: 一般的には「最後の子の next_sibling に追加」するのが自然ですが、
        //    ここでは簡略化のため first_child の next として接続しています。
        //    将来的に正確な木構造を保つなら、最後の子を辿って末尾に繋ぐのが安全です（TODO候補）。
        if current.borrow().first_child().is_some() {
            current
                .borrow()
                .first_child()
                .unwrap()
                .borrow_mut()
                .set_next_sibling(Some(node.clone()));
        } else {
            // 親に子がいなければ最初の子として登録。
            current.borrow_mut().set_first_child(Some(node.clone()));
        }

        // 5) 親の last_child を更新し、子から親への逆リンク（parent）を設定。
        current.borrow_mut().set_last_child(Rc::downgrade(&node));
        node.borrow_mut().set_parent(Rc::downgrade(&current));

        // 6) “現在の挿入位置”をこの Text ノードへ更新。
        //    以降の連続する文字は上の 1) の分岐で同一ノードへ連結されます。
        self.stack_of_open_elements.push(node);
    }

    fn create_element(&self, tag: &str, attributes: Vec<Attribute>) -> Node {
        Node::new(NodeKind::Element(Element::new(tag, attributes)))
    }

    /// 開始タグを DOM に挿入する（要素ノードの生成と親子/兄弟リンクの更新）
    ///
    /// 実ブラウザでいう「ツリービルダー」の仕事に相当します。
    /// - 直近の挿入先（stack_of_open_elements の末尾、なければ Document）に、
    ///   新しい Element ノードを子として追加します。
    /// - 兄弟がいる場合は、最後の子の直後に連結します（末尾へ追加）。
    /// - 追加後、その要素を“現在開いている要素”としてスタックに push します。
    fn insert_element(&mut self, tag: &str, attributes: Vec<Attribute>) {
        let window = self.window.borrow();
        // 1) 挿入先（カレントノード）を決める
        let current = match self.stack_of_open_elements.last() {
            // 現在開いている要素スタックの最後のノードを取得
            Some(n) => n.clone(),
            None => window.document(), // スタックが空なら Document 直下に挿入
        };

        // 2) 新しい要素ノードを作成（タグ名と属性を保持）
        //    Rc<RefCell<_>> に包むことで“共有 + 内部可変”にします（DOM 編集がしやすい）。
        let node = Rc::new(RefCell::new(self.create_element(tag, attributes)));

        // 3) 末尾に追加するため、最後の子（last_sibling）を探す
        if current.borrow().first_child().is_some() {
            // すでに子要素がある場合は、先頭から next_sibling を辿って末尾へ
            let mut last_sibling = current.borrow().first_child();
            loop {
                last_sibling = match last_sibling {
                    Some(ref node) => {
                        if node.borrow().next_sibling().is_some() {
                            node.borrow().next_sibling()
                        } else {
                            break;
                        }
                    }
                    None => unimplemented!("last_sibling should be Some"),
                };
            }

            // 4) 末尾の兄弟の next_sibling と、新ノードの previous_sibling を接続
            last_sibling
                .as_ref()
                .unwrap()
                .borrow_mut()
                .set_next_sibling(Some(node.clone())); // 兄弟ノードの直後に追加
            node.borrow_mut().set_previous_sibling(Rc::downgrade(
                &last_sibling.expect("last_sibling should be Some"),
            ))
        } else {
            // 兄弟ノードが存在しない場合（= 子がまだいない）
            current.borrow_mut().set_first_child(Some(node.clone())); // 最初の子として登録
        }

        // 5) 親子リンクの仕上げ（last_child と parent）
        current.borrow_mut().set_last_child(Rc::downgrade(&node)); // 親の最後の子を更新
        node.borrow_mut().set_parent(Rc::downgrade(&current)); // 子から親への逆リンク

        // 6) ツリービルダーの規則: 開始タグを見たら、その要素を「開いている要素スタック」に積む
        self.stack_of_open_elements.push(node);
    }

    /// HTML トークン列から DOM ツリーを組み立てる（ツリービルダーの簡易実装）
    ///
    /// 実ブラウザとの対応
    /// - HTML パーサは「トークナイザ（字句解析）」と「ツリービルダー（構文解析）」の二段構えです。
    /// - ここでは `HtmlTokenizer` が1トークンずつ供給し、`InsertionMode`（挿入モード）に応じて
    ///   DOM ノード（Element/Text）を追加・スタック操作します。
    /// - 省略可能な要素（html/head/body）は、仕様に倣い必要に応じて自動挿入します。
    pub fn construct_tree(&mut self) -> Rc<RefCell<Window>> {
        // トークナイザから最初のトークンを受け取る。
        let mut token = self.t.next();

        // トークンが存在する間、現在の挿入モードに従って処理を進める。
        while token.is_some() {
            match self.mode {
                InsertionMode::Initial => {
                    // 初期モード: この実装では DOCTYPE をサポートしない。
                    // そのため "<!doctype html>" のような入力は Char として届くが、ここでは捨てる。
                    if let Some(HtmlToken::Char(_)) = token {
                        token = self.t.next();
                        continue;
                    }

                    // DOCTYPE を読み飛ばしたら BeforeHtml へ遷移。
                    self.mode = InsertionMode::BeforeHtml;
                    continue;
                }
                InsertionMode::BeforeHtml => {
                    // html 要素の直前段階。先頭の空白は無視し、<html> を待つ。
                    match token {
                        Some(HtmlToken::Char(c)) => {
                            // 次のトークンが空白文字や改行文字の時
                            if c == ' ' || c == '\n' {
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::StartTag {
                            ref tag,
                            self_closing: _,
                            ref attributes,
                        }) => {
                            if tag == "html" {
                                // <html> を受け取ったので、要素を挿入して BeforeHead へ。
                                self.insert_element(tag, attributes.to_vec());
                                self.mode = InsertionMode::BeforeHead;
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::EndTag { ref tag }) => {
                            // 想定外の終了タグは無視（仕様にあるエラー回復のごく一部）。
                            if tag != "head" || tag != "body" || tag != "html" || tag != "br" {
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::Eof) | None => {
                            // 入力が空のときは空の Document を返す。
                            return self.window.clone();
                        }
                    }
                    // ここまで来たら <html> が省略されているとみなし、自動挿入する。
                    self.insert_element("html", Vec::new());
                    self.mode = InsertionMode::BeforeHead;
                    continue;
                }
                InsertionMode::BeforeHead => {
                    // <head> の直前。空白は無視し、<head> を待つ。
                    match token {
                        Some(HtmlToken::Char(c)) => {
                            if c == ' ' || c == '\n' {
                                // 次のトークンが空白文字や改行文字の時
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::StartTag {
                            ref tag,
                            self_closing: _,
                            ref attributes,
                        }) => {
                            if tag == "head" {
                                // <head> を受け取ったので挿入し、InHead へ。
                                self.insert_element(tag, attributes.to_vec());
                                self.mode = InsertionMode::InHead;
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::Eof) | None => {
                            // 早期終端: 現在の Document を返す。
                            return self.window.clone();
                        }
                        _ => {}
                    }
                    // <head> が省略されたとみなし、自動挿入（仕様でも許容）。
                    self.insert_element("head", Vec::new());
                    self.mode = InsertionMode::InHead;
                    continue;
                }

                // 主にheadの終了タグ、style開始タグ、script開始タグを取り扱う
                InsertionMode::InHead => {
                    // <head> 内の処理。style/script はテキストモードへ遷移。
                    match token {
                        Some(HtmlToken::Char(c)) => {
                            // 次のトークンが空白文字や改行文字の時
                            if c == ' ' || c == '\n' {
                                self.insert_char(c);
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::StartTag {
                            ref tag,
                            self_closing: _,
                            ref attributes,
                        }) => {
                            if tag == "style" || tag == "script" {
                                // StartTagかつタグの名前がstyleまたはscriptだった時
                                self.insert_element(tag, attributes.to_vec());
                                self.original_insertion_mode = self.mode;
                                self.mode = InsertionMode::Text;
                                token = self.t.next();
                                continue;
                            }
                            // 仕様書には定められていないが、このブラウザは仕様を全て実装している
                            // わけではないので、<head>が省略されているHTML文書を扱うために必要。
                            // これがないと<head>が省略されているHTML文書で無限ループが発生
                            if tag == "body" {
                                self.pop_until(ElementKind::Head);
                                self.mode = InsertionMode::AfterHead;
                                continue;
                            }
                            if let Ok(_element_kind) = ElementKind::from_str(tag) {
                                // <meta> など未対応タグが来た時の簡易的な抜け道として AfterHead へ。
                                self.pop_until(ElementKind::Head);
                                self.mode = InsertionMode::AfterHead;
                                continue;
                            }
                        }
                        Some(HtmlToken::EndTag { ref tag }) => {
                            // EndTag かつ名前が head のとき
                            if tag == "head" {
                                self.mode = InsertionMode::AfterHead;
                                token = self.t.next();
                                self.pop_until(ElementKind::Head);
                                continue;
                            }
                        }
                        Some(HtmlToken::Eof) | None => {
                            // 入力終端: ここまでの Document を返す。
                            return self.window.clone();
                        }
                    }
                    // <meta>や<title>などのサポートしていないタグは無視する
                    token = self.t.next();
                    continue;
                }

                // body開始タグを扱う
                InsertionMode::AfterHead => {
                    // <head> の後。<body> を待ち、来なければ自動挿入。
                    match token {
                        Some(HtmlToken::Char(c)) => {
                            // 空白や改行の時
                            if c == ' ' || c == '\n' {
                                self.insert_char(c);
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::StartTag {
                            ref tag,
                            self_closing: _,
                            ref attributes,
                        }) => {
                            if tag == "body" {
                                // 次のタグがStartTagでかつタグ名がbodyのとき
                                self.insert_element(tag, attributes.to_vec());
                                token = self.t.next();
                                self.mode = InsertionMode::InBody;
                                continue;
                            }
                        }
                        Some(HtmlToken::Eof) | None => {
                            return self.window.clone();
                        }
                        _ => {}
                    }
                    // ここまで来たら <body> を省略とみなし、自動挿入。
                    self.insert_element("body", Vec::new());
                    self.mode = InsertionMode::InBody;
                    continue;
                }

                // HTMLのbodyタグのコンテンツを扱う
                // div, h1, pタグなどのことを指す
                InsertionMode::InBody => {
                    // 本文の主要要素を処理。開始タグは要素を挿入、終了タグはスタックを畳む。
                    match token {
                        Some(HtmlToken::StartTag {
                            ref tag,
                            self_closing: _,
                            ref attributes,
                        }) => match tag.as_str() {
                            "p" => {
                                self.insert_element(tag, attributes.to_vec());
                                token = self.t.next();
                                continue;
                            }
                            "h1" | "h2" => {
                                self.insert_element(tag, attributes.to_vec());
                                token = self.t.next();
                                continue;
                            }
                            "a" => {
                                self.insert_element(tag, attributes.to_vec());
                                token = self.t.next();
                                continue;
                            }
                            _ => {
                                token = self.t.next();
                            }
                        },
                        Some(HtmlToken::EndTag { ref tag }) => {
                            match tag.as_str() {
                                "body" => {
                                    // </body> で AfterBody へ遷移し、BODY が開いていれば畳む。
                                    self.mode = InsertionMode::AfterBody;
                                    token = self.t.next();
                                    if !self.contain_in_stack(ElementKind::Body) {
                                        // パースの失敗。トークンを無視する
                                        continue;
                                    }
                                    self.pop_until(ElementKind::Body);
                                    continue;
                                }
                                "html" => {
                                    // </html> の簡易処理。BODY が直前に閉じていれば AfterBody。
                                    if self.pop_current_node(ElementKind::Body) {
                                        self.mode = InsertionMode::AfterBody;
                                        assert!(self.pop_current_node(ElementKind::Html));
                                    } else {
                                        token = self.t.next();
                                    }
                                    continue;
                                }
                                "p" => {
                                    // </p> 等の特定要素は、その要素が現れるまでスタックを巻き戻す。
                                    let element_kind = ElementKind::from_str(tag)
                                        .expect("failed to convert string to ElementKind");
                                    token = self.t.next();
                                    self.pop_until(element_kind);
                                    continue;
                                }
                                "h1" | "h2" => {
                                    let element_kind = ElementKind::from_str(tag)
                                        .expect("failed to convert string to ElementKind");
                                    token = self.t.next();
                                    self.pop_until(element_kind);
                                    continue;
                                }
                                "a" => {
                                    let element_kind = ElementKind::from_str(tag)
                                        .expect("failed to convert string to ElementKind");
                                    token = self.t.next();
                                    self.pop_until(element_kind);
                                    continue;
                                }
                                _ => {
                                    token = self.t.next();
                                }
                            }
                        }
                        Some(HtmlToken::Eof) | None => {
                            return self.window.clone();
                        }
                        Some(HtmlToken::Char(c)) => {
                            // テキストは現在の挿入先に連結（coalescing）される。
                            self.insert_char(c);
                            token = self.t.next();
                            continue;
                        }
                    }
                }

                // styleタグとscriptタグが開始した後の状態
                // 終了タグが出るまで、文字をテキストノードとしてDOMツリーに追加する
                InsertionMode::Text => {
                    // <style> / <script> の中身を「文字列として」追加する（終了タグまで）。
                    match token {
                        Some(HtmlToken::Eof) | None => {
                            return self.window.clone();
                        }
                        Some(HtmlToken::EndTag { ref tag }) => {
                            if tag == "style" {
                                // style終了タグ
                                self.pop_until(ElementKind::Style);
                                self.mode = self.original_insertion_mode;
                                token = self.t.next();
                                continue;
                            }
                            if tag == "script" {
                                // script終了タグ
                                self.pop_until(ElementKind::Script);
                                self.mode = self.original_insertion_mode;
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::Char(c)) => {
                            // 終了タグが出てくるまでDOMツリーに追加する
                            self.insert_char(c);
                            token = self.t.next();
                            continue;
                        }
                        _ => {}
                    }

                    self.mode = self.original_insertion_mode;
                }

                // html終了タグをあつかう
                InsertionMode::AfterBody => {
                    // 本文の後。/html で AfterAfterBody へ、文字は無視。
                    match token {
                        Some(HtmlToken::Char(_c)) => {
                            // 次のトークンが文字トークンの時。無視して次のトークンへ
                            token = self.t.next();
                            continue;
                        }
                        Some(HtmlToken::EndTag { ref tag }) => {
                            // EndTagでタグの名前がhtmlの時。AfterAfterBodyへ
                            if tag == "html" {
                                self.mode = InsertionMode::AfterAfterBody;
                                token = self.t.next();
                                continue;
                            }
                        }
                        Some(HtmlToken::Eof) | None => {
                            // それ以外の時はInBodyへ
                            return self.window.clone();
                        }
                        _ => {}
                    }

                    self.mode = InsertionMode::InBody;
                }

                // トークンが終了することを確認し、パースを終了する
                InsertionMode::AfterAfterBody => {
                    // 完全終端の最終確認モード。EoF なら終了、文字は無視。
                    match token {
                        Some(HtmlToken::Char(_c)) => {
                            // トークンが文字トークンの時、無視して次へ
                            token = self.t.next();
                            continue;
                        }
                        Some(HtmlToken::Eof) | None => {
                            // EoFまたはトークンが存在しない時、DOMツリーを返す
                            return self.window.clone();
                        }
                        _ => {}
                    }

                    // パースの失敗
                    self.mode = InsertionMode::InBody;
                }
            }
        }

        // ループ外（保険）。ここに来る場合は入力が尽きている想定。
        self.window.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn test_empty() {
        // 準備: 空文字（= 空のHTML）を入力にする
        let html = "".to_string();
        // トークナイザを作成（文字が無いので最初から EOF 相当）
        let t = HtmlTokenizer::new(html);
        // ツリービルダーで DOM を構築（省略ルールにより最低限の Document だけができる想定）
        let window = HtmlParser::new(t).construct_tree();

        // 期待: ルートは空の Document ノード
        let expected = Rc::new(RefCell::new(Node::new(NodeKind::Document)));

        // 検証: window.document() が Document 単体であること
        assert_eq!(expected, window.borrow().document());
    }

    #[test]
    fn test_body() {
        // 準備: 最小の骨組みを満たす HTML（html/head/body を明示）
        // 具体例の入力文字列:
        //   "<html><head></head><body></body></html>"
        // 期待するDOM構造（値を含む）:
        //   Document
        //   └─ Element("html")
        //      ├─ Element("head")
        //      └─ Element("body")
        let html = "<html><head></head><body></body></html>".to_string();
        // トークナイズ → ツリービルド（DOM 構築）
        let t = HtmlTokenizer::new(html);
        let window = HtmlParser::new(t).construct_tree();
        let document = window.borrow().document();
        // 期待: ルートは Document ノード（NodeKind::Document）
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Document))),
            document
        );

        // Document の最初の子は Element("html")。
        // ここで `first_child()` は `Option<Rc<RefCell<Node>>>` を返すため、
        // `expect` で Some を取り出し、`assert_eq!` では右辺も Rc<RefCell<Node>> で作っています。
        let html = document
            .borrow()
            .first_child()
            .expect("failed to get a first child of document");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "html",
                Vec::new()
            ))))),
            html
        );

        // Element("html") の最初の子は Element("head")。
        // 具体的な値: タグ名は "head"、属性は空の Vec（[]）。
        let head = html
            .borrow()
            .first_child()
            .expect("failed to get a first child of html");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "head",
                Vec::new()
            ))))),
            head
        );

        // Element("head") の次の兄弟は Element("body")。
        // 具体的な値: タグ名は "body"、属性は空の Vec（[]）。
        let body = head
            .borrow()
            .next_sibling()
            .expect("failed to get a next sibling of head");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "body",
                Vec::new()
            ))))),
            body
        );
    }

    #[test]
    fn test_text() {
        // 準備: body 内にテキスト "text" を含む HTML
        // 入力文字列:
        //   "<html><head></head><body>text</body></html>"
        // 期待するDOM:
        //   Document
        //   └─ Element("html")
        //      ├─ Element("head")
        //      └─ Element("body")
        //         └─ Text("text")
        let html = "<html><head></head><body>text</body></html>".to_string();
        // トークナイズ → ツリービルド
        let t = HtmlTokenizer::new(html);
        let window = HtmlParser::new(t).construct_tree();
        let document = window.borrow().document();
        // ルートは Document
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Document))),
            document
        );

        // Document の最初の子は Element("html")（属性は空の Vec）
        let html = document
            .borrow()
            .first_child()
            .expect("failed to get a first child of document");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "html",
                Vec::new()
            ))))),
            html
        );

        // html の最初の子は head、その次の兄弟が body（どちらも属性は空）
        let body = html
            .borrow()
            .first_child()
            .expect("failed to get a first child of document")
            .borrow()
            .next_sibling()
            .expect("failed to get a next sibling of head");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "body",
                Vec::new()
            ))))),
            body
        );

        // body の最初の子は Text("text")
        // 具体値: 文字列内容が "text" であることを検証
        let text = body
            .borrow()
            .first_child()
            .expect("failed to get a first child of document");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Text("text".to_string())))),
            text
        );
    }

    #[test]
    fn test_multiple_nodes() {
        // 準備: 入れ子構造と属性を含む HTML
        // 入力:
        //   "<html><head></head><body><p><a foo=bar>text</a></p></body></html>"
        // 期待DOM（値付き）:
        //   Document
        //   └─ Element("html")
        //      ├─ Element("head")
        //      └─ Element("body")
        //         └─ Element("p")
        //            └─ Element("a", attrs=[("foo","bar")])
        //               └─ Text("text")
        let html = "<html><head></head><body><p><a foo=bar>text</a></p></body></html>".to_string();
        let t = HtmlTokenizer::new(html);
        let window = HtmlParser::new(t).construct_tree();
        let document = window.borrow().document();

        // Document → html → head → body と辿る。
        // ここでは一気に first_child() → first_child() → next_sibling() で body を取得:
        // - document.first_child() = html
        // - html.first_child()     = head
        // - head.next_sibling()    = body
        let body = document
            .borrow()
            .first_child()
            .expect("failed to get a first child of document")
            .borrow()
            .first_child()
            .expect("failed to get a first child of document")
            .borrow()
            .next_sibling()
            .expect("failed to get a next sibling of head");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "body",
                Vec::new()
            ))))),
            body
        );

        // body の最初の子は p 要素（属性は空）
        let p = body
            .borrow()
            .first_child()
            .expect("failed to get a first child of body");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "p",
                Vec::new()
            ))))),
            p
        );

        // 期待する a の属性: foo=bar
        // Attribute::new() で空の属性を作り、add_char で name/value を1文字ずつ構築。
        //   is_name=true  で name 側（"foo"）
        //   is_name=false で value 側（"bar"）
        let mut attr = Attribute::new();
        attr.add_char('f', true);
        attr.add_char('o', true);
        attr.add_char('o', true);
        attr.add_char('b', false);
        attr.add_char('a', false);
        attr.add_char('r', false);
        // p の最初の子は a 要素で、属性に [ ("foo","bar") ] を持つ想定
        let a = p
            .borrow()
            .first_child()
            .expect("failed to get a first child of p");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Element(Element::new(
                "a",
                vec![attr]
            ))))),
            a
        );

        // a の最初の子は Text("text")
        let text = a
            .borrow()
            .first_child()
            .expect("failed to get a first child of a");
        assert_eq!(
            Rc::new(RefCell::new(Node::new(NodeKind::Text("text".to_string())))),
            text
        );
    }
}
