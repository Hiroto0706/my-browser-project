use crate::alloc::string::ToString;
use crate::constants::CHAR_HEIGHT_WITH_PADDING;
use crate::constants::CHAR_WIDTH;
use crate::constants::CONTENT_AREA_WIDTH;
use crate::constants::WINDOW_PADDING;
use crate::constants::WINDOW_WIDTH;
use crate::display_item::DisplayItem;
use crate::renderer::css::cssom::ComponentValue;
use crate::renderer::css::cssom::Declaration;
use crate::renderer::css::cssom::Selector;
use crate::renderer::css::cssom::StyleSheet;
use crate::renderer::dom::node::Node;
use crate::renderer::dom::node::NodeKind;
use crate::renderer::layout::computed_style::Color;
use crate::renderer::layout::computed_style::ComputedStyle;
use crate::renderer::layout::computed_style::DisplayType;
use crate::renderer::layout::computed_style::FontSize;
use alloc::rc::Rc;
use alloc::rc::Weak;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

/// 単語境界（スペース）での折り返し位置を、右から左へ探す
/// 仕様（参考）: https://drafts.csswg.org/css-text/#word-break-property
///
/// 概要
/// - 1 行に収めたい最大インデックス `max_index` から左へ向かって走査し、
///   最初に見つかったスペースの位置を返します。
/// - 見つからなければ `max_index` を返して、そこで強制的に折り返します。
///
/// 例: line="hello world", max_index=8 → 5（"hello| world" の '|' 位置）
fn find_index_for_line_break(line: String, max_index: usize) -> usize {
    for i in (0..max_index).rev() {
        if line.chars().collect::<Vec<char>>()[i] == ' ' {
            return i;
        }
    }
    max_index
}

/// 等幅フォントの幅ベースで、テキストを複数行へ分割する（簡易ワードラップ）
/// 仕様（参考）: https://drafts.csswg.org/css-text/#word-break-property
///
/// 概要
/// - 各文字の見かけ幅を `char_width`（px）として、1行に収まる最大文字数を計算。
/// - その範囲内で右端からスペースを探し、そこで改行。残りの文字列に対して再帰的に繰り返す。
/// - スペースが見つからない（長い単語）場合は強制的に分割（簡易実装）。
///
/// 例
/// - WINDOW_WIDTH=600, WINDOW_PADDING=5, char_width=8 のとき
///   1行あたりの概算最大文字数 = (WINDOW_WIDTH + WINDOW_PADDING) / char_width
///   line が長ければ "... ... ..." のスペース位置で折り返し、Vec<String> に分割結果を返す。
fn split_text(line: String, char_width: i64) -> Vec<String> {
    let mut result: Vec<String> = vec![];
    if line.len() as i64 * char_width > (WINDOW_WIDTH + WINDOW_PADDING) {
        let s = line.split_at(find_index_for_line_break(
            line.clone(),
            ((WINDOW_WIDTH + WINDOW_PADDING) / char_width) as usize,
        ));
        result.push(s.0.to_string());
        result.extend(split_text(s.1.trim().to_string(), char_width))
    } else {
        result.push(line);
    }
    result
}

