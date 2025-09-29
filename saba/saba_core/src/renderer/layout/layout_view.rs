//! レイアウトビュー（LayoutView）— DOM/CSSOM からレイアウトツリーを作り、サイズと位置を決める
//!
//! 役割
//! - DOM（描画対象の要素）をレイアウトオブジェクトへ変換し、CSS（CSSOM）を基に
//!   “サイズ（compute_size）”と“位置（compute_position）”を再帰的に計算します。
//! - 実ブラウザの「レイアウトステージ」の超簡易版です。display:none の除外、
//!   block/inline とテキストの基本的な並びを扱います。
//!
//! 全体フロー
//! 1) <body> の直下から、描画対象のみのレイアウトツリーを構築（build_layout_tree）
//! 2) 各ノードのサイズを自前ルールで算出（calculate_node_size → compute_size）
//! 3) 各ノードの位置（左上座標）を決定（calculate_node_position → compute_position）
//!
//! ヒットテスト（クリック判定）
//! - 画面座標（コンテンツ左上を原点）から、どのレイアウトノードの範囲にあるかを逆引きできます。
//! - `find_node_by_position((x,y))` がエントリで、内部では DFS（子優先）で矩形当たり判定を行います。
//! - UI 側ではツールバーやウィンドウ余白を引いた“コンテンツ座標”に変換してから呼びます。
//!
//! 制約（学習用の簡易化）
//! - 行折り返しや margin/padding/border、line-height 等の厳密処理は省略
//! - display は block/inline/none のみ
//! - テキストは等幅フォントで粗い見積り
use crate::constants::CONTENT_AREA_WIDTH;
use crate::display_item::DisplayItem;
use crate::renderer::css::cssom::StyleSheet;
use crate::renderer::dom::api::get_target_element_node;
use crate::renderer::dom::node::ElementKind;
use crate::renderer::dom::node::Node;
use crate::renderer::layout::layout_object::create_layout_object;
use crate::renderer::layout::layout_object::LayoutObject;
use crate::renderer::layout::layout_object::LayoutObjectKind;
use crate::renderer::layout::layout_object::LayoutPoint;
use crate::renderer::layout::layout_object::LayoutSize;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

