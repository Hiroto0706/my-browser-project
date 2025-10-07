//! Page — ブラウザの「1ページ」を表す最小モデル（初心者向け）
//!
//! 役割（実ブラウザでの位置づけ）
//! - `Browser` が複数のページを管理し、その1枚が `Page` です。
//! - ネットワーク層から受け取った HTTP レスポンス本文（HTML 文字列）を、
//!   トークナイズ→パース（ツリービルド）して DOM（`Window`/`Document`）にし、
//!   さらに <style> から CSSOM を作り、レイアウト（ツリー構築→サイズ→位置）まで進めます。
//! - 最後に描画命令（DisplayItem）を得て、描画バックエンドに渡せる状態にします。
//!
//! クリック判定（ヒットテスト）
//! - レイアウト済みツリーを使い、画面上の座標 (x,y) から「どのノード上か」を逆引きします。
//! - `clicked((x,y))` がその入口で、もし `<a href="…">` をクリックしていれば `Some(url)` を返します。
//! - 座標系は「コンテンツ左上が (0,0)」。ウィンドウのツールバー/余白分は呼び出し側で差し引きます。
//!
//! 言語ブリッジ（TS / Python / Go）
//! - `Rc<RefCell<T>>`/`Weak<T>` は「共有 + 内部可変 / 循環参照回避」。
//! - `receive_response` は“ページがネットワーク応答を受け取り、DOM/CSSOM→レイアウト→描画命令”へ進める入口メソッド。
//! - `create_frame` は“タブに表示するフレーム（Window）を作る”という意味合いです。
//! - `set_layout_view` は DOM/CSSOM からレイアウトツリーを作るステップ。
//! - `paint_tree` はレイアウトツリーから DisplayItem（矩形・テキストなど）を収集します。
//!
//! 具体例（最小の流れ）
//! 1) HTML: `<html><head><style>p{color:red}</style></head><body><p>Hi</p></body></html>`
//! 2) DOM:  Document → html → head/style, body/p/text("Hi")
//! 3) CSSOM: style から `p { color: red }`
//! 4) Layout: body をルートにレイアウトツリー構築 → サイズ/位置計算
//! 5) Paint: [Rect(..pの背景..), Text("Hi", ..座標..)] のような DisplayItem 列が得られる
use crate::browser::Browser;
use crate::display_item::DisplayItem;
use crate::http::HttpResponse;
use crate::renderer::css::cssom::CssParser;
use crate::renderer::css::cssom::StyleSheet;
use crate::renderer::css::token::CssTokenizer;
use crate::renderer::dom::api::get_js_content;
use crate::renderer::dom::api::get_style_content;
use crate::renderer::dom::node::ElementKind;
use crate::renderer::dom::node::NodeKind;
use crate::renderer::dom::node::Window;
use crate::renderer::html::parser::HtmlParser;
use crate::renderer::html::token::HtmlTokenizer;
use crate::renderer::js::ast::JsParser;
use crate::renderer::js::runtime::JsRuntime;
use crate::renderer::js::token::JsLexer;
use crate::renderer::layout::layout_view::LayoutView;
use alloc::rc::Rc;
use alloc::rc::Weak;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

#[derive(Debug, Clone)]
pub struct Page {
    browser: Weak<RefCell<Browser>>,
    frame: Option<Rc<RefCell<Window>>>,
    style: Option<StyleSheet>,
    layout_view: Option<LayoutView>,
    display_items: Vec<DisplayItem>,
}

impl Page {
    // 新しい空のページを作成。まだブラウザやフレーム（Window）、CSS/レイアウトは未設定。
    pub fn new() -> Self {
        Self {
            browser: Weak::new(),
            frame: None,
            style: None,
            layout_view: None,
            display_items: Vec::new(),
        }
    }

