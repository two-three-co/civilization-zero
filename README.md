# Civilization Zero

人工生命が社会を形成し、エージェント自身が制度を発明し、その効果を並列世界で検証するための研究開発プラットフォーム。

中心的な問い: **「人工社会は、自らを統治する実行可能なルールを発明できるか？」**

現在は **Phase 1: Deterministic World Kernel** が完了した段階です。制度・認知・分岐世界はまだありません。
進捗と計測値は [docs/STATUS.md](docs/STATUS.md)、設計は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)、
判断の記録は [docs/adr/](docs/adr/)、研究仮説とリスクは [docs/research/](docs/research/) を参照。

## 必要なもの

- Rust 1.82+（`rustup`）
- Python 3（ビューアを配信するだけ。任意）

## 使い方

```bash
cargo test --workspace --features simulation-core/parallel
```

```bash
cargo run --release -p cz-cli -- run --agents 1000 --ticks 2000 --check
```

```bash
cargo run --release -p cz-cli -- run --policy mixed --ticks 600 --trace out/trace.json --every 3
```

```bash
python3 -m http.server 8765 --bind 127.0.0.1
```

ブラウザで `http://127.0.0.1:8765/viewer/index.html?trace=../out/trace.json` を開くと再生できます。

```bash
cargo bench -p simulation-core --features parallel
```

スナップショットの保存と再開:

```bash
cargo run --release -p cz-cli -- run --ticks 500 --snapshot out/t500.json && cargo run --release -p cz-cli -- resume --snapshot out/t500.json --ticks 500
```

## 構成

```
crates/simulation-core   決定論的ワールドカーネル（ライブラリ）
crates/cz-cli            実験ランナー `cz`
viewer/                  静的 2D トレースビューア
docs/                    設計・ADR・研究仮説・進捗
```

## 決定論の契約

同一のクレートバージョンと `WorldConfig`（seed を含む）から、プラットフォームを問わず bit 単位で同一の状態列が得られます。
カーネルは浮動小数点・HashMap 走査・共有乱数状態を使いません。詳細は ADR-0002〜0004。

## 研究上の注意

本リポジトリのどの結果も「創発」や「世界初」を主張しません。方策 `Forager` は設計者の知識を直接書いたものであり、
その振る舞いは証拠ではなく健全性チェックです。主張の条件は [docs/research/HYPOTHESES.md](docs/research/HYPOTHESES.md) に定めます。

## ライセンス

MIT OR Apache-2.0