// DOM ツリー → レイアウトツリー（描画対象のみ）を組み立てる
//
// ざっくりの流れ（実ブラウザの“レイアウトツリービルド”に相当）
// 1) 現在の DOM ノードから `create_layout_object` を試みる
//    - CSS が `display: none` なら LayoutObject は「作らない」→ 兄弟へスキップ
// 2) 作れたら、それを親として子/兄弟について同様に再帰処理
// 3) 子にも兄弟にも `display: none` があり得るので、作れるまで“次の兄弟へ”進み続ける
//
// 重要ポイント
// - レイアウトツリーは「描画される要素だけ」を持つ（`display:none` はツリーから除外）
// - “DOM の形”と“レイアウトツリーの形”は一致しないことがある（不可視要素の除去など）
fn build_layout_tree(
    node: &Option<Rc<RefCell<Node>>>,
    parent_obj: &Option<Rc<RefCell<LayoutObject>>>,
    cssom: &StyleSheet,
) -> Option<Rc<RefCell<LayoutObject>>> {
    // 1) まず現在の DOM ノードから LayoutObject を作成してみる
    //    - `create_layout_object` は CSSOM を参照して `display:none` なら None を返す
    // `create_layout_object`関数によって、ノードとなるLayoutObjectの作成を試みる。
    // CSSによって"display:none"が指定されていた場合、ノードは作成されない
    let mut target_node = node.clone();
    let mut layout_object = create_layout_object(node, parent_obj, cssom);
    // 2) 作れなかった（= display:none 等）場合、兄弟へ進み“作れるまで”繰り返し
    // もしノードが作成されなかった場合、DOMノードの兄弟ノードを使用してLayoutObjectの
    // 作成を試みる。LayoutObjectが作成されるまで、兄弟ノードを辿り続ける
    while layout_object.is_none() {
        if let Some(n) = target_node {
            target_node = n.borrow().next_sibling().clone();
            layout_object = create_layout_object(&target_node, parent_obj, cssom);
        } else {
            // 兄弟ノードが無ければ、これ以上作る要素は無い → ここまでで終了
            // もし兄弟ノードがない場合、処理するべきDOMツリーは終了したので、今まで
            // 作成したレイアウトツリーを返す
            return layout_object;
        }
    }

    if let Some(n) = target_node {
        let original_first_child = n.borrow().first_child();
        let original_next_sibling = n.borrow().next_sibling();
        // 3) 子と兄弟について再帰的にレイアウトツリーを作る
        //    - 子の親は“今作った LayoutObject”
        //    - 兄弟は“同じ親”を共有するので、ここでは親 None → 上の呼び出し側が適切に接続する
        // もし子ノードに"display:node"が指定されていた場合、LayoutObjectは作成され
        // ないため、子ノードの兄弟ノードを使用してLayoutObjectの作成を試みる。
        // LayoutObjectが作成されるか、辿るべき兄弟ノードがなくなるまで処理を繰り返す
        let mut first_child = build_layout_tree(&original_first_child, &layout_object, cssom);
        let mut next_sibling = build_layout_tree(&original_next_sibling, &None, cssom);

        // 4) 子が `display:none` で作られなかった場合 → 子の“兄弟”を順に試す
        //    LayoutObject が作れるまで、または辿る兄弟が尽きるまで進める
        if first_child.is_none() && original_first_child.is_some() {
            let mut original_dom_node = original_first_child
                .expect("first child should exist")
                .borrow()
                .next_sibling();

            loop {
                first_child = build_layout_tree(&original_dom_node, &layout_object, cssom);

                if first_child.is_none() && original_dom_node.is_some() {
                    original_dom_node = original_dom_node
                        .expect("next sibling should exist")
                        .borrow()
                        .next_sibling();
                    continue;
                }

                break;
            }
        }

        // 5) 兄弟が `display:none` で作られなかった場合 → 兄弟の“次の兄弟”を順に試す
        //    LayoutObject が作れるまで、または辿る兄弟が尽きるまで進める
        // もし兄弟ノードに"display:node"が指定されていた場合、LayoutObjectは作成され
        // ないため、兄弟ノードの兄弟ノードを使用してLayoutObjectの作成を試みる。
        // LayoutObjectが作成されるか、辿るべき兄弟ノードがなくなるまで処理を繰り返す
        if next_sibling.is_none() && n.borrow().next_sibling().is_some() {
            let mut original_dom_node = original_next_sibling
                .expect("first child should exist")
                .borrow()
                .next_sibling();

            loop {
                next_sibling = build_layout_tree(&original_dom_node, &None, cssom);

                if next_sibling.is_none() && original_dom_node.is_some() {
                    original_dom_node = original_dom_node
                        .expect("next sibling should exist")
                        .borrow()
                        .next_sibling();
                    continue;
                }

                break;
            }
        }

        // 6) ここまでで“現在ノードの LayoutObject”は存在している前提
        let obj = match layout_object {
            Some(ref obj) => obj,
            None => panic!("render object should exist here"),
        };
        // 7) レイアウトツリーの接続を行う（子/兄弟）
        obj.borrow_mut().set_first_child(first_child);
        obj.borrow_mut().set_next_sibling(next_sibling);
    }

    layout_object
}

#[derive(Debug, Clone)]
pub struct LayoutView {
    root: Option<Rc<RefCell<LayoutObject>>>,
}

impl LayoutView {
    /// DOM と CSSOM からレイアウトツリーを構築し、サイズ・位置を確定する
    ///
    /// 手順
    /// - <body> のノードを起点に、`display:none` を除外したレイアウトツリーを作る（build_layout_tree）
    /// - その後、update_layout でサイズ → 位置の順に確定
    pub fn new(root: Rc<RefCell<Node>>, cssom: &StyleSheet) -> Self {
        // レイアウトツリーは描画される要素だけを持つツリーなので、<body>タグを取得し、その子要素以下をレイアウトツリーのノードに変換する。
        let body_root = get_target_element_node(Some(root), ElementKind::Body);

        let mut tree = Self {
            root: build_layout_tree(&body_root, &None, cssom),
        };

        tree.update_layout();

        tree
    }

    /// 画面上の座標 `(x,y)` にあるレイアウトノードを返す（子優先のヒットテスト）
    ///
    /// 入力
    /// - `position`: コンテンツ領域基準の座標（左上が 0,0）。UI 側でツールバー分を除去してください。
    ///
    /// 返り値
    /// - `Some(Rc<LayoutObject>)` … クリック位置を包含する最も内側のノード（子優先で探索）。
    /// - `None` … どのノードにも当たらない場合。
    ///
    /// 実装メモ
    /// - 内部の `find_node_by_position_internal` は子 → 兄弟 → 自身の順に判定することで、
    ///   より内側のノードを優先して返します（一般的なブラウザのヒットテストに近い挙動）。
    pub fn find_node_by_position(&self, position: (i64, i64)) -> Option<Rc<RefCell<LayoutObject>>> {
        Self::find_node_by_position_internal(&self.root(), position)
    }

