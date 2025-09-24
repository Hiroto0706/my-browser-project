//! HTML トークナイザ（初心者向け解説つき）
//!
//! これは HTML の文字列を「トークン（部品）」に分割する最小実装です。
//! 文字を1つずつ読み進め、状態(State)に応じて `StartTag` / `EndTag` / `Char` / `Eof` を返します。
//!
//! サンプル（入力 → トークン列）
//! ```text
//! 入力: <div class="a">hi</div>
//! 出力: StartTag { tag: "div", attributes: [("class","a")], self_closing: false }
//!         Char('h')
//!         Char('i')
//!         EndTag { tag: "div" }
//! ```
//!
//! もう少し詳しい流れ（divの開始タグ部分）
//! ```text
//! 1) '<' 読む → state=TagOpen
//! 2) 'd' 読む（英字）→ reconsume=true, state=TagName, create_tag(Start)
//! 3) TagName で 'd','i','v' を順に append_tag_name
//! 4) 空白 ' ' → state=BeforeAttributeName
//! 5) 'c' を再読（reconsume）させて state=AttributeName、属性を開始 start_new_attribute
//! 6) AttributeName: 'c','l','a','s','s' を追加 → '=' で BeforeAttributeValue
//! 7) '"' → AttributeValueDoubleQuoted、'a' を読み、'"' で閉じる
//! 8) '>' → take_latest_token() で StartTag を返す
//! ```
//!
//! コード例（イテレータとして使う）
//! ```ignore
//! use saba_core::renderer::html::token::{HtmlTokenizer, HtmlToken};
//! let mut it = HtmlTokenizer::new("<br/>".to_string());
//! assert!(matches!(it.next(), Some(HtmlToken::StartTag{ tag, self_closing: true, .. }) if tag == "br"));
//! assert!(it.next().is_none());
//! ```
//!
//! reconsume の意図
//! - 「読み取り位置は進めたが、その文字を次の状態でもう一度処理したい」ときに使います。
//!   読み過ぎを戻すのではなく、“もう一度使う”という設計にすると実装が単純になります。
//!
//! 設計メモ
//! - 仕様準拠というより“状態機械”の学習が目的の最小版です。
//! - エスケープ・エンティティ・コメント等は省略。必要になったら `State` と分岐を追加して拡張できます。
//!
//! 用語の橋渡し（TS / Python / Go）
//! - イテレータ: `impl Iterator for HtmlTokenizer` により `next()` で 1 トークンずつ取得。
//!   - TS: `for (const t of tokenizer) { ... }`、Python: `for t in tokenizer:`、Go: `for { t, ok := it.Next() }` 的な感覚。
//! - 再読（reconsume）: 1 文字読み過ぎたときに「もう一度同じ文字を処理する」ためのフラグです。
//! - 状態機械: 仕様（WHATWG）の各ステートを Rust の `enum State` で表し、`match` で分岐します。
//!
//! 注意
//! - 学習用の簡易実装であり、仕様のサブセットです。スクリプトデータ周りなどは最小限です。
//! - `Eof` トークンは内部の一部分岐でのみ生成されます。反復終了は `Iterator::next()` が `None` を返すことでも表されます。