/// DOM ノードからレイアウトオブジェクト（描画用ノード）を1つ生成する
///
/// 概要
/// - 与えられた DOM `node` に対して、CSSOM のルールを適用（カスケード）し、
///   既定値/継承でスタイルを補完した上で `LayoutObject` を作ります。
/// - `display:none` の場合はオブジェクトを生成せず `None` を返します（レイアウトツリーから除外）。
/// - 最終的な `display` に基づき、`Block`/`Inline`/`Text` などの種類を確定します。
///
/// 引数
/// - `node`: 変換対象の DOM ノード（`None` のとき何もしない）
/// - `parent_obj`: 親レイアウト（継承や接続に使う）。ルートのときは `None`。
/// - `cssom`: スタイルシート（セレクタ照合して宣言を適用）。
///
/// 戻り値
/// - `Some(Rc<RefCell<LayoutObject>>)` 生成できた場合
/// - `None` `display:none` などで描画不要な場合
pub fn create_layout_object(
    node: &Option<Rc<RefCell<Node>>>,
    parent_obj: &Option<Rc<RefCell<LayoutObject>>>,
    cssom: &StyleSheet,
) -> Option<Rc<RefCell<LayoutObject>>> {
    if let Some(n) = node {
        // 1) DOM ノードに対応する LayoutObject の“器”を作る（まだスタイル未適用）
        // LayoutObjectを作成する
        let layout_object = Rc::new(RefCell::new(LayoutObject::new(n.clone(), parent_obj)));

        // 2) CSSOM の各ルールについて、セレクタにマッチするなら宣言を適用（カスケーディング）
        //    - is_node_selected: セレクタとこのノード（タグ名/クラス/ID等の簡易版）を照合
        //    - cascading_style: 指定された宣言群を style に反映（後勝ち）
        // CSSのルールをセレクタで選択されたノードに適用する
        for rule in &cssom.rules {
            if layout_object.borrow().is_node_selected(&rule.selector) {
                layout_object
                    .borrow_mut()
                    .cascading_style(rule.declarations.clone());
            }
        }

        // 3) 指定が無いプロパティは既定値 or 親からの継承で補う（defaulting）
        //    - 例: color は親から継承、display は要素種別により既定値、など
        // CSSでスタイルが指定されていない場合、デフォルトの値または親のノードから継承した値を使用する
        let parent_style = if let Some(parent) = parent_obj {
            Some(parent.borrow().style())
        } else {
            None
        };
        layout_object.borrow_mut().defaulting_style(n, parent_style);

        // 4) display:none ならレイアウトツリーに“存在しない”扱い → ここで除外
        // displayプロパティがnoneの場合、ノードを作成しない
        if layout_object.borrow().style().display() == DisplayType::DisplayNone {
            return None;
        }

        // 5) 最終的な display に基づいて、このノードの種類（Block/Inline/Text）を確定
        // displayプロパティの最終的な値を使用してノードの種類を決定する
        layout_object.borrow_mut().update_kind();
        return Some(layout_object);
    }
    None
}

// HTML要素は表示されるコンテンツの性質に合わせてブロック要素とインライン要素に分かれる
// ブロック要素をBlock、インライン要素をInlineで表現
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LayoutObjectKind {
    Block,
    Inline,
    Text,
}

// 描画に必要な情報を全て持った構造体
#[derive(Debug, Clone)]
pub struct LayoutObject {
    kind: LayoutObjectKind,
    node: Rc<RefCell<Node>>,
    first_child: Option<Rc<RefCell<LayoutObject>>>,
    next_sibling: Option<Rc<RefCell<LayoutObject>>>,
    parent: Weak<RefCell<LayoutObject>>,
    style: ComputedStyle,
    point: LayoutPoint,
    size: LayoutSize,
}

impl PartialEq for LayoutObject {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl LayoutObject {
    pub fn new(node: Rc<RefCell<Node>>, parent_obj: &Option<Rc<RefCell<LayoutObject>>>) -> Self {
        let parent = match parent_obj {
            Some(p) => Rc::downgrade(p),
            None => Weak::new(),
        };

        Self {
            kind: LayoutObjectKind::Block,
            node: node.clone(),
            first_child: None,
            next_sibling: None,
            parent,
            style: ComputedStyle::new(),
            point: LayoutPoint::new(0, 0),
            size: LayoutSize::new(0, 0),
        }
    }