    /// 座標 `(x,y)` にある要素をヒットテストし、リンクなら `href` を返す
    ///
    /// 入力
    /// - `position`: コンテンツ領域基準の座標（左上が 0,0）。
    ///   例: UI 側で `y - TITLE_BAR_HEIGHT - TOOLBAR_HEIGHT` のように補正します。
    ///
    /// 出力
    /// - `Some(url)`: クリック位置が `<a href="…">`（または直下のテキスト）の場合、その URL。
    /// - `None`: リンクでない、またはレイアウト未生成などで見つからない場合。
    ///
    /// 実装の概要
    /// - `layout_view.find_node_by_position(position)` で、座標に重なるレイアウトノードを取得。
    /// - 見つかったノードの親が `<a>` 要素なら `href` 属性を返す（簡易版。祖先すべては辿らない）。
    pub fn clicked(&self, position: (i64, i64)) -> Option<String> {
        let view = match &self.layout_view {
            Some(v) => v,
            None => return None,
        };

        if let Some(n) = view.find_node_by_position(position) {
            if let Some(parent) = n.borrow().parent().upgrade() {
                if let NodeKind::Element(e) = parent.borrow().node_kind() {
                    if e.kind() == ElementKind::A {
                        return e.get_attribute("href");
                    }
                }
            }
        }

        None
    }

    // 所属ブラウザを弱参照でセット（循環参照回避）。
    pub fn set_browser(&mut self, browser: Weak<RefCell<Browser>>) {
        self.browser = browser;
    }

    // ネットワーク応答（HTML）を受け取り、DOM/CSSOM 構築 → JS 実行 → レイアウト → 描画命令 まで進める
    // フロー:
    // - create_frame: HTML→DOM、<style>→CSSOM を作成
    // - execute_js:   <script> を評価（DOM/属性の変更など副作用を反映）
    // - set_layout_view: 変化後の DOM + CSSOM からレイアウトツリーを構築
    // - paint_tree:   レイアウトツリーから DisplayItem（描画命令）を生成
    pub fn receive_response(&mut self, response: HttpResponse) {
        self.create_frame(response.body());
        self.execute_js();
        self.set_layout_view();
        self.paint_tree();
    }

    /// ページ内の `<script>` を取り出して実行する（超最小 JS ランタイム連携）
    ///
    /// 処理の流れ
    /// - DOM から `<script>` のテキストを抽出（`get_js_content`）。
    /// - JS を字句解析（`JsLexer`）→ 構文解析（`JsParser`）して AST を作る。
    /// - JS ランタイム（`JsRuntime`）を用意し、AST を評価して副作用（変数/DOM 変更）を反映。
    fn execute_js(&mut self) {
        // 1) DOM ルート（Document）を取得。ページが未構築なら何もしない
        let dom = match &self.frame {
            Some(frame) => frame.borrow().document(),
            None => return,
        };

        // 2) `<script>` の中身を抽出して、JS のソース文字列を得る
        let js = get_js_content(dom.clone());
        let lexer = JsLexer::new(js);

        // 3) JS をパースして AST を構築
        let mut parser = JsParser::new(lexer);
        let ast = parser.parse_ast();

        // 4) ランタイムを用意して AST を実行
        //    補足: DOM 連携（document.getElementById 等）のために DOM 参照を渡します。
        let mut runtime = JsRuntime::new(dom);
        runtime.execute(&ast);
    }

    // HTML 文字列から DOM（Window/Document）と CSSOM（StyleSheet）を作る
    fn create_frame(&mut self, html: String) {
        let html_tokenizer = HtmlTokenizer::new(html);
        let frame = HtmlParser::new(html_tokenizer).construct_tree();
        let dom = frame.borrow().document();

        let style = get_style_content(dom);
        let css_tokenizer = CssTokenizer::new(style);
        let cssom = CssParser::new(css_tokenizer).parse_stylesheet();

        self.frame = Some(frame);
        self.style = Some(cssom);
    }

    // DOM + CSSOM から LayoutView（レイアウトツリー）を作る
    fn set_layout_view(&mut self) {
        let dom = match &self.frame {
            Some(frame) => frame.borrow().document(),
            None => return,
        };

        let style = match self.style.clone() {
            Some(style) => style,
            None => return,
        };

        let layout_view = LayoutView::new(dom, &style);

        self.layout_view = Some(layout_view);
    }

    // レイアウトツリーから DisplayItem を収集（描画命令列）
    fn paint_tree(&mut self) {
        if let Some(layout_view) = &self.layout_view {
            self.display_items = layout_view.paint();
        }
    }

    // 収集済みの DisplayItem（矩形・テキストなど）を返す
    pub fn display_items(&self) -> Vec<DisplayItem> {
        self.display_items.clone()
    }

    // DisplayItem のバッファをクリアする（再描画前の初期化などに利用）
    pub fn clear_display_items(&mut self) {
        self.display_items = Vec::new();
    }
}
