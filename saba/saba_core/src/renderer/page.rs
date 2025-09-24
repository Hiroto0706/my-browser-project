//! Page — ブラウザの「1ページ」を表す最小モデル（初心者向け）
//!
//! 役割（実ブラウザでの位置づけ）
//! - `Browser` が複数のページを管理し、その1枚が `Page` です。
//! - ネットワーク層から受け取った HTTP レスポンス本文（HTML 文字列）を、
//!   トークナイズ→パース（ツリービルド）して DOM（`Window`/`Document`）にします。
//! - 本実装では描画の代わりに、デバッグ用のテキストに変換して返します。
//!
//! 言語ブリッジ（TS / Python / Go）
//! - `Rc<RefCell<T>>`/`Weak<T>` は「共有 + 内部可変 / 循環参照回避」。
//! - `receive_response` は“ページがネットワーク応答を受け取り、DOMを更新する”入口メソッド。
//! - `create_frame` は“タブに表示するフレーム（Window）を作る”という意味合いです。

use crate::alloc::string::ToString;
use crate::browser::Browser;
use crate::http::HttpResponse;
use crate::renderer::dom::node::Window;
use crate::renderer::html::parser::HtmlParser;
use crate::renderer::html::token::HtmlTokenizer;
use crate::utils::convert_dom_to_string;
use alloc::rc::Rc;
use alloc::rc::Weak;
use alloc::string::String;
use core::cell::RefCell;

#[derive(Debug, Clone)]
pub struct Page {
    browser: Weak<RefCell<Browser>>,
    frame: Option<Rc<RefCell<Window>>>,
}

impl Page {
    // 新しい空のページを作成。まだブラウザやフレーム（Window）は紐づけられていません。
    pub fn new() -> Self {
        Self {
            browser: Weak::new(),
            frame: None,
        }
    }

    // 所属ブラウザを弱参照でセット（循環参照回避）。
    pub fn set_browser(&mut self, browser: Weak<RefCell<Browser>>) {
        self.browser = browser;
    }
    // ネットワーク応答（HTTP）を受け取り、DOM を構築し、デバッグ文字列を返す。
    // 実ブラウザではここから レンダリングツリー更新 → レイアウト → ペイント に進みますが、
    // 本書の最小実装では "DOMをテキスト化して返す" までとします。
    pub fn receive_response(&mut self, response: HttpResponse) -> String {
        // 1) レスポンスの本文（HTML）からフレーム(Window)を作る
        self.create_frame(response.body());

        // 2) デバッグ用に DOM ツリーを文字列として返す
        if let Some(frame) = &self.frame {
            let dom = frame.borrow().document().clone();
            let debug = convert_dom_to_string(&Some(dom));
            return debug;
        }

        "".to_string()
    }

    // HTML 文字列からトークナイザ/パーサを組み合わせて Window（DOMツリー）を構築する。
    // - HtmlTokenizer: 文字列 → トークン列（<tag>, </tag>, Char, Eof）
    // - HtmlParser: トークン列 → DOMツリー（Window/Document/Element/Text）
    fn create_frame(&mut self, html: String) {
        let html_tokenizer = HtmlTokenizer::new(html);
        let frame = HtmlParser::new(html_tokenizer).construct_tree();
        self.frame = Some(frame);
    }
}