use crate::renderer::html::attribute::Attribute;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlToken {
    // 開始タグ（例: <div class="a">）
    StartTag {
        tag: String,
        self_closing: bool,
        attributes: Vec<Attribute>,
    },
    // 終了タグ（例: </div>）
    EndTag {
        tag: String,
    },
    // テキストノードの 1 文字（例: 'h'）
    Char(char),
    // ファイルの終了（End Of File）。Iterator の `None` と併用されます。
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// https://html.spec.whatwg.org/multipage/parsing.html#data-state
    Data,
    /// https://html.spec.whatwg.org/multipage/parsing.html#tag-open-state
    TagOpen,
    /// https://html.spec.whatwg.org/multipage/parsing.html#end-tag-open-state
    EndTagOpen,
    /// https://html.spec.whatwg.org/multipage/parsing.html#tag-name-state
    TagName,
    /// https://html.spec.whatwg.org/multipage/parsing.html#before-attribute-name-state
    BeforeAttributeName,
    /// https://html.spec.whatwg.org/multipage/parsing.html#attribute-name-state
    AttributeName,
    /// https://html.spec.whatwg.org/multipage/parsing.html#after-attribute-name-state
    AfterAttributeName,
    /// https://html.spec.whatwg.org/multipage/parsing.html#before-attribute-value-state
    BeforeAttributeValue,
    /// https://html.spec.whatwg.org/multipage/parsing.html#attribute-value-(double-quoted)-state
    AttributeValueDoubleQuoted,
    /// https://html.spec.whatwg.org/multipage/parsing.html#attribute-value-(single-quoted)-state
    AttributeValueSingleQuoted,
    /// https://html.spec.whatwg.org/multipage/parsing.html#attribute-value-(unquoted)-state
    AttributeValueUnquoted,
    /// https://html.spec.whatwg.org/multipage/parsing.html#after-attribute-value-(quoted)-state
    AfterAttributeValueQuoted,
    /// https://html.spec.whatwg.org/multipage/parsing.html#self-closing-start-tag-state
    SelfClosingStartTag,
    /// https://html.spec.whatwg.org/multipage/parsing.html#script-data-state
    ScriptData,
    /// https://html.spec.whatwg.org/multipage/parsing.html#script-data-less-than-sign-state
    ScriptDataLessThanSign,
    /// https://html.spec.whatwg.org/multipage/parsing.html#script-data-end-tag-open-state
    ScriptDataEndTagOpen,
    /// https://html.spec.whatwg.org/multipage/parsing.html#script-data-end-tag-name-state
    ScriptDataEndTagName,
    /// https://html.spec.whatwg.org/multipage/parsing.html#temporary-buffer
    TemporaryBuffer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlTokenizer {
    state: State,                    // 現在の状態（状態機械）
    pos: usize,                      // 次に読む入力位置（0..len）。`next()`ごとに進みます。
    reconsume: bool,                 // 直前に読んだ文字を“もう一度この状態で”処理したいとき true
    latest_token: Option<HtmlToken>, // 生成途中のタグトークンを保持
    input: Vec<char>,                // 入力全体を char ベクタ化したもの
    buf: String,                     // スクリプトデータ等で一時的に使うバッファ
}

impl HtmlTokenizer {
    /// 文字列からトークナイザを作成します。
    ///
    /// - 内部では `html.chars().collect()` でいったん `Vec<char>` を作ります。
    ///   実装をシンプルにする代わりに、UTF-8 の 1 文字ずつに事前分割します（速度より分かりやすさ優先）。
    /// - 初期状態は `State::Data`（タグの外のテキスト処理）。
    ///
    /// 例:
    /// ```ignore
    /// let mut it = HtmlTokenizer::new("<p>hi</p>".to_string());
    /// assert!(it.next().is_some()); // StartTag("p")
    /// ```
    pub fn new(html: String) -> Self {
        Self {
            state: State::Data,
            pos: 0,
            reconsume: false,
            latest_token: None,
            input: html.chars().collect(),
            buf: String::new(),
        }
    }

    /// 入力の終端かどうか判定します。
    ///
    /// - `pos > input.len()` のとき true。
    /// - 通常は `next()` の外側で境界管理しているため、ここは“安全確認”的な用途です。
    fn is_eof(&self) -> bool {
        self.pos > self.input.len()
    }

    /// 直前の1文字を“もう一度”返します（`reconsume = true` のとき専用）。
    ///
    /// - この関数は `pos` を進めません（読み取り位置は据え置き）。
    /// - 直前に `consume_next_input()` を呼んでいることが前提です（そうでないと `pos-1` が underflow）。
    /// - 呼び出し後は `reconsume` を自動で `false` に戻します。
    fn reconsume_input(&mut self) -> char {
        self.reconsume = false;
        self.input[self.pos - 1]
    }

    /// 次の1文字を返し、`pos` を 1 進めます。
    ///
    /// - 事前に `pos < input.len()` が成り立っている必要があります（`next()` が保証）。
    /// - UTF-8 の1文字単位で進みます（`.chars()` 済みのため）。
    fn consume_next_input(&mut self) -> char {
        let c = self.input[self.pos];
        self.pos += 1;
        c
    }

    /// StartTag/EndTag の生成を開始し、`latest_token` に仮置きします。
    ///
    /// - `start_tag_token=true` なら空の StartTag を、false なら空の EndTag を作ります。
    /// - タグ名は空文字で開始し、`append_tag_name` で1文字ずつ積みます。
    fn create_tag(&mut self, start_tag_token: bool) {
        if start_tag_token {
            self.latest_token = Some(HtmlToken::StartTag {
                tag: String::new(),
                self_closing: false,
                attributes: Vec::new(),
            });
        } else {
            self.latest_token = Some(HtmlToken::EndTag { tag: String::new() })
        }
    }

    /// 生成中のタグ名に1文字追加します。
    ///
    /// - 事前条件: `latest_token` が `Some(StartTag|EndTag)` であること。
    /// - 大文字→小文字化は呼び出し元（状態機械側）で行います。
    /// - 不変条件が崩れた場合は `assert!` / `panic!` で早期に気づけるようにしています（学習用）。
    fn append_tag_name(&mut self, c: char) {
        assert!(self.latest_token.is_some());

        if let Some(t) = self.latest_token.as_mut() {
            match t {
                HtmlToken::StartTag {
                    ref mut tag,
                    self_closing: _,
                    attributes: _,
                }
                | HtmlToken::EndTag { ref mut tag } => tag.push(c),
                _ => panic!("`latest_token` should be either StartTag or EndTag"),
            }
        }
    }

    /// 生成中の `latest_token` を取り出して返し、内部は `None` に戻します。
    ///
    /// - 事前条件: `latest_token.is_some()`。
    /// - 返り値: 完成したタグトークン（`StartTag` または `EndTag`）。
    fn take_latest_token(&mut self) -> Option<HtmlToken> {
        assert!(self.latest_token.is_some());

        let t = self.latest_token.as_ref().cloned();
        self.latest_token = None;
        assert!(self.latest_token.is_none());

        t
    }

    /// 新しい属性を開始します（`StartTag.attributes` に空の Attribute を push）。
    ///
    /// - 事前条件: `latest_token` が `Some(StartTag)`。
    /// - 値や名前の実体は `append_attribute` で 1 文字ずつ追加します。
    fn start_new_attribute(&mut self) {
        assert!(self.latest_token.is_some());

        if let Some(t) = self.latest_token.as_mut() {
            match t {
                HtmlToken::StartTag {
                    tag: _,
                    self_closing: _,
                    ref mut attributes,
                } => {
                    attributes.push(Attribute::new());
                }
                _ => panic!("`latest_token` should be either StartTag"),
            }
        }
    }

    /// 現在の属性の name/value のどちらかに 1 文字を追加します。
    ///
    /// - `is_name=true` で属性名、`false` で属性値に追加。
    /// - 事前条件: `attributes.len() > 0`（直前に `start_new_attribute()` を呼んでいる）。
    fn append_attribute(&mut self, c: char, is_name: bool) {
        assert!(self.latest_token.is_some());

        if let Some(t) = self.latest_token.as_mut() {
            match t {
                HtmlToken::StartTag {
                    tag: _,
                    self_closing: _,
                    ref mut attributes,
                } => {
                    let len = attributes.len();
                    assert!(len > 0);
                    attributes[len - 1].add_char(c, is_name)
                }
                _ => panic!("`latest_token` should be either StartTag"),
            }
        }
    }

    /// 現在の StartTag を自己終了（`<br/>` など）としてマークします。
    ///
    /// - 事前条件: `latest_token` が `Some(StartTag)`。
    /// - これにより返されるトークンの `self_closing` が `true` になります。
    fn set_self_closing_flag(&mut self) {
        assert!(self.latest_token.is_some());

        if let Some(t) = self.latest_token.as_mut() {
            match t {
                HtmlToken::StartTag {
                    tag: _,
                    ref mut self_closing,
                    attributes: _,
                } => *self_closing = true,
                _ => panic!("`latest_token` should be either StartTag"),
            }
        }
    }
}

impl Iterator for HtmlTokenizer {
    type Item = HtmlToken;

    fn next(&mut self) -> Option<Self::Item> {
        // 反復が終わったら None（イテレータの終了）を返します。
        // この関数は「最大で1つのトークン」を返すのがルールです。
        // - Some(token) を返したら一旦呼び出し側へ制御を返します（次の next() で続き）。
        // - まだトークンが確定しない場合は `continue` で次の文字へ進みます。
        if self.pos >= self.input.len() {
            return None;
        }

        loop {
            // reconsume が true のときは直前文字を「同じ位置のまま」もう一度処理します。
            // 読み過ぎを戻すのではなく、“この文字を次の状態で使い直す”という設計です。
            let c = match self.reconsume {
                true => self.reconsume_input(),
                false => self.consume_next_input(),
            };

            match self.state {
                State::Data => {
                    // 通常のテキスト（タグ外）を処理する状態。
                    if c == '<' {
                        self.state = State::TagOpen;
                        continue; // まだトークンは返さず、次の文字でタグかどうかを判断
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    return Some(HtmlToken::Char(c)); // ここで1文字ぶんのトークンを確定して返す
                }

                State::TagOpen => {
                    // `<` の直後に来る文字で分岐：`/` なら終了タグ、英字なら開始タグ。
                    if c == '/' {
                        self.state = State::EndTagOpen;
                        continue;
                    }

                    if c.is_ascii_alphabetic() {
                        self.reconsume = true; // 同じ文字を TagName で一文字目として処理
                        self.state = State::TagName;
                        self.create_tag(true);
                        continue;
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    self.reconsume = true; // データとして扱い直す
                    self.state = State::Data;
                }

                State::EndTagOpen => {
                    // `</` の直後：タグ名開始
                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    if c.is_ascii_alphabetic() {
                        self.reconsume = true;
                        self.state = State::TagName;
                        self.create_tag(false);
                        continue;
                    }
                }

                State::TagName => {
                    // タグ名を読み取る。
                    // - 空白で属性（BeforeAttributeName）
                    // - '/' で自己終了（SelfClosingStartTag）
                    // - '>' でタグ確定して Start/EndTag を返す
                    if c == ' ' {
                        self.state = State::BeforeAttributeName;
                        continue;
                    }

                    if c == '/' {
                        self.state = State::SelfClosingStartTag;
                        continue;
                    }

                    if c == '>' {
                        self.state = State::Data;
                        return self.take_latest_token(); // latest_token を取り出して None に戻す
                    }

                    if c.is_ascii_uppercase() {
                        self.append_tag_name(c.to_ascii_lowercase());
                        continue;
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    self.append_tag_name(c); // それ以外はタグ名の続きとして追加
                }

                State::BeforeAttributeName => {
                    // 次の属性開始の前。空白をスキップし、属性名の最初の文字を reconsume して AttributeName へ。
                    if c == '/' || c == '>' || self.is_eof() {
                        self.reconsume = true;
                        self.state = State::AfterAttributeName;
                        continue;
                    }

                    self.reconsume = true;
                    self.state = State::AttributeName;
                    self.start_new_attribute(); // 空の Attribute を attributes へプッシュ
                }

                State::AttributeName => {
                    // 属性名を読み取る。`=` で属性値へ、空白/`/`/`>` で属性名終了。
                    if c == ' ' || c == '/' || c == '>' || self.is_eof() {
                        self.reconsume = true;
                        self.state = State::AfterAttributeName;
                        continue;
                    }

                    if c == '=' {
                        self.state = State::BeforeAttributeValue;
                        continue;
                    }

                    if c.is_ascii_uppercase() {
                        self.append_attribute(c.to_ascii_lowercase(), /*is_name*/ true);
                        continue;
                    }

                    self.append_attribute(c, /*is_name*/ true); // name 側に1文字追加
                }

                State::AfterAttributeName => {
                    // 属性名を読み終わった直後。`=` なら値、`/` なら自己終了、`>` ならタグ完了
                    if c == ' ' {
                        // 空白文字は無視する
                        continue;
                    }

                    if c == '/' {
                        self.state = State::SelfClosingStartTag;
                        continue;
                    }

                    if c == '=' {
                        self.state = State::BeforeAttributeValue;
                        continue;
                    }

                    if c == '>' {
                        self.state = State::Data;
                        return self.take_latest_token();
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    self.reconsume = true;
                    self.state = State::AttributeName;
                    self.start_new_attribute();
                }

                State::BeforeAttributeValue => {
                    // 属性値の開始を待つ。引用符で始まれば対応するステートへ。
                    if c == ' ' {
                        // 空白文字は無視する
                        continue;
                    }

                    if c == '"' {
                        self.state = State::AttributeValueDoubleQuoted;
                        continue;
                    }

                    if c == '\'' {
                        self.state = State::AttributeValueSingleQuoted;
                        continue;
                    }

                    self.reconsume = true;
                    self.state = State::AttributeValueUnquoted;
                }

                State::AttributeValueDoubleQuoted => {
                    // ダブルクォート囲みの属性値を処理。
                    if c == '"' {
                        self.state = State::AfterAttributeValueQuoted;
                        continue;
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    self.append_attribute(c, /*is_name*/ false); // value 側に1文字追加
                }

                State::AttributeValueSingleQuoted => {
                    // シングルクォート囲みの属性値を処理。
                    if c == '\'' {
                        self.state = State::AfterAttributeValueQuoted;
                        continue;
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    self.append_attribute(c, /*is_name*/ false); // value 側に1文字追加
                }

                State::AttributeValueUnquoted => {
                    // 非引用の属性値。空白/`>`/`/` などで値を閉じる。
                    if c == ' ' {
                        self.state = State::BeforeAttributeName;
                        continue;
                    }

                    if c == '>' {
                        self.state = State::Data;
                        return self.take_latest_token();
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    self.append_attribute(c, /*is_name*/ false); // value 側に1文字追加
                }

                State::AfterAttributeValueQuoted => {
                    // 引用で閉じた直後。次の属性へ進むか、自己終了/終了を判定
                    if c == ' ' {
                        self.state = State::BeforeAttributeName;
                        continue;
                    }

                    if c == '/' {
                        self.state = State::SelfClosingStartTag;
                        continue;
                    }

                    if c == '>' {
                        self.state = State::Data;
                        return self.take_latest_token();
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    self.reconsume = true;
                    self.state = State::BeforeAttributeName;
                }

                State::SelfClosingStartTag => {
                    // `<br/>` の `/` を読んだ後。`>` で自己終了タグを確定。
                    if c == '>' {
                        self.set_self_closing_flag();
                        self.state = State::Data;
                        return self.take_latest_token();
                    }

                    if self.is_eof() {
                        // invalid parse error.
                        return Some(HtmlToken::Eof);
                    }
                }

                State::ScriptData => {
                    // `<script>` タグ内の簡易処理
                    if c == '<' {
                        self.state = State::ScriptDataLessThanSign;
                        continue;
                    }

                    if self.is_eof() {
                        return Some(HtmlToken::Eof);
                    }

                    return Some(HtmlToken::Char(c));
                }

                State::ScriptDataLessThanSign => {
                    // `<script>` 内で `<` を見たところ
                    if c == '/' {
                        // 一時的なバッファを空文字でリセットする
                        self.buf = String::new();
                        self.state = State::ScriptDataEndTagOpen;
                        continue;
                    }

                    self.reconsume = true;
                    self.state = State::ScriptData;
                    return Some(HtmlToken::Char('<'));
                }

                State::ScriptDataEndTagOpen => {
                    // `</` の後。タグ名に入るか、テキストに戻るか
                    if c.is_ascii_alphabetic() {
                        self.reconsume = true;
                        self.state = State::ScriptDataEndTagName;
                        self.create_tag(false);
                        continue;
                    }

                    self.reconsume = true;
                    self.state = State::ScriptData;
                    // 仕様では、"<"と"/"の2つの文字トークンを返すとなっているが、
                    // 私たちの実装ではnextメソッドからは一つのトークンしか返せない
                    // ため、"<"のトークンのみを返す
                    return Some(HtmlToken::Char('<'));
                }

                State::ScriptDataEndTagName => {
                    // スクリプトの終了タグ名を読み取る
                    if c == '>' {
                        self.state = State::Data;
                        return self.take_latest_token();
                    }

                    if c.is_ascii_alphabetic() {
                        self.buf.push(c);
                        self.append_tag_name(c.to_ascii_lowercase());
                        continue;
                    }

                    self.state = State::TemporaryBuffer;
                    self.buf = String::from("</") + &self.buf;
                    self.buf.push(c);
                    continue;
                }

                State::TemporaryBuffer => {
                    // 一時バッファを1文字ずつ吐き出してテキストに戻す
                    self.reconsume = true;

                    if self.buf.chars().count() == 0 {
                        self.state = State::ScriptData;
                        continue;
                    }

                    // remove the first char
                    let c = self
                        .buf
                        .chars()
                        .nth(0)
                        .expect("self.buf should have at least 1 char");
                    self.buf.remove(0);
                    return Some(HtmlToken::Char(c));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn test_empty() {
        let html = "".to_string();
        let mut tokenizer = HtmlTokenizer::new(html);
        assert!(tokenizer.next().is_none())
    }

    #[test]
    fn test_start_and_end_tag() {
        let html = "<body></body>".to_string();
        let mut tokenizer = HtmlTokenizer::new(html);
        let expected = [
            HtmlToken::StartTag {
                tag: "body".to_string(),
                self_closing: false,
                attributes: Vec::new(),
            },
            HtmlToken::EndTag {
                tag: "body".to_string(),
            },
        ];

        for e in expected {
            assert_eq!(Some(e), tokenizer.next())
        }
    }

    #[test]
    fn test_attributes() {
        let html = "<p class=\"A\" id='B' foo=bar></p>".to_string();
        let mut tokenizer = HtmlTokenizer::new(html);
        let mut attr1 = Attribute::new();
        attr1.add_char('c', true);
        attr1.add_char('l', true);
        attr1.add_char('a', true);
        attr1.add_char('s', true);
        attr1.add_char('s', true);
        attr1.add_char('A', false);

        let mut attr2 = Attribute::new();
        attr2.add_char('i', true);
        attr2.add_char('d', true);
        attr2.add_char('B', false);

        let mut attr3 = Attribute::new();
        attr3.add_char('f', true);
        attr3.add_char('o', true);
        attr3.add_char('o', true);
        attr3.add_char('b', false);
        attr3.add_char('a', false);
        attr3.add_char('r', false);

        let expected = [
            HtmlToken::StartTag {
                tag: "p".to_string(),
                self_closing: false,
                attributes: vec![attr1, attr2, attr3],
            },
            HtmlToken::EndTag {
                tag: "p".to_string(),
            },
        ];

        for e in expected {
            assert_eq!(Some(e), tokenizer.next());
        }
    }

    #[test]
    fn test_self_closing_tag() {
        let html = "<img />".to_string();
        let mut tokenizer = HtmlTokenizer::new(html);
        let expected = [HtmlToken::StartTag {
            tag: "img".to_string(),
            self_closing: true,
            attributes: Vec::new(),
        }];

        for e in expected {
            assert_eq!(Some(e), tokenizer.next())
        }
    }

    #[test]
    fn test_script_tag() {
        let html = "<script>js code;</script>".to_string();
        let mut tokenizer = HtmlTokenizer::new(html);
        let expected = [
            HtmlToken::StartTag {
                tag: "script".to_string(),
                self_closing: false,
                attributes: Vec::new(),
            },
            HtmlToken::Char('j'),
            HtmlToken::Char('s'),
            HtmlToken::Char(' '),
            HtmlToken::Char('c'),
            HtmlToken::Char('o'),
            HtmlToken::Char('d'),
            HtmlToken::Char('e'),
            HtmlToken::Char(';'),
            HtmlToken::EndTag {
                tag: "script".to_string(),
            },
        ];

        for e in expected {
            assert_eq!(Some(e), tokenizer.next())
        }
    }
}
