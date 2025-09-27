use crate::error::Error;
use crate::renderer::dom::node::ElementKind;
use crate::renderer::dom::node::Node;
use crate::renderer::dom::node::NodeKind;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::string::ToString;
use core::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    background_color: Option<Color>,
    color: Option<Color>,
    display: Option<DisplayType>,
    font_size: Option<FontSize>,
    text_decoration: Option<TextDecoration>,
    height: Option<f64>,
    width: Option<f64>,
}

impl ComputedStyle {
    pub fn new() -> Self {
        Self {
            background_color: None,
            color: None,
            display: None,
            font_size: None,
            text_decoration: None,
            height: None,
            width: None,
        }
    }

    pub fn defaulting(&mut self, node: &Rc<RefCell<Node>>, parent_style: Option<ComputedStyle>) {
        // もし親ノードが存在し、親のCSSの値が初期値とは異なる場合、値を継承する
        if let Some(parent_style) = parent_style {
            if self.background_color.is_none() && parent_style.background_color() != Color::white()
            {
                self.background_color = Some(parent_style.background_color());
            }
            if self.color.is_none() && parent_style.color() != Color::black() {
                self.color = Some(parent_style.color());
            }
            if self.font_size.is_none() && parent_style.font_size() != FontSize::Medium {
                self.font_size = Some(parent_style.font_size());
            }
            if self.text_decoration.is_none()
                && parent_style.text_decoration() != TextDecoration::None
            {
                self.text_decoration = Some(parent_style.text_decoration());
            }
        }

        // 各プロパティに対して、初期値を設定する
        if self.background_color.is_none() {
            self.background_color = Some(Color::white());
        }
        if self.color.is_none() {
            self.color = Some(Color::black());
        }
        if self.display.is_none() {
            self.display = Some(DisplayType::default(node));
        }
        if self.font_size.is_none() {
            self.font_size = Some(FontSize::default(node));
        }
        if self.text_decoration.is_none() {
            self.text_decoration = Some(TextDecoration::default(node));
        }
        if self.height.is_none() {
            self.height = Some(0.0);
        }
        if self.width.is_none() {
            self.width = Some(0.0);
        }
    }

    pub fn set_background_color(&mut self, color: Color) {
        self.background_color = Some(color);
    }

    pub fn background_color(&self) -> Color {
        self.background_color
            .clone()
            .expect("failed to access CSS property: background_color")
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = Some(color);
    }

    pub fn color(&self) -> Color {
        self.color
            .clone()
            .expect("failed to access CSS property: color")
    }

    pub fn set_display(&mut self, display: DisplayType) {
        self.display = Some(display);
    }

    pub fn display(&self) -> DisplayType {
        self.display
            .expect("failed to access CSS property: display")
    }

    pub fn font_size(&self) -> FontSize {
        self.font_size
            .expect("failed to access CSS property: font_size")
    }

    pub fn text_decoration(&self) -> TextDecoration {
        self.text_decoration
            .expect("failed to access CSS property: text_decoration")
    }

    pub fn set_height(&mut self, height: f64) {
        self.height = Some(height);
    }

    pub fn height(&self) -> f64 {
        self.height.expect("failed to access CSS property: height")
    }

    pub fn set_width(&mut self, width: f64) {
        self.width = Some(width);
    }

    pub fn width(&self) -> f64 {
        self.width.expect("failed to access CSS property: width")
    }
}

// CSS の色（color）を表す最小構造体
//
// 役割（実ブラウザのどの部分？）
// - CSS の color/background-color などで使う色を、名前（例: "red"）と 6 桁の #RRGGBB で保持します。
// - 実装をシンプルにするため、対応色は固定のサンプルのみ。未対応は Error を返します。
//
// 言語ブリッジ（TS / Python / Go）
// - `Option<String>` は “ある/ない” を型で表す（TS: string | undefined, Python: Optional[str]）。
// - 16進 → 数値変換（code_u32）は `int('RRGGBB', 16)` のイメージ。
#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    name: Option<String>, // 例: Some("red")。コードから生成した場合でも逆引きで Some(...) を入れる
    code: String,         // 例: "#ff0000"（常に # + 6桁の小文字16進）
}

