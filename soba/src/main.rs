//! エントリポイントとなる最小のサンプルアプリ
//!
//! このプログラムは標準ライブラリを使わない `no_std` 環境で動き、
//! WasabiOS（独自OS）の API を通じて文字列出力やプロセス終了を行います。

#![no_std]
// Linux 以外（= WasabiOS ターゲットなど）では標準の `main` 初期化を無効化する。
// `no_main` にすることで、独自のエントリポイント（下の `entry_point!` マクロ）を使う。
#![cfg_attr(not(target_os = "linux"), no_main)]

// WasabiOS 向けのユーティリティとプリミティブをまとめた `noli` の便利セット。
// `Api` や `println!` などが使えるようになります。
use noli::prelude::*;

// アプリの本体。通常の `fn main()` と同じ形で書けます。
fn main() {
    // 低レベル API を使って直接文字列を出力する例。
    Api::write_string("Hello World\n");

    // `println!` マクロも利用可能（内部で WasabiOS の出力に流れます）。
    println!("Hello from println!");

    // 終了コード 42 を返してプロセスを終了。
    // QEMU 実行時はこのコードで終了を検知できる場合があります。
    Api::exit(42);
}

// `no_main` 環境で `main` 関数をエントリポイントとして登録するマクロ。
// これにより、OS からこの関数が最初に呼ばれるようになります。
entry_point!(main);