    /// DFS でレイアウトツリーをたどり、矩形当たり判定でノードを見つける内部関数
    ///
    /// アルゴリズム（子優先）
    /// 1) まず子を調べる（より細かい要素を優先）
    /// 2) 次に兄弟を調べる（同じ階層の別要素）
    /// 3) それでも無ければ自分自身の矩形にヒットするか判定
    ///
    /// 備考
    /// - ノードの矩形は `point(): LayoutPoint`（左上）と `size(): LayoutSize`（幅・高さ）で表現。
    /// - 座標系はこの `LayoutView` で計算したコンテンツ座標と一致します。
    fn find_node_by_position_internal(
        node: &Option<Rc<RefCell<LayoutObject>>>,
        position: (i64, i64),
    ) -> Option<Rc<RefCell<LayoutObject>>> {
        match node {
            Some(n) => {
                let first_child = n.borrow().first_child();
                let result1 = Self::find_node_by_position_internal(&first_child, position);
                if result1.is_some() {
                    return result1;
                }

                let next_sibling = n.borrow().next_sibling();
                let result2 = Self::find_node_by_position_internal(&next_sibling, position);
                if result2.is_some() {
                    return result2;
                }

                // 最後に、自分自身の外接矩形に当たっているかを判定
                if n.borrow().point().x() <= position.0
                    && position.0 <= (n.borrow().point().x() + n.borrow().size().width())
                    && n.borrow().point().y() <= position.1
                    && position.1 <= (n.borrow().point().y() + n.borrow().size().height())
                {
                    return Some(n.clone());
                }
                None
            }
            None => None,
        }
    }

    // レイアウトツリーのノードの位置を再起的に計算する関数
    // 第1引数が計算するターゲットとなるノード、第2引数が親ノードの位置、第3引数は自分より前の兄弟ノードの種類、第4引数は自分より前の兄弟ノードの位置、第5引数は自分より前の兄弟ノードのサイズ
    //
    // 概要
    // - Block は縦方向（Y）へ、Inline は横方向（X）へ配置を進める簡易ルール。
    // - 子→兄弟の順で深さ優先に進み、各ノードは self.compute_position で座標を確定します。
    fn calculate_node_position(
        node: &Option<Rc<RefCell<LayoutObject>>>,
        parent_point: LayoutPoint,
        previous_sibling_kind: LayoutObjectKind,
        previous_sibling_point: Option<LayoutPoint>,
        previous_sibling_size: Option<LayoutSize>,
    ) {
        if let Some(n) = node {
            n.borrow_mut().compute_position(
                parent_point,
                previous_sibling_kind,
                previous_sibling_point,
                previous_sibling_size,
            );

            // ノード（node）の子ノードの位置を計算をする
            let first_child = n.borrow().first_child();
            Self::calculate_node_position(
                &first_child,
                n.borrow().point(),
                LayoutObjectKind::Block,
                None,
                None,
            );

            // ノード（node）の兄弟ノードの位置を計算する
            let next_sibling = n.borrow().next_sibling();
            Self::calculate_node_position(
                &next_sibling,
                parent_point,
                n.borrow().kind(),
                Some(n.borrow().point()),
                Some(n.borrow().size()),
            );
        }
    }

    // レイアウトツリーの各ノードのサイズを再起的に計算する関数
    // 第一引数がターゲットとなるノード、第二引数は親ノードのサイズ
    //
    // 概要
    // - Block の幅は“先に”親幅で確定し、高さは子のサイズ確定後に集計。
    // - Inline/Text は子（テキスト）のサイズに依存するため、子の計算後に自分を計算。
    fn calculate_node_size(node: &Option<Rc<RefCell<LayoutObject>>>, parent_size: LayoutSize) {
        if let Some(n) = node {
            // ノードがブロック要素の場合、子ノードのレイアウトを計算する前に横幅を決める
            if n.borrow().kind() == LayoutObjectKind::Block {
                n.borrow_mut().compute_size(parent_size);
            }

            let first_child = n.borrow().first_child();
            Self::calculate_node_size(&first_child, n.borrow().size());

            let next_sibling = n.borrow().next_sibling();
            Self::calculate_node_size(&next_sibling, parent_size);

            // 子ノードのサイズが決まった後にサイズを計算する。
            // ブロック要素のとき、高さは子ノードの高さに依存する
            // インライン要素のとき、高さも横幅も子ノードに依存する
            n.borrow_mut().compute_size(parent_size);
        }
    }

