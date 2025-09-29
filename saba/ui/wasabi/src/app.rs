use crate::alloc::string::ToString;
use crate::cursor::Cursor;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use core::cell::RefCell;
use noli::error::Result as OsResult;
use noli::prelude::SystemApi;
use noli::println;
use noli::rect::Rect;
use noli::sys::api::MouseEvent;
use noli::sys::wasabi::Api;
use noli::window::StringSize;
use noli::window::Window;
use saba_core::browser::Browser;
use saba_core::constants::WHITE;
use saba_core::constants::WINDOW_HEIGHT;
use saba_core::constants::WINDOW_INIT_X_POS;
use saba_core::constants::WINDOW_INIT_Y_POS;
use saba_core::constants::WINDOW_WIDTH;
use saba_core::constants::*;
use saba_core::display_item::DisplayItem;
use saba_core::error::Error;
use saba_core::http::HttpResponse;
use saba_core::renderer::layout::computed_style::FontSize;
use saba_core::renderer::layout::computed_style::TextDecoration;

// 状態がNormalの時は入力ができず、Editingの時は文字入力ができる
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputMode {
    Normal,
    Editing,
}

#[derive(Debug)]
pub struct WasabiUI {
    browser: Rc<RefCell<Browser>>,
    input_url: String,
    input_mode: InputMode,
    window: Window,
    cursor: Cursor,
}

impl WasabiUI {
    pub fn new(browser: Rc<RefCell<Browser>>) -> Self {
        Self {
            browser,
            input_url: String::new(),
            input_mode: InputMode::Normal,
            window: Window::new(
                "saba".to_string(), // windowの名前
                WHITE,              // 色
                WINDOW_INIT_X_POS,  // ウィンドウの初期位置のX座標
                WINDOW_INIT_Y_POS,  // 初期位置のY座標
                WINDOW_WIDTH,       // ウィンドウの横幅
                WINDOW_HEIGHT,      // ウィンドウの縦幅
            )
            .unwrap(),
            cursor: Cursor::new(),
        }
    }

    /// UI の起動エントリ（1 回だけの初期化 → イベントループ開始）
    ///
    /// 引数
    /// - `handle_url`: アドレスバーで受け取った URL を処理して `HttpResponse` を返すコールバック。
    ///   例: `|url| http_client.get(url)` のような関数ポインタを渡す想定。
    ///
    /// 流れ
    /// 1) `setup()` … ツールバー描画などの初期化を行い画面を一度フラッシュ
    /// 2) `run_app()` … 入力（マウス/キーボード）を処理するイベントループに入る
    pub fn start(
        &mut self,
        handle_url: fn(String) -> Result<HttpResponse, Error>,
    ) -> Result<(), Error> {
        self.setup()?;

        self.run_app(handle_url)?;

        Ok(())
    }

    /// メインのイベントループ
    ///
    /// - マウス入力・キーボード入力をポーリングし、必要に応じて `handle_url` を呼びます。
    /// - 例えば、Enter 押下でアドレスバーの文字列を URL とみなし、`handle_url(url)` で取得した
    ///   `HttpResponse` を Page に渡して再描画する……といった流れを組みます。
    fn run_app(
        &mut self,
        handle_url: fn(String) -> Result<HttpResponse, Error>,
    ) -> Result<(), Error> {
        loop {
            // マウスイベント（クリック/ドラッグ/スクロール 等）の処理
            self.handle_mouse_input()?;
            // キーイベント（文字入力/Enter/Esc 等）の処理
            self.handle_key_input(handle_url)?;
        }
    }