impl Color {
    // 色名から Color を作る（固定の代表色のみ対応）
    // 例: Color::from_name("red") → Ok(Color { name: Some("red"), code: "#ff0000" })
    pub fn from_name(name: &str) -> Result<Self, Error> {
        let code = match name {
            "black" => "#000000".to_string(),
            "silver" => "#c0c0c0".to_string(),
            "gray" => "#808080".to_string(),
            "white" => "#ffffff".to_string(),
            "maroon" => "#800000".to_string(),
            "red" => "#ff0000".to_string(),
            "purple" => "#800080".to_string(),
            "fuchsia" => "#ff00ff".to_string(),
            "green" => "#008000".to_string(),
            "lime" => "#00ff00".to_string(),
            "olive" => "#808000".to_string(),
            "yellow" => "#ffff00".to_string(),
            "navy" => "#000080".to_string(),
            "blue" => "#0000ff".to_string(),
            "teal" => "#008080".to_string(),
            "aqua" => "#00ffff".to_string(),
            "orange" => "#ffa500".to_string(),
            "lightgray" => "#d3d3d3".to_string(),
            _ => {
                return Err(Error::UnexpectedInput(format!(
                    "color name {:?} is not supported yet",
                    name
                )));
            }
        };

        Ok(Self {
            name: Some(name.to_string()),
            code,
        })
    }

    // #RRGGBB から Color を作る（6桁固定の簡易版）
    // 例: Color::from_code("#00ff00") → Ok(Color { name: Some("lime"), code: "#00ff00" })
    // 注意: #付き7文字のみを許可。短縮形 #0f0 や alpha #RRGGBBAA は未対応。
    pub fn from_code(code: &str) -> Result<Self, Error> {
        if code.chars().nth(0) != Some('#') || code.len() != 7 {
            return Err(Error::UnexpectedInput(format!(
                "invalid color code {}",
                code
            )));
        }

        let name = match code {
            "#000000" => "black".to_string(),
            "#c0c0c0" => "silver".to_string(),
            "#808080" => "gray".to_string(),
            "#ffffff" => "white".to_string(),
            "#800000" => "maroon".to_string(),
            "#ff0000" => "red".to_string(),
            "#800080" => "purple".to_string(),
            "#ff00ff" => "fuchsia".to_string(),
            "#008000" => "green".to_string(),
            "#00ff00" => "lime".to_string(),
            "#808000" => "olive".to_string(),
            "#ffff00" => "yellow".to_string(),
            "#000080" => "navy".to_string(),
            "#0000ff" => "blue".to_string(),
            "#008080" => "teal".to_string(),
            "#00ffff" => "aqua".to_string(),
            "#ffa500" => "orange".to_string(),
            "#d3d3d3" => "lightgray".to_string(),
            _ => {
                return Err(Error::UnexpectedInput(format!(
                    "color code {:?} is not supported yet",
                    code
                )));
            }
        };

        Ok(Self {
            name: Some(name),
            code: code.to_string(),
        })
    }

    // 定数ショートカット（便利メソッド）: 白/黒
    pub fn white() -> Self {
        Self {
            name: Some("white".to_string()),
            code: "#ffffff".to_string(),
        }
    }

    pub fn black() -> Self {
        Self {
            name: Some("black".to_string()),
            code: "#000000".to_string(),
        }
    }

    // #RRGGBB を 0xRRGGBB の数値（u32）に変換する
    // 例: "#00ff00" → 0x00ff00（= 65280）
    // 注意: unwrap() はサンプル向け（固定 6 桁を前提）。実運用ではエラーハンドリング推奨。
    pub fn code_u32(&self) -> u32 {
        u32::from_str_radix(self.code.trim_start_matches('#'), 16).unwrap()
    }
}

