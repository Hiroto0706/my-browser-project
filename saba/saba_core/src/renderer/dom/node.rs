//! DOM ノード（Window/Document/Element/Text）の最小実装（初心者向け）
//!
//! これは“ブラウザの内部表現（DOM: Document Object Model）”のごく小さなモデルです。
//! - 実際のブラウザでは、HTML をパースすると「ノードの木構造（DOM ツリー）」が作られます。
//! - このファイルでは、その最小構成として `Window`（最上位のグローバル）→`Document`→
//!   `Element`/`Text` という階層を `Node` で表現しています。
//! - `Element` はタグ種別（`ElementKind`）と属性（`attributes: Vec<Attribute>`）を持ち、
//!   `get_attribute("href")` のように属性値を取り出せます（学習用の簡易実装）。
//! - 兄弟/親子リンクを持つ「双方向の木」を、Rust の `Rc<RefCell<...>>` と `Weak` を使って実現します。
//!
//! 言語ブリッジ（TS / Python / Go）
//! - `Rc<T>` は参照カウント付きの“共有所有権”。TS/Python/Go では普通の参照共有に近い感覚。
//! - `RefCell<T>` は“内部可変”。借用ルールをランタイムチェックに委ね、中身を変更可能にします。
//! - `Weak<T>` は循環参照を避けるための「弱い参照」。親や前後のリンクに使い、`Rc` サイクルを防ぎます。
//! - DOM 用語の対応: Node ≈ DOM Node, Element ≈ HTMLElement, Text ≈ TextNode, Window/Document はブラウザのグローバル/文書。
//!
//! 例（簡単なツリー: <p>Hello</p>）
//! ```ignore
//! let win = Window::new();                 // Window → Document（空）
//! let doc = win.document();                // Rc<RefCell<Node>>
//! // ここでパーサが作る想定: Document の下に Element(p) と Text("Hello") をぶら下げる …
//! // 本ファイルは木の型・リンクを提供し、パース・レンダリング層がこの構造を操作します。
//! ```
//! 属性の取得例（<a href="/next">link</a>）
//! ```ignore
//! if let Some(Element(e)) = &doc.borrow().first_child().unwrap().borrow().kind() { /* 概念図 */ }
//! // 実際にはパーサが Element を作り、以下のように参照できます:
//! let a: Element = /* <a href=...> を指す要素 */;
//! assert_eq!(a.get_attribute("href"), Some("/next".to_string()));
//! ```
//! ブラウザ挙動での役割
//! - パース後: DOM ツリー（この Node 構造体群）が完成。
//! - レイアウト/描画: この DOM ツリーをもとにフレームツリー/レイアウトツリーを作成し描画（ここでは未実装）。
//! - イベント/スクリプト: Window/Document を起点にイベント配信や JS 実行（ここでは最小限）。

use crate::renderer::html::attribute::Attribute;
use alloc::format;
use alloc::rc::Rc;
use alloc::rc::Weak;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt::Display;
use core::fmt::Formatter;
use core::str::FromStr;

#[derive(Debug, Clone)]
pub struct Window {
    document: Rc<RefCell<Node>>,
}

impl Window {
    // ブラウザの `window` に相当。作成時に空の `Document` ノードを用意します。
    // 注意: `Rc`/`Weak` で自己参照関係を張るため、生成ステップが少し回りくどいです。
    pub fn new() -> Self {
        let window = Self {
            document: Rc::new(RefCell::new(Node::new(NodeKind::Document))),
        };

        window
            .document
            .borrow_mut()
            .set_window(Rc::downgrade(&Rc::new(RefCell::new(window.clone()))));

        window
    }

    // 実ブラウザ API の `window.document` に相当。
    pub fn document(&self) -> Rc<RefCell<Node>> {
        self.document.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    window: Weak<RefCell<Window>>,
    parent: Weak<RefCell<Node>>,
    first_child: Option<Rc<RefCell<Node>>>,
    last_child: Weak<RefCell<Node>>,
    previous_sibling: Weak<RefCell<Node>>,
    next_sibling: Option<Rc<RefCell<Node>>>,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Node {
    // ノードを新規に作成。リンク（親/兄弟/子）は空で、種別だけを持ちます。
    pub fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            window: Weak::new(),
            parent: Weak::new(),
            first_child: None,
            last_child: Weak::new(),
            previous_sibling: Weak::new(),
            next_sibling: None,
        }
    }