    /// 自分自身に対応する描画命令（DisplayItem）を生成する
    ///
    /// ざっくりの方針（学習用）
    /// - display:none のときは何も描かない（空ベクタ）
    /// - Block 要素: 背景などの矩形（Rect）を 1 枚追加
    /// - Inline 要素: 現時点では描かない（将来 `<img>` などをここで処理）
    /// - Text ノード: テキストを折り返し単位で複数の Text DisplayItem に分割
    ///
    /// 具体例
    /// - <p style="background-color:yellow">Hi</p>
    ///   → Rect(bg=yellow, point=pの左上, size=pのサイズ)
    ///   → Text("Hi", point=pの左上)
    pub fn paint(&mut self) -> Vec<DisplayItem> {
        if self.style.display() == DisplayType::DisplayNone {
            return vec![];
        }

        match self.kind {
            LayoutObjectKind::Block => {
                // (d1)
                // ブロック要素は背景（や枠線など）を塗る前提の最小モデル。
                // ここでは常に 1 枚の Rect を返し、色や大きさは ComputedStyle/レイアウト結果に従う。
                if let NodeKind::Element(_e) = self.node_kind() {
                    return vec![DisplayItem::Rect {
                        style: self.style(),
                        layout_point: self.point(),
                        layout_size: self.size(),
                    }];
                }
            }
            LayoutObjectKind::Inline => { // (d2)
                 // 本書のブラウザでは、描画するインライン要素はない。
                 // <img>タグなどをサポートした場合はこのアームの中で処理をする
            }
            LayoutObjectKind::Text => {
                // (d3)
                // テキストはフォントサイズ（ratio）と等幅フォント幅（CHAR_WIDTH）から
                // 折り返し幅を計算し、行ごとに DisplayItem::Text を生成します。
                // 例:
                //   point=(x,y), ratio=1, CHAR_HEIGHT_WITH_PADDING=20, 行が3つ
                //   → Text("line1", point=(x, y))
                //   → Text("line2", point=(x, y+20))
                //   → Text("line3", point=(x, y+40))
                if let NodeKind::Text(t) = self.node_kind() {
                    let mut v = vec![];

                    let ratio = match self.style.font_size() {
                        FontSize::Medium => 1,
                        FontSize::XLarge => 2,
                        FontSize::XXLarge => 3,
                    };
                    // 改行はスペースに置換し、連続スペースを 1 個に圧縮（見た目の乱れを抑える）
                    let plain_text = t
                        .replace("\n", " ")
                        .split(' ')
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    // 1 行あたりに乗る最大幅（px）を与えてテキストを折り返す
                    let lines = split_text(plain_text, CHAR_WIDTH * ratio);
                    let mut i = 0;
                    for line in lines {
                        let item = DisplayItem::Text {
                            text: line,
                            style: self.style(),
                            layout_point: LayoutPoint::new(
                                self.point().x(),
                                self.point().y() + CHAR_HEIGHT_WITH_PADDING * i,
                            ),
                        };
                        v.push(item);
                        i += 1;
                    }

                    return v;
                }
            }
        }

        vec![]
    }

    /// 子のサイズをもとに、このノードのレイアウトサイズ（幅・高さ）を計算する
    ///
    /// ルール（学習用の簡易モデル）
    /// - Block: 親の幅をそのまま使い、高さは“子の高さの合計”（ただしインラインが横並びの場合は注意）。
    /// - Inline: 幅=子の幅の合計, 高さ=子の高さの合計（横並び/改行の厳密処理は省略）。
    /// - Text: 文字数×フォント比率×等幅フォント幅で幅を見積もり、コンテンツ幅を超えたら折り返し行数で高さを算出。
    ///
    /// 具体例
    /// - <div>（Block）に子が <p>(20px 高) と <h1>(24px 高) → 高さ=44px, 幅=親幅。
    /// - <span>（Inline）に "Hi"(文字高さ16px, 幅 2×8px) と 子 <a> の幅足し込み → 幅=合計, 高さ=16px など。
    pub fn compute_size(&mut self, parent_size: LayoutSize) {
        let mut size = LayoutSize::new(0, 0);

        match self.kind() {
            LayoutObjectKind::Block => {
                size.set_width(parent_size.width());

                // 全ての子ノードの高さを足し合わせた結果が高さになる。
                // ただし、インライン要素が横に並んでいる場合は注意が必要
                let mut height = 0;
                let mut child = self.first_child();
                let mut previous_child_kind = LayoutObjectKind::Block;
                while child.is_some() {
                    let c = match child {
                        Some(c) => c,
                        None => panic!("first child should exist"),
                    };

                    // 簡易モデル: Block → Block は縦積み、高さを単純加算。
                    // Inline が続く場合は横並び想定だが、ここでは単純化して
                    // “前が Block だった/今が Block だった”ときだけ加算する制御を入れている。
                    if previous_child_kind == LayoutObjectKind::Block
                        || c.borrow().kind() == LayoutObjectKind::Block
                    {
                        height += c.borrow().size.height();
                    }

                    previous_child_kind = c.borrow().kind();
                    child = c.borrow().next_sibling();
                }
                size.set_height(height);
            }
            LayoutObjectKind::Inline => {
                // 全ての子ノードの高さと横幅を足し合わせた結果が現在のノードの高さと横幅とになる
                // 注: 本来は “同じ行の最大高さ＝行の高さ” だが、学習用に単純合計としている。
                let mut width = 0;
                let mut height = 0;
                let mut child = self.first_child();
                while child.is_some() {
                    let c = match child {
                        Some(c) => c,
                        None => panic!("first child should exist"),
                    };

                    width += c.borrow().size.width();
                    height += c.borrow().size.height();

                    child = c.borrow().next_sibling();
                }

                size.set_width(width);
                size.set_height(height);
            }
            LayoutObjectKind::Text => {
                if let NodeKind::Text(t) = self.node_kind() {
                    let ratio = match self.style.font_size() {
                        FontSize::Medium => 1,
                        FontSize::XLarge => 2,
                        FontSize::XXLarge => 3,
                    };
                    // 文字幅 = 等幅フォント幅 × フォント倍率 × 文字数
                    let width = CHAR_WIDTH * ratio * t.len() as i64;
                    if width > CONTENT_AREA_WIDTH {
                        // テキストが複数行のとき
                        size.set_width(CONTENT_AREA_WIDTH);
                        let line_num = if width.wrapping_rem(CONTENT_AREA_WIDTH) == 0 {
                            width.wrapping_div(CONTENT_AREA_WIDTH)
                        } else {
                            width.wrapping_div(CONTENT_AREA_WIDTH) + 1
                        };
                        // 行高 = 文字高さ(行送り込) × フォント倍率 × 行数
                        size.set_height(CHAR_HEIGHT_WITH_PADDING * ratio * line_num);
                    } else {
                        // テキストが1行に収まるとき
                        size.set_width(width);
                        // 行高 = 文字高さ(行送り込) × フォント倍率
                        size.set_height(CHAR_HEIGHT_WITH_PADDING * ratio);
                    }
                }
            }
        }

        self.size = size;
    }

