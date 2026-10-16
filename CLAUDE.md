# Civilization Zero — 作業規約

新しいセッションは最初に `docs/STATUS.md` → `docs/TODO.md` → `docs/ARCHITECTURE.md` の順に読む。

## 原則
- 動くコードを優先。仕様書だけで終わらない。
- 未実装・未テスト・未計測を「済」と報告しない。性能は必ず計測値（機種・コミット・日付付き）で報告する。
- 主要な設計判断は `docs/adr/NNNN-*.md` に残す。
- 遷移関数を変えたら `tests/determinism.rs` のゴールデンハッシュを更新し、STATUS.md の表に理由を書く。
- 「創発」「世界初」「新規性」は証拠（事前登録した指標・ベースライン・複数シード・CI）なしに書かない。
- 外部クレートは保守状況・ライセンス・必要性を確認してから追加する。

## カーネルの不変条件（壊さないこと）
- 浮動小数点を使わない。HashMap を走査しない。共有 RNG を持たない（`RngStream` を座標から作る）。
- `World.agents` は id 昇順。id は再利用しない。
- 方策は `Observation -> Intent` の純関数。世界を直接変更しない。
- `Failure` は物理的不能のみ。規則違反は別概念。

## コマンド
- テスト: `cargo test --workspace --features simulation-core/parallel`（release でも通すこと）
- lint: `cargo fmt --all && cargo clippy --workspace --all-targets --features simulation-core/parallel -- -D warnings`
- ベンチ: `cargo bench -p simulation-core --features parallel`

Rust は `/opt/homebrew/opt/rustup/bin` 経由で導入（keg-only）。PATH に無ければ追加する。