    // Window 弱参照をセット。`Window::new` 時に Document 側へ逆リンクとして張ります。
    pub fn set_window(&mut self, window: Weak<RefCell<Window>>) {
        self.window = window;
    }

    // 親ノードを Weak でセット。循環参照（リーク）を避けるため Rc ではなく Weak。
    pub fn set_parent(&mut self, parent: Weak<RefCell<Node>>) {
        self.parent = parent;
    }

    // 親ノード取得（Weak）。必要に応じて `upgrade()` して Rc に変換します。
    pub fn parent(&self) -> Weak<RefCell<Node>> {
        self.parent.clone()
    }

    // 最初の子への Rc をセット/取得。
    pub fn set_first_child(&mut self, first_child: Option<Rc<RefCell<Node>>>) {
        self.first_child = first_child;
    }

    pub fn first_child(&self) -> Option<Rc<RefCell<Node>>> {
        self.first_child.as_ref().cloned()
    }

    // 最後の子への Weak をセット/取得。Rc にしたいときは `upgrade()` を使います。
    pub fn set_last_child(&mut self, last_child: Weak<RefCell<Node>>) {
        self.last_child = last_child;
    }

    pub fn last_child(&self) -> Weak<RefCell<Node>> {
        self.last_child.clone()
    }

    // 兄（直前の兄弟）/弟（直後の兄弟）のリンクをセット/取得。
    pub fn set_previous_sibling(&mut self, previous_sibling: Weak<RefCell<Node>>) {
        self.previous_sibling = previous_sibling;
    }

    pub fn previous_sibling(&self) -> Weak<RefCell<Node>> {
        self.previous_sibling.clone()
    }

    pub fn set_next_sibling(&mut self, next_sibling: Option<Rc<RefCell<Node>>>) {
        self.next_sibling = next_sibling;
    }

    pub fn next_sibling(&self) -> Option<Rc<RefCell<Node>>> {
        self.next_sibling.as_ref().cloned()
    }

    // ノード種別（Document / Element / Text）を取得。
    pub fn kind(&self) -> NodeKind {
        self.kind.clone()
    }

    // Element ノードなら要素情報を返す（タグ名や属性群）。Text/Document なら None。
    pub fn get_element(&self) -> Option<Element> {
        match self.kind {
            NodeKind::Document | NodeKind::Text(_) => None,
            NodeKind::Element(ref e) => Some(e.clone()),
        }
    }

    // Element ノードなら要素の種類（p/h1/body など）を返す。Text/Document なら None。
    pub fn element_kind(&self) -> Option<ElementKind> {
        match self.kind {
            NodeKind::Document | NodeKind::Text(_) => None,
            NodeKind::Element(ref e) => Some(e.kind()),
        }
    }
}

#[derive(Debug, Clone, Eq)]
pub enum NodeKind {
    /// https://dom.spec.whatwg.org/#interface-document
    Document,
    /// https://dom.spec.whatwg.org/#interface-element
    Element(Element),
    /// https://dom.spec.whatwg.org/#interface-text
    Text(String),
}

impl PartialEq for NodeKind {
    fn eq(&self, other: &Self) -> bool {
        match &self {
            NodeKind::Document => matches!(other, NodeKind::Document),
            NodeKind::Element(e1) => match &other {
                NodeKind::Element(e2) => e1.kind == e2.kind,
                _ => false,
            },
            NodeKind::Text(_) => matches!(other, NodeKind::Text(_)),
        }
    }
}

/// https://dom.spec.whatwg.org/#interface-element
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    kind: ElementKind,
    attributes: Vec<Attribute>,
}

impl Element {
    // 文字列のタグ名（"p", "h1" など）と属性リストから Element を生成。
    // 実ブラウザでは不正なタグ名も許容されますが、ここでは列挙型に無いとエラーにします。
    pub fn new(element_name: &str, attributes: Vec<Attribute>) -> Self {
        Self {
            kind: ElementKind::from_str(element_name)
                .expect("failed to convert string to ElementKind"),
            attributes,
        }
    }

