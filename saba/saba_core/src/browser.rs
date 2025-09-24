//! ブラウザ本体（Browser）— 複数ページの管理と“現在のページ”の取得
//!
//! 役割（実ブラウザのどの部分？）
//! - タブ/ウィンドウの管理に相当します。ここでは最小構成として「複数ページ(Page)配列」と
//!   「アクティブなページのインデックス」だけを持ちます。
//! - `Page` が DOM 構築やレンダリングの具体を担当し、`Browser` は切替や生成を司るイメージです。
//!
//! 言語ブリッジ（TS / Python / Go）
//! - `Rc<RefCell<T>>` は“共有 + 内部可変”。複数箇所から同じ `Page` を参照し、必要時に書き換えます。
//!   - TS/Python/Go だと普通の参照共有に近い感覚ですが、Rust では所有権のために包む必要があります。
//! - `Weak<...>` を使うと循環参照を避けられます（`Page` → `Browser` の逆参照など）。
//!
//! 使い方（最小例）
//! ```ignore
//! let browser = Browser::new();
//! let page = browser.borrow().current_page();
//! // page.borrow_mut().navigate("http://...") のようなAPIを増やしていく想定
//! ```

use crate::renderer::page::Page;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

#[derive(Debug, Clone)]
pub struct Browser {
    active_page_index: usize,
    pages: Vec<Rc<RefCell<Page>>>,
}

impl Browser {
    // 新しいブラウザを作成し、空の Page を1枚だけ持った状態で返します。
    // 返り値を Rc<RefCell<Self>> にしているのは、アプリのあちこちから共有し、
    // 必要に応じて内部を書き換えたい（内部可変にしたい）ためです。
    pub fn new() -> Rc<RefCell<Self>> {
        let mut page = Page::new();

        let browser = Rc::new(RefCell::new(Self {
            active_page_index: 0,
            pages: Vec::new(),
        }));

        // Page から Browser への逆参照（弱参照）を張ることで、
        // 循環参照（強参照サイクル）にならないようにしています。
        page.set_browser(Rc::downgrade(&browser));
        // 最初のページをブラウザに登録。
        browser.borrow_mut().pages.push(Rc::new(RefCell::new(page)));

        browser
    }

    // 現在アクティブなページ（タブ）を取得します。
    // ここでは単一タブ運用のため常に index=0 を返しますが、将来はタブ切替で変動します。
    pub fn current_page(&self) -> Rc<RefCell<Page>> {
        self.pages[self.active_page_index].clone()
    }
}