    /// マウス入力を処理する（カーソルの表示更新とクリック判定）
    ///
    /// 何をしているか（初心者向け）
    /// - `Api::get_mouse_cursor_info()` から「ボタン状態 + 位置」を取得。
    /// - カーソル（赤い 10x10 のシート）の位置を更新 → 部分フラッシュで描画を反映。
    /// - クリックがあれば、座標をウィンドウ左上基準の「相対座標」に変換して範囲判定。
    ///   - ツールバー帯をクリック: 入力モード `Editing` に切り替え、アドレスバーをクリア。
    ///   - それ以外: 入力モードを `Normal` に戻す。
    ///
    /// 補足（TS/Python 的たとえ）
    /// - `if let Some(e) = get_event()` は「イベントがあれば処理、なければスキップ」のパターン。
    /// - 画面更新は「バッファ→画面」の二段階。`flush_area(rect)` は部分コミット。
    fn handle_mouse_input(&mut self) -> Result<(), Error> {
        // 最新のマウス情報を取得（イベントが無いフレームは None）
        if let Some(MouseEvent { button, position }) = Api::get_mouse_cursor_info() {
            // 1) 直前のカーソル描画領域を部分フラッシュして掃除（前の位置の残像を消す）
            self.window.flush_area(self.cursor.rect());
            // 2) カーソルのバッファ上の座標を更新
            self.cursor.set_position(position.x, position.y);
            // 3) 新しい位置の領域を部分フラッシュして表示
            self.window.flush_area(self.cursor.rect());
            // 4) カーソルの自身のシートも更新（内部の変更を反映）
            self.cursor.flush();

            // いずれかのボタンが押されている？ (L/C/R)
            if button.l() || button.c() || button.r() {
                // ウィンドウ基準の相対座標に変換（ウィンドウの左上を原点とする）
                let relative_pos = (
                    position.x - WINDOW_INIT_X_POS,
                    position.y - WINDOW_INIT_Y_POS,
                );

                // ウィンドウ外なら何もしない（ログだけ出す）
                if relative_pos.0 < 0
                    || relative_pos.0 > WINDOW_WIDTH
                    || relative_pos.1 < 0
                    || relative_pos.1 > WINDOW_HEIGHT
                {
                    println!("button clicked OUTSIDE window: {button:?} {position:?}");
                    return Ok(());
                }

                // ツールバー帯（タイトルバー直下〜TOOLBAR_HEIGHT 分のエリア）をクリック
                // → 入力開始モードへ。既存文字は消して真っさらに。
                if relative_pos.1 < TOOLBAR_HEIGHT + TITLE_BAR_HEIGHT
                    && relative_pos.1 >= TITLE_BAR_HEIGHT
                {
                    self.clear_address_bar()?;
                    self.input_url = String::new();
                    self.input_mode = InputMode::Editing;
                    println!("button clicked in toolbar: {button:?} {position:?}");
                    return Ok(());
                }

                // ツールバー以外をクリックしたので通常モードへ戻す
                self.input_mode = InputMode::Normal;
            }
        }

        Ok(())
    }

    /// キー入力を処理する（アドレスバーへの文字編集とナビゲーション開始）
    ///
    /// 振る舞い
    /// - `InputMode::Normal` … キー入力は無視（読み捨て）。
    /// - `InputMode::Editing` … 1 文字ずつ読み、`Backspace/Delete` なら削除、それ以外は追記。
    ///   Enter 押下で `handle_url` コールバックを使ってナビゲーションを開始します。
    ///   各入力後に `update_address_bar()` で部分再描画します。
    ///
    /// 引数
    /// - `handle_url: fn(String) -> Result<HttpResponse, Error>`: 入力 URL を処理して HTTP レスポンスを返す関数ポインタ。
    ///   例えば `|url| http_client.get(url)` のような関数を渡す想定です。
    ///
    /// 補足
    /// - Enter のキーコードは `0x0A`（LF）。この OS/入力 API では Enter が LF として届きます。
    ///   環境によっては CR(`0x0D`) の場合もありますが、ここでは LF を採用しています。
    fn handle_key_input(
        &mut self,
        handle_url: fn(String) -> Result<HttpResponse, Error>,
    ) -> Result<(), Error> {
        match self.input_mode {
            InputMode::Normal => {
                // 入力不可モード。イベントキューが溜まらないよう読み捨てだけ行う。
                let _ = Api::read_key();
            }
            InputMode::Editing => {
                // 1 フレームに 0/1 文字想定で読み取り（なければ None）
                if let Some(c) = Api::read_key() {
                    if c == 0x0A as char {
                        // Enter(LF) が押された: 入力中 URL でナビゲーションを開始
                        // - `start_navigation` 内でコンテンツ領域をクリア → HTTP を実行 → ページへ反映 → UI 更新
                        self.start_navigation(handle_url, self.input_url.clone())?;

                        self.input_url = String::new();
                        self.input_mode = InputMode::Normal;
                    } else if c == 0x7F as char || c == 0x08 as char {
                        // Delete(0x7F) / Backspace(0x08): 末尾 1 文字を削る
                        self.input_url.pop();
                        // バッファの変更をアドレスバーへ反映（部分フラッシュ内蔵）
                        self.update_address_bar()?;
                    } else {
                        // 通常の可視文字を末尾に追加
                        self.input_url.push(c);
                        // 表示を更新
                        self.update_address_bar()?;
                    }
                }
            }
        }

        Ok(())
    }