    // 要素の種類（ElementKind）を返す（タグ名に相当）。
    pub fn kind(&self) -> ElementKind {
        self.kind
    }

    /// この要素が持つ全属性を返す（順序は保持）
    ///
    /// メモ
    /// - ここでは `Vec<Attribute>` をそのまま複製して返します（学習用の単純実装）。
    /// - 実ブラウザでは属性マップの正規化や大小文字の扱い、名前空間等が絡みます。
    pub fn attributes(&self) -> Vec<Attribute> {
        self.attributes.clone()
    }

    /// 属性 `name` の値を返す（存在しなければ `None`）
    ///
    /// 使い方
    /// - `<a href="/next">` に対して `get_attribute("href")` → `Some("/next".to_string())`
    /// - 見つからない場合は `None`
    ///
    /// 注意（簡易実装）
    /// - 名前比較は完全一致（大文字小文字の正規化はしません）。HTML 的には小文字想定です。
    /// - 重複属性は想定しません。最初に見つかった 1 件を返します。
    pub fn get_attribute(&self, name: &str) -> Option<String> {
        for attr in &self.attributes {
            if attr.name() == name {
                return Some(attr.value());
            }
        }
        None
    }

    // 要素がデフォルトでブロック要素かインライン要素か決める
    pub fn is_block_element(&self) -> bool {
        match self.kind {
            ElementKind::Body | ElementKind::H1 | ElementKind::H2 | ElementKind::P => true,
            _ => false,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// https://dom.spec.whatwg.org/#interface-element
pub enum ElementKind {
    /// https://html.spec.whatwg.org/multipage/semantics.html#the-html-element
    Html,
    /// https://html.spec.whatwg.org/multipage/semantics.html#the-head-element
    Head,
    /// https://html.spec.whatwg.org/multipage/semantics.html#the-style-element
    Style,
    /// https://html.spec.whatwg.org/multipage/scripting.html#the-script-element
    Script,
    /// https://html.spec.whatwg.org/multipage/sections.html#the-body-element
    Body,
    /// https://html.spec.whatwg.org/multipage/grouping-content.html#the-p-element
    P,
    /// https://html.spec.whatwg.org/multipage/sections.html#the-h1,-h2,-h3,-h4,-h5,-and-h6-elements
    H1,
    H2,
    /// https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-a-element
    A,
}

// ElementKind ↔ タグ名（文字列）の相互変換ヘルパー
//
// - Display 実装: ElementKind → "html" / "body" などのタグ名へ
//   例: format!("{}", ElementKind::P) == "p"
// - FromStr 実装: "html" / "body" などのタグ名 → ElementKind へ
//   例: ElementKind::from_str("h1") == Ok(ElementKind::H1)
//
// これにより、DOM API 側で「列挙型と文字列」を簡単に相互運用できます。
impl Display for ElementKind {
    fn fmt(&self, f: &mut Formatter) -> core::fmt::Result {
        // 列挙子ごとに対応するタグ名（小文字）を返す
        let s = match self {
            ElementKind::Html => "html",
            ElementKind::Head => "head",
            ElementKind::Style => "style",
            ElementKind::Script => "script",
            ElementKind::Body => "body",
            ElementKind::H1 => "h1",
            ElementKind::H2 => "h2",
            ElementKind::P => "p",
            ElementKind::A => "a",
        };
        write!(f, "{}", s) // 実体は単純な文字列の書き出し
    }
}

impl FromStr for ElementKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 小文字のタグ名文字列から ElementKind を得る
        // 注意: ここでは対応している要素のみサポート（未実装は Err を返す）
        match s {
            "html" => Ok(ElementKind::Html),
            "head" => Ok(ElementKind::Head),
            "style" => Ok(ElementKind::Style),
            "script" => Ok(ElementKind::Script),
            "body" => Ok(ElementKind::Body),
            "p" => Ok(ElementKind::P),
            "h1" => Ok(ElementKind::H1),
            "h2" => Ok(ElementKind::H2),
            "a" => Ok(ElementKind::A),
            _ => Err(format!("unimplemented element name {:?}", s)), // 対応外タグ（学習用の簡易実装）
        }
    }
}
