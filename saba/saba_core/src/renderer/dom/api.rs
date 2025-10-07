//! DOM のユーティリティ API（初心者向け）
//!
//! ここでは DOM ツリー（`Node`）をたどって、特定の要素（`ElementKind`）を探す
//! 小さなヘルパー関数を提供します。実ブラウザで言うと、非常に限定的な
//! `document.querySelector("tag")` のような処理のイメージです。
//!
//! 言語ブリッジ（TS / Python / Go）
//! - 再帰関数で「先に子、次に兄弟」をたどる DFS（深さ優先探索）をしています。
//! - 返り値 `Option<Rc<RefCell<Node>>>` は、“見つかったら Some(ノード)、なければ None”。
//! - `Rc<RefCell<Node>>` は「共有 + 内部可変」なノード参照です（TS/Python/Go の参照共有に近い）。
//!
//! 例（概念）
//! - ツリー: Document → html → head, body → body 配下に p, h1…
//! - 呼び出し: `get_target_element_node(Some(document), ElementKind::Body)`
//!   → 最初に見つかった `<body>` ノードの `Rc<RefCell<Node>>` を返します。

use crate::renderer::dom::node::Element;
use crate::renderer::dom::node::ElementKind;
use crate::renderer::dom::node::Node;
use crate::renderer::dom::node::NodeKind;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::cell::RefCell;

/// id 属性が一致する最初の要素ノードを返します（深さ優先探索）。
///
/// 引数
/// - `node`: 探索の起点となるノード（`Some(root)` を渡す想定）。
/// - `id_name`: マッチさせたい `id` の文字列。
///
/// 戻り値
/// - 見つかれば `Some(Rc<RefCell<Node>>)`、無ければ `None`。
///
/// アルゴリズム概略
/// - 現在ノードが要素なら属性を走査して `id == id_name` を判定。
/// - 見つからなければ「子 → 兄弟」の順で再帰（DFS）。
pub fn get_element_by_id(
    node: Option<Rc<RefCell<Node>>>,
    id_name: &String,
) -> Option<Rc<RefCell<Node>>> {
    match node {
        Some(n) => {
            // 1) 現在ノードが要素なら、属性から id 一致を探す
            if let NodeKind::Element(e) = n.borrow().kind() {
                for attr in &e.attributes() {
                    if attr.name() == "id" && attr.value() == *id_name {
                        return Some(n.clone());
                    }
                }
            }
            // 2) 見つからなければ、子 → 兄弟の順で検索を進める
            let result1 = get_element_by_id(n.borrow().first_child(), id_name);
            let result2 = get_element_by_id(n.borrow().next_sibling(), id_name);
            // 子側で見つからなかったら兄弟側の結果を返す
            if result1.is_none() {
                return result2;
            }
            result1
        }
        // 3) 末端（None）に達したら探索終了
        None => None,
    }
}

/// ツリーを深さ優先で探索し、最初に見つかった `element_kind` の要素ノードを返す
///
/// - 探索順: 「自分 → 子（first_child）→ 兄弟（next_sibling）」の順で DFS。
/// - 一致条件: `NodeKind::Element(ElementKind == 指定)` のノードかどうか。
/// - 返り値: 見つかれば Some(ノード参照)、なければ None。
///
/// 注意
/// - ここでは比較の簡易化のため、`Element::new(kind_as_str, Vec::new())` を使って
///   `NodeKind::Element(...)` と等価比較しています（属性は空で OK）。
/// - 最初に見つかった 1 件だけ返す仕様です（複数取得は別の関数でベクタに集めるのが良い）。
pub fn get_target_element_node(
    node: Option<Rc<RefCell<Node>>>,
    element_kind: ElementKind,
) -> Option<Rc<RefCell<Node>>> {
    match node {
        Some(n) => {
            // 1) 現在ノードが条件に一致するかをチェック
            if n.borrow().kind()
                == NodeKind::Element(Element::new(&element_kind.to_string(), Vec::new()))
            {
                return Some(n.clone()); // 見つかったので即返す
            }
            // 2) 異なる場合は、子 → 兄弟の順で再帰的に探索する
            let result1 = get_target_element_node(n.borrow().first_child(), element_kind); // 子へ降りる
            let result2 = get_target_element_node(n.borrow().next_sibling(), element_kind); // 兄弟へ進む
            if result1.is_none() && result2.is_none() {
                return None;
            }
            if result1.is_none() {
                return result2;
            }
            result1
        }
        None => None,
    }
}

/// DOM から <style> タグの“テキスト中身”だけを取り出すヘルパー
///
/// 仕様（このプロジェクト内での前提）
/// - スタイルは `<style>ここに CSS 文字列</style>` のように、テキストノード1つで入っている想定。
/// - `<style>` が無い、または <style> の直下がテキストでない場合は空文字を返します。
///
/// 例
/// - 入力 DOM: <head><style>p { color: red; }</style></head> → "p { color: red; }"
pub fn get_style_content(root: Rc<RefCell<Node>>) -> String {
    // 1) ツリーから最初の <style> 要素を探す
    let style_node = match get_target_element_node(Some(root), ElementKind::Style) {
        Some(node) => node,
        None => return "".to_string(), // スタイルが無ければ空文字
    };
    // 2) <style> の直下の最初の子がテキストノードであることを期待
    let text_node = match style_node.borrow().first_child() {
        Some(node) => node,
        None => return "".to_string(),
    };
    // 3) テキストであればそのまま中身を返す。その他（要素など）の場合は空文字
    let content = match &text_node.borrow().kind() {
        NodeKind::Text(ref s) => s.clone(),
        _ => "".to_string(),
    };
    content
}

/// DOM から `<script>` タグの“テキスト中身”だけを取り出すヘルパー
///
/// 仕様（このプロジェクト内での前提）
/// - `<script>` はテキストノード1つでソースを保持している想定。
/// - `<script>` が無い、または直下がテキストでない場合は空文字を返します。
///
/// 例
/// - 入力 DOM: `<body><script>var a=1;</script></body>` → "var a=1;"
pub fn get_js_content(root: Rc<RefCell<Node>>) -> String {
    // 1) ツリーから最初の <script> 要素を探す
    let js_node = match get_target_element_node(Some(root), ElementKind::Script) {
        Some(node) => node,
        None => return "".to_string(), // スクリプトが無ければ空文字
    };
    // 2) <script> の直下の最初の子がテキストノードであることを期待
    let text_node = match js_node.borrow().first_child() {
        Some(node) => node,
        None => return "".to_string(),
    };
    // 3) テキストであればそのまま中身を返す。その他（要素など）の場合は空文字
    let content = match &text_node.borrow().kind() {
        NodeKind::Text(ref s) => s.clone(),
        _ => "".to_string(),
    };
    content
}