    /// 入力 URL に対するナビゲーション（取得→描画）をまとめて行う
    ///
    /// 流れ
    /// 1) `clear_content_area()` … コンテンツ表示領域を真っさらにする（前のページを消す）。
    /// 2) `handle_url(destination)` … URL を処理して `HttpResponse` を取得（HTTP GET 相当）。
    /// 3) `current_page().receive_response(response)` … 取得レスポンスを現在のページモデルに流し込む。
    /// 4) `update_ui()` … 画面の再描画（アドレスバー/コンテンツなどの見た目を最新に）。
    ///
    /// エラーハンドリング
    /// - `handle_url` が失敗したら、そのまま `Error` を返して上位に伝えます。
    ///
    /// メモ
    /// - `browser.borrow().current_page()` は `Rc<RefCell<_>>` 越しにページモデルへアクセスしています。
    ///   Rust の「所有権/借用」では「可変アクセスは一つだけ」というルールがあるため、
    ///   ここでは `RefCell` を使って実行時に可変借用の整合性チェックを行っています。
    fn start_navigation(
        &mut self,
        handle_url: fn(String) -> Result<HttpResponse, Error>,
        destination: String,
    ) -> Result<(), Error> {
        // 1) 旧コンテンツを消す（スクリーン上の中身をクリア）
        self.clear_content_area()?;

        // 2) URL を処理し、HTTP レスポンスを得る
        match handle_url(destination) {
            Ok(response) => {
                // 3) 現在のページにレスポンスを適用（ページモデルが再描画用データを持つ想定）
                let page = self.browser.borrow().current_page();
                page.borrow_mut().receive_response(response);
            }
            Err(e) => {
                return Err(e);
            }
        }

        // 4) UI 全体を最新状態へ（必要範囲のフラッシュなどを行う）
        self.update_ui()?;

        Ok(())
    }