    /// レイアウトの再計算（サイズ→位置の順）
    ///
    /// - まずコンテンツ領域幅（CONTENT_AREA_WIDTH）を親幅に見立ててサイズ計算
    /// - 次に (0,0) を起点に座標を割り当てていきます
    fn update_layout(&mut self) {
        Self::calculate_node_size(&self.root, LayoutSize::new(CONTENT_AREA_WIDTH, 0));

        Self::calculate_node_position(
            &self.root,
            LayoutPoint::new(0, 0),
            LayoutObjectKind::Block,
            None,
            None,
        );
    }

    // レイアウトツリーを前順（自分→子→兄弟）にたどり、描画命令（DisplayItem）を集める
    //
    // 役割
    // - 各 LayoutObject に対し `paint()` を呼び、矩形塗り・文字描画などの DisplayItem を収集します。
    // - その後、子→兄弟の順に再帰して、最終的に“描画命令のリスト”を返せるようにします。
    //
    // 具体例
    // - <body><p>text</p></body> の場合:
    //   1) body.paint() → 背景などの DisplayItem を push
    //   2) p.paint()    → 段落の背景や枠などを push
    //   3) text.paint() → 文字列 "text" を描く命令を push
    //   4) p の兄弟が無ければ body の兄弟へ、無ければ終了
    fn paint_node(node: &Option<Rc<RefCell<LayoutObject>>>, display_items: &mut Vec<DisplayItem>) {
        match node {
            Some(n) => {
                // 1) 自分自身の描画命令を収集
                display_items.extend(n.borrow_mut().paint());

                // 2) 子を先に描画（前順）
                let first_child = n.borrow().first_child();
                Self::paint_node(&first_child, display_items);

                // 3) 兄弟を描画
                let next_sibling = n.borrow().next_sibling();
                Self::paint_node(&next_sibling, display_items);
            }
            None => (),
        }
    }

    /// レイアウトツリー全体を描画命令列（DisplayItem の配列）へ変換する
    ///
    /// 概要
    /// - ルートから前順にたどり、各ノードの `paint()` が返す DisplayItem を順に集めます。
    /// - 返り値のベクタは、後段のラスタライズ/合成ステージに渡して画面に反映させます。
    ///
    /// 例
    /// - <p>Hi</p> → [Rect(.. 背景 ..), Text(.. "Hi" ..)] のような命令が並ぶ想定。
    pub fn paint(&self) -> Vec<DisplayItem> {
        let mut display_items = Vec::new();

        Self::paint_node(&self.root, &mut display_items);

        display_items
    }

    /// レイアウトツリーのルート（描画される最上位の LayoutObject）を返す
    pub fn root(&self) -> Option<Rc<RefCell<LayoutObject>>> {
        self.root.clone()
    }
}

#[cfg(test)]
mod tests {
    // この tests モジュールでは、レイアウトビューを構築する“最小の足場”を用意します。
    use super::*;
    use crate::alloc::string::ToString;
    use crate::renderer::css::cssom::CssParser;
    use crate::renderer::css::token::CssTokenizer;
    use crate::renderer::dom::api::get_style_content;
    use crate::renderer::dom::node::Element;
    use crate::renderer::dom::node::NodeKind;
    use crate::renderer::html::parser::HtmlParser;
    use crate::renderer::html::token::HtmlTokenizer;
    use alloc::string::String;
    use alloc::vec::Vec;

    // テスト用のレイアウトビューを作るユーティリティ
    //
    // 手順
    // 1) HTML 文字列 → HtmlTokenizer → HtmlParser で DOM を構築
    // 2) DOM から <style> の中身だけを取り出し（get_style_content）、CSS をトークナイズ/パースして CSSOM を構築
    // 3) DOM + CSSOM から LayoutView を生成（レイアウトツリーを作り、サイズ・位置を確定）
    fn create_layout_view(html: String) -> LayoutView {
        // 1) HTML → DOM
        let t = HtmlTokenizer::new(html);
        let window = HtmlParser::new(t).construct_tree();
        let dom = window.borrow().document();

        // 2) DOM 内の <style> から CSS 文字列を抽出→ CSSOM へ
        let style = get_style_content(dom.clone());
        let css_tokenizer = CssTokenizer::new(style);
        let cssom = CssParser::new(css_tokenizer).parse_stylesheet();

        // 3) DOM + CSSOM → LayoutView（以降のテストはこの戻り値を使って検証していく）
        LayoutView::new(dom, &cssom)
    }

