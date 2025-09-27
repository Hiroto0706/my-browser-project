//! Page — ブラウザの「1ページ」を表す最小モデル（初心者向け）
//!
//! 役割（実ブラウザでの位置づけ）
//! - `Browser` が複数のページを管理し、その1枚が `Page` です。
//! - ネットワーク層から受け取った HTTP レスポンス本文（HTML 文字列）を、
//!   トークナイズ→パース（ツリービルド）して DOM（`Window`/`Document`）にし、
//!   さらに <style> から CSSOM を作り、レイアウト（ツリー構築→サイズ→位置）まで進めます。
//! - 最後に描画命令（DisplayItem）を得て、描画バックエンドに渡せる状態にします。
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
use crate::renderer::dom::api::get_style_content;
use crate::renderer::dom::node::Window;
use crate::renderer::html::parser::HtmlParser;
use crate::renderer::html::token::HtmlTokenizer;
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

    // 所属ブラウザを弱参照でセット（循環参照回避）。
    pub fn set_browser(&mut self, browser: Weak<RefCell<Browser>>) {
        self.browser = browser;
    }

    // ネットワーク応答（HTMLを含む）を受け取り、DOM/CSSOM→レイアウト→描画命令まで進める
    // フロー: create_frame(HTML→DOM/CSSOM) → set_layout_view(DOM+CSSOM→Layout) → paint_tree(Layout→DisplayItem)
    pub fn receive_response(&mut self, response: HttpResponse) {
        self.create_frame(response.body());
        self.set_layout_view();
        self.paint_tree();
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