    /// ブラウザの「表示リスト」を画面に描く（テキスト/矩形）
    ///
    /// 仕組み（初心者向け）
    /// - `display_items()` は「何をどこにどう描くか」の並び（Display List）。
    ///   Web ブラウザのレイアウト後の出力に近い概念で、最終的な描画指示です。
    /// - 各 `DisplayItem` を `draw_string` や `fill_rect` に落として実際のウィンドウへ描きます。
    /// - 座標は「コンテンツ左上」を基準に計算しつつ、`WINDOW_PADDING` と `TOOLBAR_HEIGHT` を
    ///   加算してウィンドウ内の実座標へずらします（余白とツールバー分のオフセット）。
    /// - 最後に `flush()` でバッファの内容を画面へ反映します（部分でなく全体フラッシュ）。
    ///
    /// TS/Python たとえ
    /// - `DisplayItem::Text { ... }` は TS の判別可能ユニオンに似ています（`kind === "Text"`）。
    /// - `match` は `switch` より型安全な分岐。全バリアントを網羅しないとコンパイル警告が出ます。
    fn update_ui(&mut self) -> Result<(), Error> {
        let display_items = self
            .browser
            .borrow()
            .current_page()
            .borrow()
            .display_items();

        for item in display_items {
            match item {
                DisplayItem::Text {
                    text,
                    style,
                    layout_point,
                } => {
                    // テキストを描く
                    // - 色: `style.color()`（CSS 的な色）→ `code_u32()` で 0xRRGGBB へ
                    // - 座標: レイアウト済みの点 + 余白 + ツールバー高
                    // - フォント: レンダラーの `FontSize` → `convert_font_size` で描画 API の段階へ
                    // - 下線: `text_decoration` が Underline のとき true
                    if self
                        .window
                        .draw_string(
                            style.color().code_u32(),
                            layout_point.x() + WINDOW_PADDING,
                            layout_point.y() + WINDOW_PADDING + TOOLBAR_HEIGHT,
                            &text,
                            convert_font_size(style.font_size()),
                            style.text_decoration() == TextDecoration::Underline,
                        )
                        .is_err()
                    {
                        return Err(Error::InvalidUI("failed to draw a string".to_string()));
                    }
                }
                DisplayItem::Rect {
                    style,
                    layout_point,
                    layout_size,
                } => {
                    // 塗りつぶし矩形を描く（背景など）
                    // - 色: `background_color`
                    // - 位置とサイズ: レイアウトの結果 + 余白/ツールバー分のオフセット
                    if self
                        .window
                        .fill_rect(
                            style.background_color().code_u32(),
                            layout_point.x() + WINDOW_PADDING,
                            layout_point.y() + WINDOW_PADDING + TOOLBAR_HEIGHT,
                            layout_size.width(),
                            layout_size.height(),
                        )
                        .is_err()
                    {
                        return Err(Error::InvalidUI("failed to draw a string".to_string()));
                    }
                }
            }
        }

        // 最後にまとめて画面へ反映。多くの `draw_*` はバッファに描くだけなので、
        // `flush()` しないと実画面に見えません。
        self.window.flush();

        Ok(())
    }

    /// 一度だけ行う UI 初期化
    ///
    /// - ツールバー/アドレスバーの描画（`setup_toolbar`）
    /// - 描画内容を画面へ反映（`flush`）
    fn setup(&mut self) -> Result<(), Error> {
        if let Err(error) = self.setup_toolbar() {
            // OsResultとResultが持つError型は異なるので、変換する
            return Err(Error::InvalidUI(format!(
                "failed to initialize a toolbar with error: {:#?}",
                error
            )));
        }
        // 画面バッファの内容を実際の画面に転送する（初期描画）
        self.window.flush();
        Ok(())
    }