    /// 直前の兄弟や親の位置から、このノードの描画位置（左上座標）を決める
    ///
    /// ルール（学習用の簡易モデル）
    /// - ブロック要素は縦に積む（Y方向に進む）。兄弟が Block でも自分が Block でも改行して下へ。
    /// - インライン要素同士は横に並べる（X方向に進む）。行折り返しの厳密処理は省略。
    /// - 最初の子/兄弟が無い場合は、親の `parent_point` を基準に置く。
    ///
    /// 引数
    /// - `parent_point`: 親のコンテンツ開始位置（この座標から子を配置）
    /// - `previous_sibling_kind`: 直前の兄弟の種類（Block/Inline/Text）。配置方向を決めるヒント
    /// - `previous_sibling_point`: 直前の兄弟の配置座標（Some の時のみ利用）
    /// - `previous_sibling_size`: 直前の兄弟のサイズ（高さ/幅の足し込みに使用）
    pub fn compute_position(
        &mut self,
        parent_point: LayoutPoint,
        previous_sibling_kind: LayoutObjectKind,
        previous_sibling_point: Option<LayoutPoint>,
        previous_sibling_size: Option<LayoutSize>,
    ) {
        let mut point = LayoutPoint::new(0, 0);

        match (self.kind(), previous_sibling_kind) {
            // もしブロック要素が兄弟ノードの場合、Y軸方向に進む
            // 具体例: <p> の次に <h1> → h1.y = p.y + p.height / h1.x = 親の x
            (LayoutObjectKind::Block, _) | (_, LayoutObjectKind::Block) => {
                if let (Some(size), Some(pos)) = (previous_sibling_size, previous_sibling_point) {
                    point.set_y(pos.y() + size.height());
                } else {
                    point.set_y(parent_point.y());
                }
                point.set_x(parent_point.x());
            }
            // もしインライン要素が並ぶ場合、X軸方向に進む
            // 具体例: <span>a</span><span>b</span> → 2つ目の x = 1つ目の x + 1つ目の幅, y は同じ
            (LayoutObjectKind::Inline, LayoutObjectKind::Inline) => {
                if let (Some(size), Some(pos)) = (previous_sibling_size, previous_sibling_point) {
                    point.set_x(pos.x() + size.width());
                    point.set_y(pos.y());
                } else {
                    point.set_x(parent_point.x());
                    point.set_y(parent_point.y());
                }
            }
            _ => {
                // それ以外（例えば最初の子や、前の兄弟が無い/不定のケース）は親座標に揃える
                point.set_x(parent_point.x());
                point.set_y(parent_point.y());
            }
        }

        self.point = point;
    }

