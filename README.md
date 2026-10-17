# Civilization Zero

人工生命が社会を形成し、エージェント自身が制度を発明し、その効果を並列世界で検証するための研究開発プラットフォーム。
最終的には一般ユーザーが「文明を観察 → 巻き戻し → 介入 → 分岐を比較」して遊べる製品を目指す（[docs/PRODUCT.md](docs/PRODUCT.md)）。

中心的な問い: **「人工社会は、自らを統治する実行可能なルールを発明できるか？」**

現在は **Phase 1: Deterministic World Kernel**（出生・寿命を含む）が完了した段階です。制度・認知・分岐世界はまだありません。
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
viewer/                  静的 2D トレースビューア / wasm 計測ページ
docs/                    製品ゴール・設計・ADR・研究仮説・実験・進捗
```

## 決定論の契約

同一のクレートバージョンと `WorldConfig`（seed を含む）から、プラットフォームを問わず bit 単位で同一の状態列が得られます。
カーネルは浮動小数点・HashMap 走査・共有乱数状態を使いません。詳細は ADR-0002〜0004、ライフサイクルは ADR-0008。

## 研究上の注意

本リポジトリのどの結果も「創発」や「世界初」を主張しません。方策 `Forager` は設計者の知識を直接書いたものであり、
その振る舞いは証拠ではなく健全性チェックです。主張の条件は [docs/research/HYPOTHESES.md](docs/research/HYPOTHESES.md) に定めます。

## ライセンス

MIT OR Apache-2.0