    /// 画面上部のツールバー（アドレスバー含む）を 1 回だけ描く
    ///
    /// 何をしているか（初心者向け）
    /// - "塗る" → `fill_rect(色, x, y, 幅, 高さ)` で長方形を塗りつぶします。
    /// - "線を引く" → `draw_line(色, x1, y1, x2, y2)` で 1 ピクセルの線を描きます。
    /// - "文字" → `draw_string(色, x, y, 文字列, サイズ, 下線)` でテキストを描きます。
    ///
    /// レイアウトの考え方
    /// - ツールバーはウィンドウ左上から `y=0..TOOLBAR_HEIGHT` の帯。
    /// - アドレスバーは x=70px から右端まで広げ、上/左に薄い影、内側に黒線で立体感を出しています。
    ///
    /// 具体例
    /// - 幅 600px の場合、アドレスバーは x=70..596px あたりまでの白い長方形（右端 4px 余白）。
    fn setup_toolbar(&mut self) -> OsResult<()> {
        // ツールバーの背景（横一帯）を塗る
        self.window
            .fill_rect(LIGHTGREY, 0, 0, WINDOW_WIDTH, TOOLBAR_HEIGHT)?;

        // ツールバーとコンテンツエリアの境目に“2 本線”を引いて段差を表現
        // 1 本目: GREY の線（境界）
        self.window
            .draw_line(GREY, 0, TOOLBAR_HEIGHT, WINDOW_WIDTH - 1, TOOLBAR_HEIGHT)?;
        // 2 本目: DARKGREY の線（すぐ下に影）
        self.window.draw_line(
            DARKGREY,
            0,
            TOOLBAR_HEIGHT + 1,
            WINDOW_WIDTH - 1,
            TOOLBAR_HEIGHT + 1,
        )?;

        // ラベル "Address:" を左上に表示（ツールバーの 5,5 あたり）
        self.window.draw_string(
            BLACK,
            5,
            5,
            "Address:",
            StringSize::Medium,
            /*underline=*/ false,
        )?;

        // アドレスバーの“箱”（白い長方形）を描画
        // x=70px から右端までを使う。右に 4px の余白を残すため、幅は (WINDOW_WIDTH - 74) にしています。
        // y=2..(2+高さ) の帯を置く（TOOLBAR_HEIGHT より少し小さい高さ）。
        self.window
            .fill_rect(WHITE, 70, 2, WINDOW_WIDTH - 74, 2 + ADDRESSBAR_HEIGHT)?;

        // アドレスバーの“立体感”を出すための縁取り（薄い影 → 内側の黒線）
        // 上辺と左辺に GREY のラインで段差感を演出
        self.window.draw_line(GREY, 70, 2, WINDOW_WIDTH - 4, 2)?;
        self.window
            .draw_line(GREY, 70, 2, 70, 2 + ADDRESSBAR_HEIGHT)?;
        // その内側 1px に黒いライン（フチ取り）
        self.window.draw_line(BLACK, 71, 3, WINDOW_WIDTH - 5, 3)?;

        self.window
            .draw_line(GREY, 71, 3, 71, 1 + ADDRESSBAR_HEIGHT)?;

        Ok(())
    }

    /// アドレスバーを「入力文字列で更新」して、その範囲だけ画面反映（部分フラッシュ）する
    ///
    /// 初心者向けポイント（TS/Python にたとえて）
    /// - `Result<(), Error>` は「成功 or 失敗理由」を返す型（TSの`Result`/Pythonの`try/except`の結果）。
    ///   `?` 演算子で早期リターンする代わりに、ここでは `is_err()` で明示チェックして独自エラーに変換しています。
    /// - 描画 API は「バッファに描く → `flush`/`flush_area` で実画面へ反映」という二段階。
    /// - 座標は左上が `(0,0)`。`x` 右方向、`y` 下方向へ増えます。
    ///
    /// 何をしているか
    /// 1) 既存の文字を消すため、アドレスバーの内側だけ白で塗りつぶす（枠線は残すため数ピクセル内側を指定）。
    /// 2) 入力中の URL 文字列（`self.input_url`）を黒文字で描く。
    /// 3) ツールバー領域だけ `flush_area` で画面に反映（全画面ではなく部分更新）。
    fn update_address_bar(&mut self) -> Result<(), Error> {
        // 1) アドレスバーの内側を白でクリア
        //    `fill_rect(色, x, y, 幅, 高さ)`
        //    アドレスバー外枠（70,2 起点）の内側に 1〜2px だけ縮めて塗ることで、
        //    先に描いた枠線（影・フチ取り）を上書きしないようにしています。
        if self
            .window
            .fill_rect(WHITE, 72, 4, WINDOW_WIDTH - 76, ADDRESSBAR_HEIGHT - 2)
            .is_err()
        {
            return Err(Error::InvalidUI(
                "failed to clear an address bar".to_string(),
            ));
        }

        // 2) 入力中の文字列を描画（まだ画面には出ない＝バッファに描いているだけ）
        //    `draw_string(色, x, y, 文字列, サイズ, 下線の有無)`
        //    先頭文字の描画位置を (74,6) あたりに置き、`Medium` サイズで下線なし。
        if self
            .window
            .draw_string(
                BLACK,
                74,
                6,
                &self.input_url,
                StringSize::Medium,
                /*underline=*/ false,
            )
            .is_err()
        {
            return Err(Error::InvalidUI(
                "failed to update an address bar".to_string(),
            ));
        }

        // 3) ツールバー帯（アドレスバーを含む）だけを部分フラッシュ
        //    ここで初めて GUI に描画が現れます。全体 `flush()` よりも無駄が少なく、
        //    スクロールや入力のたびの再描画を軽くできます。
        self.window.flush_area(
            Rect::new(
                WINDOW_INIT_X_POS,
                WINDOW_INIT_Y_POS + TITLE_BAR_HEIGHT,
                WINDOW_WIDTH,
                TOOLBAR_HEIGHT,
            )
            .expect("failed to create a rect for the address bar"),
        );

        Ok(())
    }