    //  CSSのルールをレイアウトツリーのノードに適用すべきかどうかを判断する
    //  具体的には、与えられた `selector`（簡易: タグ/クラス/id）と、このレイアウトオブジェクトが
    //  参照している DOM ノード（Element）の情報を突き合わせ、マッチすれば true を返す。
    //
    //  簡易仕様（学習用）
    //  - TypeSelector("p")  → 要素名が "p" のときに一致
    //  - ClassSelector("note") → 属性 `class="note"` のときに一致（複数クラスは未対応）
    //  - IdSelector("main") → 属性 `id="main"` のときに一致
    //  - UnknownSelector → 常に不一致
    //  注: 現実の CSS は複合セレクタやスペース区切り（子孫/子/兄弟）など多様だが、ここでは最小限のみ。
    pub fn is_node_selected(&self, selector: &Selector) -> bool {
        match &self.node_kind() {
            NodeKind::Element(e) => match selector {
                // タグ名（タイプセレクタ）: 例) p { ... }
                Selector::TypeSelector(type_name) => {
                    // ElementKind → 文字列（"p" など）に直して完全一致で判定
                    if e.kind().to_string() == *type_name {
                        return true;
                    }
                    false
                }
                // クラスセレクタ: 例) .note { ... }
                Selector::ClassSelector(class_name) => {
                    // 属性列から name="class" を探し、値が完全一致するかで判定
                    // 注意: 本実装は "class1 class2" のような複数クラスを分割しない（簡易）
                    for attr in &e.attributes() {
                        if attr.name() == "class" && attr.value() == *class_name {
                            return true;
                        }
                    }
                    false
                }
                // ID セレクタ: 例) #main { ... }
                Selector::IdSelector(id_name) => {
                    // 属性列から name="id" を探し、値が完全一致するかで判定
                    for attr in &e.attributes() {
                        if attr.name() == "id" && attr.value() == *id_name {
                            return true;
                        }
                    }
                    false
                }
                // 未対応（あるいは読み飛ばし対象）のセレクタは常に不一致
                Selector::UnknownSelector => false,
            },
            // テキスト/ドキュメントはセレクタの対象外とする
            _ => false,
        }
    }

    // ノードがセレクタによって選択されている場合、そのCSSルールをノードに適用する
    // CSSの宣言リストを引数に取り、各宣言のプロパティをノードに適用する
    //
    // 関数の概要
    // - `declarations` に含まれる `property: value` を 1 件ずつこの LayoutObject の ComputedStyle に反映します。
    // - サポート範囲（学習用）: background-color / color / display
    // - 値の受け取り方は簡易トークンベース（ComponentValue = CssToken）。
    //
    // 具体例
    // - background-color: Ident("red")         → set_background_color(#ff0000)
    // - background-color: HashToken("#00ff00") → set_background_color(#00ff00)
    // - color: Ident("blue")                   → set_color(#0000ff)
    // - display: Ident("block")                → set_display(DisplayType::Block)
    pub fn cascading_style(&mut self, declarations: Vec<Declaration>) {
        for declaration in declarations {
            match declaration.property.as_str() {
                "background-color" => {
                    // 例1) background-color: red;  → Ident("red") を色名に解釈
                    if let ComponentValue::Ident(value) = &declaration.value {
                        let color = match Color::from_name(&value) {
                            Ok(color) => color,
                            Err(_) => Color::white(),
                        };
                        self.style.set_background_color(color);
                        continue;
                    }

                    // 例2) background-color: #00ff00; → HashToken("#00ff00") を #RRGGBB として解釈
                    if let ComponentValue::HashToken(color_code) = &declaration.value {
                        let color = match Color::from_code(&color_code) {
                            Ok(color) => color,
                            Err(_) => Color::white(),
                        };
                        self.style.set_background_color(color);
                        continue;
                    }
                }
                "color" => {
                    // 例3) color: blue; → Ident("blue")
                    if let ComponentValue::Ident(value) = &declaration.value {
                        let color = match Color::from_name(&value) {
                            Ok(color) => color,
                            Err(_) => Color::black(),
                        };
                        self.style.set_color(color);
                    }

                    // 例4) color: #0000ff; → HashToken("#0000ff")
                    if let ComponentValue::HashToken(color_code) = &declaration.value {
                        let color = match Color::from_code(&color_code) {
                            Ok(color) => color,
                            Err(_) => Color::black(),
                        };
                        self.style.set_color(color);
                    }
                }
                "display" => {
                    // 例5) display: block; / inline; / none; → Ident("block"|"inline"|"none")
                    if let ComponentValue::Ident(value) = declaration.value {
                        let display_type = match DisplayType::from_str(&value) {
                            Ok(display_type) => display_type,
                            Err(_) => DisplayType::DisplayNone,
                        };
                        self.style.set_display(display_type)
                    }
                }
                _ => {}
            }
        }
    }

