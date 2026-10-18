# Civilization Zero — 作業規約

新しいセッションは最初に `docs/PRODUCT.md` → `docs/STATUS.md` → `docs/TODO.md` → `docs/ARCHITECTURE.md` の順に読む。

## ゴール（docs/PRODUCT.md、2026-10-18 改定）
- 最終目標は「一般ユーザーが遊べる製品」。研究的な正しさ（決定論・再現性・検証可能性）は巻き戻し・分岐体験の土台として守る。
- 理念: まだ誰も見たことがなく、公開後にどうなるか予想がつかず、人間が介入し・問い・確かめることで理解に変わるもの。人間は観客ではなく、現象に問いを立て、名前をつける側。
- 遊び方（観察型・パズル型・箱庭型・対戦型…）は Toy 1 が完成してから製品オーナーが触って決める。それまで特定の遊び方に設計を固定しない。
- **Toy 1 = ブラウザで 5 操作**: 観察 → 任意の時点に巻き戻す → 介入する → 分岐と元の世界を並べて比較する → **なぜ違いが生まれたかを辿れる**（5 は 2026-10-18 時点で未）。
- 設計原則 A〜F（詳細は PRODUCT.md）: **A** 実行環境を固定しない（native と WASM の両方を維持）/ **B** 歴史は「初期状態 + シード + 介入ログ」から再現でき、共有可能な単位として扱う（マルチプレイは ADR に検討事項として記録、実装しない）/ **C** 台帳は「何が起きたか」だけでなく原因となった行動・状態を辿れる情報を残す / **D** 事実と解釈を分ける（LLM は説明・語りに限定し状態遷移に関与させない。説明文は事実と解釈を区別できる構造で）/ **E** 「所有権」「封建制」「通貨」のような人間社会のラベルをシステム側で自動付与しない（研究用の内部指標は可、プレイヤー向け表示では命名は人間の役割）/ **F** 各 Phase の完了条件に製品オーナーがブラウザで触って確認できるデモを必ず含める。

## 原則
- 動くコードを優先。仕様書だけで終わらない。
- 未実装・未テスト・未計測を「済」と報告しない。性能は必ず計測値（機種・コミット・日付付き）で報告する。
- 主要な設計判断は `docs/adr/NNNN-*.md` に残す。
- 遷移関数を変えたら `tests/determinism.rs` のゴールデンハッシュ（config を含まない `World::dynamics_hash`）を更新し、STATUS.md の表と ADR に理由を書く。config のフィールド追加はゴールデンを変えない。ゴールデンは 3 本 — Forager `0x658ee873d1866501`、Learner 32 行 `0xcb8bc0a9772c09a4`、制度世界 `0xfedc281503180c8e`（後の 2 本は `crates/cz-web/tests/{native,wasm}.rs` にもあり、CI が wasm = native を照合）— で、符号化は「あるときだけ」の規約（空の記憶・`None` の表や記録・`nomos` オフの `World.nomos` と `arrived` はバイトを足さない）により、状態を増やしても既定世界の値を変えない。
- 「創発」「世界初」「新規性」は証拠（事前登録した指標・ベースライン・複数シード・CI）なしに書かない。
- 外部クレートは保守状況・ライセンス・必要性を確認してから追加する。

## カーネルの不変条件（壊さないこと）
- 浮動小数点を使わない。HashMap を走査しない。共有 RNG を持たない（`RngStream` を座標から作る）。
- `World.agents` は id 昇順。id は再利用しない。同 tick の出生は親 id 順に採番する。
- 方策は `Observation -> Intent` の純関数。世界を直接変更しない。
- `Failure` は物理的・手続き的不能のみ（届かないメッセージ、受け取っていない提案の受諾、埋まっている議場、動議のない投票など）。規則違反は決して `Failure` にならない（係争とカウンタになる。ADR-0014 D4）。
- ライフサイクルイベント（`Event`）は tick を含む自己完結レコード。Phase 2 の台帳にそのまま載る形を保つ。

## コマンド
- テスト: `cargo test --workspace --features simulation-core/parallel`（release でも通すこと）
- lint: `cargo fmt --all && cargo clippy --workspace --all-targets --features simulation-core/parallel -- -D warnings`
- ベンチ: `cargo bench -p simulation-core --features parallel`
- 個体数の収束計測: `cargo run --release -p cz-cli -- sweep --seeds 10 --ticks 20000 --out out/forager`

Rust は `/opt/homebrew/opt/rustup/bin` 経由で導入（keg-only）。PATH に無ければ追加する。