    /// アドレスバーのテキストだけを消して、枠は残したまま部分フラッシュする
    ///
    /// 用途
    /// - 入力開始時に真っさらな状態へ戻す、エラー後に文字を消す、など。
    ///
    /// 実装のポイント
    /// - 「外枠を崩さない」ため、塗りつぶす領域は外枠より 1〜2px 内側を指定します。
    /// - 反映は `flush_area` でツールバー帯のみ。高速でチラつきにくいです。
    fn clear_address_bar(&mut self) -> Result<(), Error> {
        // 文字部分だけを白でクリア（枠線は残す）
        if self
            .window
            .fill_rect(WHITE, 72, 4, WINDOW_WIDTH - 76, ADDRESSBAR_HEIGHT - 2)
            .is_err()
        {
            return Err(Error::InvalidUI(
                "failed to clear an address bar".to_string(),
            ));
        }

        // ツールバー帯の範囲を部分フラッシュ
        self.window.flush_area(
            Rect::new(
                WINDOW_INIT_X_POS,
                WINDOW_INIT_Y_POS + TITLE_BAR_HEIGHT,
                WINDOW_WIDTH,
                TOOLBAR_HEIGHT,
            )
            .expect("failed to create a rect for the address bar"),
        );

        Ok(())
    }

    fn clear_content_area(&mut self) -> Result<(), Error> {
        // コンテンツエリアを白く塗りつぶす
        if self
            .window
            .fill_rect(
                WHITE,
                0,
                TOOLBAR_HEIGHT + 2,
                CONTENT_AREA_WIDTH,
                CONTENT_AREA_HEIGHT - 2,
            )
            .is_err()
        {
            return Err(Error::InvalidUI(
                "failed to clear a content area".to_string(),
            ));
        }

        self.window.flush();

        Ok(())
    }
}

/// レイアウト層のフォントサイズ（`FontSize`）を、描画ライブラリのサイズ（`StringSize`）へ変換する
///
/// 背景
/// - `FontSize` はレンダラー側（`saba_core`）の論理サイズ。CSS の `font-size` のような概念。
/// - `StringSize` は描画 API（`noli::window`）がサポートする実サイズの列挙。
///   名称と段階が完全一致しないため、最も近い段階へ丸めます。
///
/// 変換ルール（現在の対応）
/// - `Medium`  → `Medium`
/// - `XLarge`  → `Large`   （名前は異なるが実寸が近い）
/// - `XXLarge` → `XLarge`
///
/// メモ
/// - 列挙型（Rust の enum ≈ TS の union 型）なので、将来 `FontSize` に新しい値が増えたら、
///   ここでどの `StringSize` に落とすか決める必要があります（未対応だとコンパイルエラーで気づけます）。
///
/// 使用例
/// ```rust
/// let s = convert_font_size(FontSize::XLarge); // => StringSize::Large
/// ```
fn convert_font_size(size: FontSize) -> StringSize {
    match size {
        FontSize::Medium => StringSize::Medium,
        FontSize::XLarge => StringSize::Large,
        FontSize::XXLarge => StringSize::XLarge,
    }
}