    pub fn defaulting_style(
        &mut self,
        node: &Rc<RefCell<Node>>,
        parent_style: Option<ComputedStyle>,
    ) {
        self.style.defaulting(node, parent_style);
    }

    // カスケード、デフォルティングを経てCSSの値が最終的に決定した後、あらためてLayoutObjectのノードがブロック要素になるかインライン要素になるかを決定する
    //
    // 関数の概要
    // - この時点で `self.style` には、指定（cascading）→ 既定/継承（defaulting）を通過した
    //   “最終”の display が入っています。その値にもとづき `LayoutObjectKind` を確定します。
    // - Document はレイアウトオブジェクト化しない前提（パニック）。Text は常に Text。
    // - display:none はレイアウトツリーに現れないはずなので、ここに来たらロジック不整合としてパニック。
    //
    // 具体例
    // - <p style="display:block"> ... → kind=Block
    // - <span style="display:inline"> ... → kind=Inline
    // - テキストノード → kind=Text
    pub fn update_kind(&mut self) {
        match self.node_kind() {
            NodeKind::Document => panic!("should not create a layout object for a Document node"),
            NodeKind::Element(_) => {
                // 1) 最終的な display を取得
                let display = self.style.display();
                match display {
                    // 2) display の種類に応じて LayoutObjectKind を確定
                    DisplayType::Block => self.kind = LayoutObjectKind::Block,
                    DisplayType::Inline => self.kind = LayoutObjectKind::Inline,
                    DisplayType::DisplayNone => {
                        panic!("should not create a layout object for display:none")
                    }
                }
            }
            // テキストノードは常に Text（行内に流れる）
            NodeKind::Text(_) => self.kind = LayoutObjectKind::Text,
        }
    }

    // LayoutObjectのフィールドは全てプライベートなので、変更・取得のメソッドを追加する
    pub fn kind(&self) -> LayoutObjectKind {
        self.kind
    }

    pub fn node_kind(&self) -> NodeKind {
        self.node.borrow().kind().clone()
    }

    pub fn set_first_child(&mut self, first_child: Option<Rc<RefCell<LayoutObject>>>) {
        self.first_child = first_child;
    }

    pub fn first_child(&self) -> Option<Rc<RefCell<LayoutObject>>> {
        self.first_child.as_ref().cloned()
    }

    pub fn set_next_sibling(&mut self, next_sibling: Option<Rc<RefCell<LayoutObject>>>) {
        self.next_sibling = next_sibling;
    }

    pub fn next_sibling(&self) -> Option<Rc<RefCell<LayoutObject>>> {
        self.next_sibling.as_ref().cloned()
    }

    pub fn parent(&self) -> Weak<RefCell<Self>> {
        self.parent.clone()
    }

    pub fn style(&self) -> ComputedStyle {
        self.style.clone()
    }

    pub fn point(&self) -> LayoutPoint {
        self.point
    }

    pub fn size(&self) -> LayoutSize {
        self.size
    }
}

// LayoutObjectの位置を表すデータ構造
// レイアウトツリー構築の際に、各要素の描画される位置を計算する
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct LayoutPoint {
    x: i64,
    y: i64,
}

impl LayoutPoint {
    pub fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    pub fn x(&self) -> i64 {
        self.x
    }

    pub fn y(&self) -> i64 {
        self.y
    }

    pub fn set_x(&mut self, x: i64) {
        self.x = x;
    }

    pub fn set_y(&mut self, y: i64) {
        self.y = y;
    }
}

// LayoutObjectのサイズを表すデータ構造
// レイアウトツリー構築の際に、各要素のサイズを計算する
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct LayoutSize {
    width: i64,
    height: i64,
}

impl LayoutSize {
    pub fn new(width: i64, height: i64) -> Self {
        Self { width, height }
    }

    pub fn width(&self) -> i64 {
        self.width
    }

    pub fn height(&self) -> i64 {
        self.height
    }

    pub fn set_width(&mut self, width: i64) {
        self.width = width;
    }

    pub fn set_height(&mut self, height: i64) {
        self.height = height;
    }
}
