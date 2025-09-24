//! utils — デバッグ用ユーティリティ（DOM をインデント付きの文字列にする）
//!
//! 目的
//! - `Node`（Document/Element/Text）の木構造を、人間が読みやすいテキストに変換します。
//! - レンダリングの代わりに“DOM の概形”を確認する用途に使います。
//!
//! 出力イメージ
//! ```text
//! Document
//!   Element(ElementKind::Html)
//!     Element(ElementKind::Head)
//!     Element(ElementKind::Body)
//!       Text("hello")
//! ```
//!
//! 言語ブリッジ（TS / Python / Go）
//! - 再帰関数で「先に子、次に兄弟」を辿る前順走査の一種。
//! - 文字列連結は `String`（所有文字列）を `push_str`/`push` で伸ばしていきます。

use crate::renderer::dom::node::Node;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use core::cell::RefCell;

// ルートノード（Option<Rc<RefCell<Node>>>）から、インデント付きのツリー文字列を作る
pub fn convert_dom_to_string(root: &Option<Rc<RefCell<Node>>>) -> String {
    // 先頭に改行を入れて見やすくする（呼び出し側が println! で1行ずつ出すため）
    let mut result = String::from("\n");
    convert_dom_to_string_internal(root, 0, &mut result);
    result
}

// 内部実装: 再帰で (1) 自分を出力 → (2) 最初の子へ（深さ+1） → (3) 次の兄弟へ（同じ深さ）
fn convert_dom_to_string_internal(
    node: &Option<Rc<RefCell<Node>>>,
    depth: usize,
    result: &mut String,
) {
    match node {
        Some(n) => {
            // 深さに応じて2スペースずつインデント
            result.push_str(&"  ".repeat(depth));
            // {:?} で NodeKind のデバッグ表現（Document/Element(...)/Text("...")）を出力
            result.push_str(&format!("{:?}", n.borrow().kind()));
            result.push('\n');
            // 先に「最初の子」を深さ+1で出力（子孫をすべて出す）
            convert_dom_to_string_internal(&n.borrow().first_child(), depth + 1, result);
            // 次に「次の兄弟」を同じ深さで出力
            convert_dom_to_string_internal(&n.borrow().next_sibling(), depth, result);
        }
        None => (),
    }
}
