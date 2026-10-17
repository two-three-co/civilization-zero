# ADR-0009: simulation-core を WASM にコンパイルしてブラウザ内で実行する

- 状態: 採用（Toy 1 の実行基盤。ADR-0010 の前提）
- 計測日: 2026-10-17 · commit ace3ef0 以降のカーネル · Apple M1 · アプリ内蔵ブラウザ（Chromium 系）

## 問い

1. 依存クレートは wasm32 でビルドできるか
2. 決定論は native と wasm の間で保たれるか
3. 性能は Toy 1 の目標（1,000〜2,000 体で ≥ 60 tick/s）に足りるか
4. サイズ・メモリ・スナップショットの扱いは現実的か

## 試したこと

`crates/cz-wasm-probe`: wasm-bindgen を使わず `extern "C"` 5 関数（`probe_init/step/tick/hash/snapshot_len`）だけを公開する cdylib。
`viewer/wasm-probe.html` が `WebAssembly.instantiate` で読み込み、`performance.now()` で計測する。
ビルド: `cargo build -p cz-wasm-probe --target wasm32-unknown-unknown --profile wasm`（profile は `Cargo.toml`: lto, strip, codegen-units=1）。

## 結果

**1. 依存** — 追加の対応なしでビルドできた。`simulation-core` の依存は serde / serde_json のみ（`rayon` は feature `parallel` を OFF）。
`std::time`・スレッド・ファイル I/O をカーネルが使っていないので、`wasm32-unknown-unknown` の制約に当たらない。
サイズ: `--profile wasm` で **73.7 KB**（debug info 付き release だと 2.86 MB）。インスタンス化 14 ms。

**2. 決定論** — 同一条件（seed 42、1,000 体、128²、1,000 tick）で native（`cargo test -p cz-wasm-probe --release -- --nocapture`）と wasm の `state_hash` が一致した: `0x1c677f12c51ae3b1`。
整数演算のみ（ADR-0003）なので予想どおりだが、推測ではなく計測で確認した。CI に同じ比較を入れる（ADR-0010 2b-1）。

**3. 性能**（wasm、メインスレッド、50 tick ずつ呼び出し）

| ケース | 個体数（開始 → 終了） | tick | ms | tick/s | ns/体·tick |
|---|---|---|---|---|---|
| seed 42, 1,000 tick | 1,000 → 1,312 | 1,000 | 764 | **1,308** | 579 |
| seed 42, 2,000 tick | 1,000 → 1,333 | 2,000 | 1,682 | 1,189 | 638 |
| 10,000 体, 400² | 10,000 → 13,357 | 200 | 1,245 | 161 | 531 |

native の同条件（CLI、3,000 tick 平均）は約 5,000 tick/s・152 ns/体·tick なので、wasm は **約 3.8 倍遅い**。
それでも 1,300 体の世界で 1,200〜1,300 tick/s であり、Toy 1 の目標 60 tick/s に対して約 20 倍の余裕がある。
10,000 体では 161 tick/s で、これも目標は満たすが描画を含めると余裕は小さい。

注意: 内蔵ブラウザのページが非表示だとタイマーが鈍る可能性がある。数値は表示状態で取った 1 回の計測で、分散は未評価。

**4. メモリとスナップショット** — 線形メモリは 10,000 体・400² の世界を含めて 13.6 MiB。
JSON スナップショットは 10,000 体世界で 3.0 MB、1,300 体世界では約 0.3 MB（native 計測: 往復 1.67 ms）。
巻き戻しのために 500 tick ごとに保持すると 20,000 tick で約 12 MB。許容範囲だが、バイナリ化（ADR-0010 2a-6）で 1/5〜1/10 にする。

## 決定

1. **Toy 1 はブラウザ内 WASM で実行する。サーバーは置かない。**
2. 製品用バインディングは **wasm-bindgen**（`crates/cz-web`、ADR-0010 2b-1）。C-ABI の probe は計測用に残す。
   理由: バイト列・文字列の受け渡し、JS 側の型定義、Worker との組み合わせが整っている。Rust 公式 WG が保守、MIT/Apache-2.0。
   代償: `wasm-bindgen-cli` とクレートのバージョン一致が必要（CI で固定）。
3. **Web Worker 1 つ = 世界 1 つ。** メインスレッドは描画のみ。分岐比較は Worker 2 つ。
   描画フレームは Worker から Transferable な ArrayBuffer で渡す（trace 形式ではなく、型付き配列の素のレイアウト）。
4. wasm threads（rayon）は使わない。SharedArrayBuffer に COOP/COEP ヘッダが要り、静的配信を複雑にする割に、1〜2k 体では利益が小さい（native の並列 decide は 1.3k 体で 1.35 倍、STATUS.md）。
5. 再現性の契約を「同一 wasm バイナリ + 同一 config + 同一介入列 → 同一ハッシュ」に拡張し、UI にハッシュを表示する。

## 却下した代替案

- **native サーバー + WebSocket 配信**: 性能は高いが、Toy 1 の「サーバーなしで動く」要件（PRODUCT.md 基準 4）に反し、配布・保存・分岐のたびに往復が要る。共有機能が必要になったときに再検討。
- **JS/TS でカーネルを書き直す**: 決定論と保存則のテスト資産を二重に持つことになる。却下。

## 再検討条件

- 1 世界あたり 10,000 体以上が製品要件になったとき（視界走査の最適化 → wasm SIMD → threads の順に検討）。
- 複数世界の同時実行で Worker 数が CPU コア数を超えるとき。