// 文字の大きさ（font-size）の簡易列挙
//
// - CSS の絶対サイズ（absolute-size）に対応するサンプル：medium / x-large / xx-large。
// - 実ブラウザは相対指定（em/rem/%）や継承、ユーザー設定など多くの要素を考慮しますが、
//   ここでは見出しタグに応じた“わかりやすい差”だけを表現します。
// 仕様: https://www.w3.org/TR/css-fonts-4/#absolute-size-mapping
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FontSize {
    Medium,
    XLarge,
    XXLarge,
}

impl FontSize {
    // 要素種別に応じた既定フォントサイズを返す（学習用の最小ルール）
    // - <h1> → XXLarge, <h2> → XLarge, その他 → Medium
    // - これは“ブラウザのスタイルシート（UA スタイル）”の超簡易版と考えてください。
    fn default(node: &Rc<RefCell<Node>>) -> Self {
        match &node.borrow().kind() {
            NodeKind::Element(element) => match element.kind() {
                ElementKind::H1 => FontSize::XXLarge,
                ElementKind::H2 => FontSize::XLarge,
                _ => FontSize::Medium,
            },
            _ => FontSize::Medium,
        }
    }
}

// CSS の display プロパティ（要素の“並び方”）に対応する値
//
// - 最小実装として `block` / `inline` / `none` の3種類のみを扱います。
// - 実ブラウザには他にも `inline-block` / `flex` / `grid` など多数ありますが、
//   レイアウトの基本理解に必要なコアだけに絞っています。
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DisplayType {
    /// https://www.w3.org/TR/css-display-3/#valdef-display-block
    Block,
    /// https://www.w3.org/TR/css-display-3/#valdef-display-inline
    Inline,
    /// https://www.w3.org/TR/css-display-3/#valdef-display-none
    DisplayNone,
}

impl DisplayType {
    // 要素の“既定表示形式（UA スタイルシートの超簡易版）”を返す
    // ルール（学習用）
    // - Document はブロック
    // - Element: ブロック要素なら Block、そうでなければ Inline（`is_block_element()` に依存）
    // - Text はインライン（テキストは行内に流れる）
    fn default(node: &Rc<RefCell<Node>>) -> Self {
        match &node.borrow().kind() {
            NodeKind::Document => DisplayType::Block,
            NodeKind::Element(e) => {
                if e.is_block_element() {
                    DisplayType::Block
                } else {
                    DisplayType::Inline
                }
            }
            NodeKind::Text(_) => DisplayType::Inline,
        }
    }

    // 文字列 → DisplayType への変換
    // 入力例: "block" / "inline" / "none"
    // 未対応のキーワードは Error を返して早期発見（将来拡張時に追加）。
    pub fn from_str(s: &str) -> Result<Self, Error> {
        match s {
            "block" => Ok(Self::Block),
            "inline" => Ok(Self::Inline),
            "none" => Ok(Self::DisplayNone),
            _ => Err(Error::UnexpectedInput(format!(
                "display {:?} is not supported yet",
                s
            ))),
        }
    }
}

// CSS の text-decoration プロパティ（下線など）に対する最小の列挙
//
// - 学習用に `none` と `underline` のみをサポートしています。
// - 実ブラウザには `overline` / `line-through` / `blink`（非推奨）などもあります。
//   必要になったらここに列挙子を追加していきます。
// 仕様: https://w3c.github.io/csswg-drafts/css-text-decor/#text-decoration-property
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TextDecoration {
    None,
    Underline,
}

impl TextDecoration {
    // 既定の装飾（UA スタイルの超簡易版）を返す
    // ルール（学習用）
    // - アンカータグ <a> は下線（Underline）
    // - それ以外は None
    fn default(node: &Rc<RefCell<Node>>) -> Self {
        match &node.borrow().kind() {
            NodeKind::Element(element) => match element.kind() {
                ElementKind::A => TextDecoration::Underline,
                _ => TextDecoration::None,
            },
            _ => TextDecoration::None,
        }
    }
}
