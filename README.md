# Civilization Zero

人工生命が社会を形成し、エージェント自身が制度を発明し、その効果を並列世界で検証するための研究開発プラットフォーム。
最終的には一般ユーザーが「文明を観察 → 巻き戻し → 介入 → 分岐を比較」して遊べる製品を目指す（[docs/PRODUCT.md](docs/PRODUCT.md)）。

中心的な問い: **「人工社会は、自らを統治する実行可能なルールを発明できるか？」**

**遊ぶ: https://two-three-co.github.io/civilization-zero/** （Toy 1。ブラウザだけで動き、サーバーには何も送らない。遊び方は [docs/TUTORIAL.md](docs/TUTORIAL.md)）

現在は **Phase 4: Institutions（Nomos Engine）** まで完了しています（2026-10-18、[ADR-0014](docs/adr/0014-phase4-institutions.md) の完了判定）。世界の中に規則（先占の侵害・約束の不履行・加害のどれを違反とし、違反者を何 tick 係争中にし、どの応答を制裁と数えるか）と憲章を置け、個体がそれを守り・破り・罰し、動議と投票で成立させられます。規則は物理を変えず、制裁は普通の加害・拒否をカーネルが「係争中の個体に届いた」と読んだものです。公開 URL では Toy 1 と Phase 3 の操作に加えて、制度エクスプローラ（`?nomos=1`）で規則の布告と議会の動議・投票・可決の時系列、違反（誰が・どの規則を・誰に・根拠）、制裁とそれが答えた係争、率のカーブ（E・違反/tick・S′・S）、「Compare: with vs without rule」による規則の有無の分岐比較が見られます（TUTORIAL 10〜11 節）。事前登録した実験 EXP-004（30 シード）では、台本の制裁者と先占の規範のある世界で学習者の侵害率が 0.080 → 0.013 に下がりましたが、その機構は違反者の除去であって規範の学習ではありません（[docs/research/experiments/EXP-004-first-occupant-norm.md](docs/research/experiments/EXP-004-first-occupant-norm.md)）。
進捗と計測値は [docs/STATUS.md](docs/STATUS.md)、設計は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)、
判断の記録は [docs/adr/](docs/adr/)、研究仮説・リスク・実験は [docs/research/](docs/research/) を参照。

## 必要なもの

- Rust 1.82+（`rustup`）。wasm 計測には `rustup target add wasm32-unknown-unknown`
- Python 3（静的ファイルを配信するだけ。任意）

## 使い方

```bash
cargo test --workspace --features simulation-core/parallel
```

1,000 体の世界を 2,000 tick 進め、毎 tick 不変条件を検証:

```bash
cargo run --release -p cz-cli -- run --agents 1000 --ticks 2000 --check
```

10 シードの個体数の収束を計測（docs/research/experiments/EXP-001）:

```bash
cargo run --release -p cz-cli -- sweep --seeds 10 --ticks 20000 --policy forager --out out/forager
```

ビューア用トレースを出して静的サーバーで見る:

```bash
cargo run --release -p cz-cli -- run --policy mixed --ticks 600 --trace out/trace.json --every 3
```

```bash
python3 -m http.server 8765 --bind 127.0.0.1
```

ブラウザで `http://127.0.0.1:8765/viewer/index.html?trace=../out/trace.json` を開くと再生できます。

イベント台帳（JSON Lines）を書き、tick 範囲を読む:

```bash
cargo run --release -p cz-cli -- run --ticks 20000 --log out/events.jsonl && cargo run --release -p cz-cli -- log --path out/events.jsonl --from 1000 --to 1002
```

制度世界（`institution_preset`: 学習者 128 行 90% + 台本の Enforcer 10%、先占・侵害・約束の追跡と議場）を 300 tick 進め、議会が tick 51 に可決した先占の規範を台帳で読む（公開 URL の `?nomos=1&policy=institution&seed=42` と同じ世界。動議そのものは tick 19 = `--from 19 --to 20`）:

```bash
cargo run --release -p cz-cli -- run --policy institution --seed 42 --ticks 300 --log out/inst.jsonl && cargo run --release -p cz-cli -- log --path out/inst.jsonl --from 51 --to 52
```

規則を布告して「規則あり / なし」の 2 枝を比べる（同一 tick の 2 回の fork は MATCH、ADR-0014 D8）:

```bash
cargo run --release -p cz-cli -- fork --preset nomos --policy learner --ticks 600 --at 300 --enact encroach
```

Toy 1 の UI（本物のカーネルを wasm で実行。`wasm-pack` が必要。遊び方は [docs/TUTORIAL.md](docs/TUTORIAL.md)、構成は `web/README.md`）:

```bash
cd web && npm install && npm run build:wasm && npm run dev
```

スナップショットの保存と再開（`.json` は可読形式、それ以外はバイナリ）:

```bash
cargo run --release -p cz-cli -- run --ticks 500 --snapshot out/t500.bin && cargo run --release -p cz-cli -- resume --snapshot out/t500.bin --ticks 500
```

巻き戻しと分岐（ADR-0012）: 2 回同じ fork をしてハッシュが一致することを確認し、スナップショット + 台帳から再生する:

```bash
cargo run --release -p cz-cli -- fork --ticks 2000 --at 1000 --scale-food 500
```

```bash
cargo run --release -p cz-cli -- run --ticks 1000 --snapshot out/t1000.bin && cargo run --release -p cz-cli -- run --ticks 2000 --log out/t2000.jsonl && cargo run --release -p cz-cli -- replay --snapshot out/t1000.bin --log out/t2000.jsonl --to 2000
```

ベンチマーク:

```bash
cargo bench -p simulation-core --features parallel
```

wasm32 で動かして native とハッシュを比べる（ADR-0009）:

```bash
cargo build -p cz-wasm-probe --target wasm32-unknown-unknown --profile wasm && cargo test -p cz-wasm-probe --release -- --nocapture
```

その後 `http://127.0.0.1:8765/viewer/wasm-probe.html` を開く。

## 構成

```
crates/simulation-core   決定論的ワールドカーネル（ライブラリ）
crates/cz-cli            実験ランナー `cz`
crates/cz-wasm-probe     wasm32 可否と性能の計測用 C-ABI
crates/multiverse        再生・分岐・系譜（ADR-0012）
crates/cz-web            wasm-bindgen 境界（`WasmWorld`）と読み手（交渉スレッド、制度）
web/                     Toy 1 UI（Vite + React + TS、Worker 経由で cz-web の wasm を駆動）
viewer/                  静的 2D トレースビューア / wasm 計測ページ
docs/                    製品ゴール・設計・ADR・研究仮説・実験・進捗
```

## 決定論の契約

同一のクレートバージョンと `WorldConfig`（seed を含む）から、プラットフォームを問わず bit 単位で同一の状態列が得られます。
カーネルは浮動小数点・HashMap 走査・共有乱数状態を使いません。詳細は ADR-0002〜0004、ライフサイクルは ADR-0008。

## 研究上の注意

本リポジトリのどの結果も「創発」や「世界初」を主張しません。方策 `Forager` と台本の `Enforcer` は設計者の知識を直接書いたものであり、
その振る舞いは証拠ではなく健全性チェック / 陽性対照です（[docs/research/RISKS.md](docs/research/RISKS.md) R7 / R14）。主張の条件は [docs/research/HYPOTHESES.md](docs/research/HYPOTHESES.md) に定めます。

## ライセンス

MIT OR Apache-2.0