    #[test]
    fn test_empty() {
        // 入力: 空HTML → <body> も無いので描画対象が無い
        let layout_view = create_layout_view("".to_string());
        // 期待: レイアウトツリーのルートは None（何も描画しない）
        assert_eq!(None, layout_view.root());
    }

    #[test]
    fn test_body() {
        // 入力: <html><head></head><body></body></html>
        // 期待: ルートは <body> の LayoutObject（Block）
        let html = "<html><head></head><body></body></html>".to_string();
        let layout_view = create_layout_view(html);

        let root = layout_view.root();
        assert!(root.is_some());
        // <body> は既定で display:block → Block になる
        assert_eq!(
            LayoutObjectKind::Block,
            root.clone().expect("root should exist").borrow().kind()
        );
        // 参照している DOM ノードは <body>
        assert_eq!(
            NodeKind::Element(Element::new("body", Vec::new())),
            root.clone()
                .expect("root should exist")
                .borrow()
                .node_kind()
        );
    }

    #[test]
    fn test_text() {
        // 入力: <html><head></head><body>text</body></html>
        // 期待: ルートは <body>（Block）で、その最初の子がテキストノード "text"
        let html = "<html><head></head><body>text</body></html>".to_string();
        let layout_view = create_layout_view(html);

        let root = layout_view.root();
        assert!(root.is_some());
        // <body> は Block
        assert_eq!(
            LayoutObjectKind::Block,
            root.clone().expect("root should exist").borrow().kind()
        );
        // DOM 参照は <body>
        assert_eq!(
            NodeKind::Element(Element::new("body", Vec::new())),
            root.clone()
                .expect("root should exist")
                .borrow()
                .node_kind()
        );

        // 子はテキストノード（LayoutObjectKind::Text）で、内容は "text"
        let text = root.expect("root should exist").borrow().first_child();
        assert!(text.is_some());
        assert_eq!(
            LayoutObjectKind::Text,
            text.clone()
                .expect("text node should exist")
                .borrow()
                .kind()
        );
        assert_eq!(
            NodeKind::Text("text".to_string()),
            text.clone()
                .expect("text node should exist")
                .borrow()
                .node_kind()
        );
    }

    #[test]
    fn test_display_none() {
        // 入力: body に display:none を当てる
        // 期待: <body> 自体がレイアウトツリーから除外されるため、ルートは None
        let html = "<html><head><style>body{display:none;}</style></head><body>text</body></html>"
            .to_string();
        let layout_view = create_layout_view(html);

        assert_eq!(None, layout_view.root());
    }

    #[test]
    fn test_hidden_class() {
        // 入力: .hidden { display:none } を定義し、隠す対象を混ぜる
        // - <a class="hidden">link1</a> → インライン要素だが display:none で除外
        // - <p></p>                         → 表示対象（Block）
        // - <p class="hidden"><a>link2</a></p> → <p> ごと非表示（中の <a> も消える）
        // 期待: レイアウトツリーの <body> の最初の子は “空の <p>” で、兄弟は存在しない
        let html = r#"<html>
<head>
<style>
  .hidden {
    display: none;
  }
</style>
</head>
<body>
  <a class="hidden">link1</a>
  <p></p>
  <p class="hidden"><a>link2</a></p>
</body>
</html>"#
            .to_string();
        let layout_view = create_layout_view(html);

        let root = layout_view.root();
        assert!(root.is_some());
        assert_eq!(
            LayoutObjectKind::Block,
            root.clone().expect("root should exist").borrow().kind()
        );
        assert_eq!(
            NodeKind::Element(Element::new("body", Vec::new())),
            root.clone()
                .expect("root should exist")
                .borrow()
                .node_kind()
        );

        // body の最初の子は表示対象の <p>（Block）
        let p = root.expect("root should exist").borrow().first_child();
        assert!(p.is_some());
        assert_eq!(
            LayoutObjectKind::Block,
            p.clone().expect("p node should exist").borrow().kind()
        );
        assert_eq!(
            NodeKind::Element(Element::new("p", Vec::new())),
            p.clone().expect("p node should exist").borrow().node_kind()
        );

        // その <p> は空（子なし）で、また 2 つ目の <p class="hidden"> は display:none のため兄弟も存在しない
        assert!(p
            .clone()
            .expect("p node should exist")
            .borrow()
            .first_child()
            .is_none());
        assert!(p
            .expect("p node should exist")
            .borrow()
            .next_sibling()
            .is_none());
    }
}
