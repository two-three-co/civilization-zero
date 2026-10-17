# 実装進捗（新しいセッションはまずここを読む）

最終更新: 2026-10-17 · **Phase 1 完了（ライフサイクル含む）** · **Phase 2 = Toy 1 進行中（2a-1, 2a-2, 2b-2 済）**（ADR-0010）· カーネル版 `simulation-core 0.1.0`
リポジトリ: https://github.com/two-three-co/civilization-zero（private）· CI: GitHub Actions（fmt / clippy / test debug+release / wasm ビルド）

## 状態の区別

- **実装済み・テスト通過・計測済み**: 下表の Phase 1 項目、ADR-0008 のライフサイクル、ADR-0009 の wasm 計測。
- **設計のみ**: Phase 2 の 2a-3 以降と 2b-1 / 2b-3 以降（ADR-0010 のタスク表）、Phase 3 以降。
- **未着手**: Multiverse（再生・fork）/ Nomos / Genesis の全コード、`crates/cz-web`（wasm-bindgen）。`web/` はモック Worker で動く骨格のみ。

## Phase 1 チェックリスト

| # | タスク | 状態 | 根拠 |
|---|---|---|---|
| 1 | Cargo workspace | 済 | `Cargo.toml`（simulation-core, cz-cli, cz-wasm-probe） |
| 2 | simulation-core | 済 | `crates/simulation-core` |
| 3 | World 構造体 | 済 | `world.rs` |
| 4 | Agent / Resource モデル | 済 | `agent.rs`, `resources.rs` |
| 5 | SimulationClock | 済（`World.tick` + `regrow_interval`） | 専用型は不要と判断 |
| 6 | Move / Gather / Consume / Rest | 済 | `action.rs` の表、`tests/actions.rs` |
| 7 | 初期行動方針 | 済（Forager + Random 帰無モデル、`PolicyParams` 継承） | `policy.rs`, ADR-0005/0008 |
| 8 | シード付き乱数 | 済 | `rng.rs`, ADR-0004 |
| 9 | 決定論的 step | 済 | `tests/determinism.rs`、native = wasm32（ADR-0009） |
| 10 | シリアライズ | 済（形式 3） | スナップショット + ハッシュ照合 |
| 11 | 自動テスト | 済 | 37 テスト（proptest 48 ケース含む）、debug/release 両方通過、CI 緑 |
| 12 | ベンチマーク | 済 | criterion、下記 |
| 13 | 2D ビューア | 済 | `viewer/index.html`（trace 形式 2、年齢表示）、ブラウザで確認済み |
| 14 | ドキュメント | 済 | PRODUCT, README, ARCHITECTURE, ADR×10, research/*, EXP-001 |
| 15 | 出生・寿命・死因・イベント | 済 | ADR-0008、`tests/lifecycle.rs`、EXP-001 |
| 16 | 個体数の収束計測（10 シード） | 済 | `cz sweep`、EXP-001: 全シード収束、CV 0.033（ADR-0011 後の再実行でも同値） |
| 17 | decide フェーズのプロファイルと高速化 | 済 | ADR-0011、`docs/perf/*.svg`、step 194 → 132 µs/tick |
| – | Store / Transfer / Communicate | **未** | Phase 2 で台帳と同時に（ADR-0010 2a-3） |
| – | GitHub private リポジトリ + CI 実行確認 | 済 | run 38984941736 success（2m09s） |

## Phase 2（Toy 1）進捗 — ADR-0010 のタスク表に対応

| # | タスク | 状態 | 根拠 |
|---|---|---|---|
| 2a-1 | `Intervention` enum（Drought / ScaleFood / SpawnAgents / RemoveAgent）、`World::intervene()`、`Event::Intervened`、台帳 `food_intervened` / `energy_intervened` / `interventions` | 済 (2026-10-17) | `src/intervention.rs`、`tests/intervention.rs`（8 テスト + proptest 32 ケース: 不変条件・無効時の無変更・同一列の再現・スナップショット往復）、`cz run --drought-at` |
| 2a-2 | `EventLog`（追記専用、tick 非減少を検証、`range(from..to)` は二分探索、JSON Lines のヘッダ付き書き出し/読み戻し）、`cz run --log` / `cz log` | 済 (2026-10-17) | `src/event_log.rs`、`tests/event_log.rs`（6 テスト: 台帳と World.ledger の一致、範囲取得、往復、順序違反・壊れたファイルの拒否）。**20,000 tick × 1,000 体の台帳 = 54,649 イベント、6.27 MB（約 115 B/イベント、313 B/tick）**、書き出し込みで 3 秒未満 |
| 2b-2 | `web/`: Vite + React + TypeScript、UI ↔ Worker プロトコル（`Frame` を Transferable な `Uint8Array` / `Int32Array` で転送）、モック Worker（スナップショットで seek）、Canvas2D 描画、タイムライン、個体インスペクタ、介入・分岐のプレースホルダ | 済 (2026-10-17) | `web/src/*`、`npm run build` 通過（JS 150 KB / gzip 48.7 KB）、CI に `web` ジョブ追加。ブラウザで再生・スクラブ・個体選択を確認 |
| 2a-3 〜 2a-6, 2b-1, 2b-3 〜 2b-7 | Store/Transfer + `Acted`、再生、fork、バイナリスナップショット、wasm パッケージ、実測表示、巻き戻し、介入 UI、分岐比較、配信 | 未 | — |

## 計測値（Apple M1, rustc 1.99.1, release, 2026-10-17）

### カーネル（criterion, `cargo bench -p simulation-core --features parallel`）

計測手順は本日 2 回変えた。いずれも 1,000 tick 進めた定常帯域の世界（EXP-001）を対象にする。

**手順 A: 1,000 tick 後の 1 tick だけを計測**（再生直後の食料が多い tick に偏る）

| 初期個体数 | グリッド | 計測時の個体数 | serial / tick | parallel decide / tick | serial ns/体·tick |
|---|---|---|---|---|---|
| 1,000 | 128² | ≈1,310 | 49.5 µs | 105 µs | 38 |
| 10,000 | 400² | ≈12,900 | 747 µs | 749 µs | 58 |
| 100,000 | 1,280² | ≈137,000 | 10.3 ms | 8.93 ms | 75 |

**手順 B: 1,000 tick 後の 10 tick（再生周期 1 つ分）を計測し tick あたりに換算**（現在のベンチの定義）

| 初期個体数 | グリッド | 計測時の個体数 | serial / tick | parallel decide / tick | 並列の効果 | serial ns/体·tick |
|---|---|---|---|---|---|---|
| 1,000 | 128² | ≈1,310 | 194 µs → **132 µs** | 144 → 115 µs | 1.14× | 148 → **100** |
| 10,000 | 400² | ≈12,900 | 2.24 → **1.37 ms** | 0.92 → 0.68 ms | 2.0× | 174 → **107** |
| 100,000 | 1,280² | ≈137,000 | 28.4 → **16.3 ms** | 9.49 → 7.24 ms | 2.25× | 207 → **119** |

「→」の左が本日朝の基準（ADR-0011 前）、右が ADR-0011 後。decide 単体（`decide/serial/1000`）は 110 → 74 ns/体·tick。
手順 B が CLI の長時間平均（ADR-0011 前 152、後 112 ns/体·tick）と一致する。手順 A は再生直後の食料が多い tick（採集が多く走査が少ない）に偏っており、約 4 倍楽観的だった。以後は手順 B を正とする。

### CLI（`cz run` / `cz sweep`、定常帯域、`--check` なし）

| 条件 | tick/s | ns/体·tick |
|---|---|---|
| 1,000 体から 3,000 tick（平均 ≈1,320 体）**ADR-0011 後** | **6,750** | **112** |
| 同上、ADR-0011 前（本日朝の基準） | 5,000 | 152 |
| sweep 20,000 tick × 10 seed、ADR-0011 後（前: 4,850〜5,400） | 7,190〜7,890 | — |

### ブラウザ内 wasm（ADR-0009、`--profile wasm`、73.7 KB）

| 条件 | tick/s | ns/体·tick |
|---|---|---|
| 1,000 体から 1,000 tick（→1,312 体） | 1,308 | 579 |
| 1,000 体から 2,000 tick | 1,189 | 638 |
| 10,000 体 400² を 200 tick | 161 | 531 |

native と同条件で `state_hash` 一致（`0x1c677f12c51ae3b1`）。

### その他

- `state_hash`（≈1,310 体, 128²）: 261 µs。JSON スナップショット往復: 1.63 ms（約 0.3 MB）。
- 並列 decide（rayon, M1 8 コア）は手順 B で 1.14×（1.3k 体）〜 2.25×（137k 体）（ADR-0011 後。直列が速くなった分、相対効果は 1.35×〜3.0× から下がった）。1〜2k 体の Toy 1 では使わない（ADR-0009）。手順 A で「1k 体では逆効果」に見えたのは、走査の少ない tick ではスレッド起動コストが支配的になるため。

### 性能に関する注意（正直な記録）

Phase 1 初版で報告した 12.8 ns/体·tick は、出生前の 610 体がパッチ上で採集している tick 200 の状態での値だった。
定常帯域では個体の多くが食料を探して視界（半径 4 = 41 セル）を走査しており、`policy::decide` が実行時間の 78%（`docs/perf/2026-10-17-decide-before.svg`）。
ADR-0011 で走査を行スライス化（ハッシュ不変、-16%）→ 2 パス分岐なし化（ハッシュ変更、-19%）し、累積で step 194 → 132 µs/tick。
切り分け（視界 0 の世界との差）では走査が decide の 84% を占めていた。ADR-0011 後も decide が 72.5%（`docs/perf/2026-10-17-decide-after.svg`）で、残りの主な候補は `apply` の採集請求ソート（約 7%）。

## 観察された挙動（EXP-001、既定設定）

- Forager（10 シード）: 1,000 体 → tick 300 に 1,367〜1,507 体のピーク → 約 1,250〜1,365 体の帯域に収束（tick ≤ 400 で ±10% 帯域入り、以後 20,000 tick まで ±6% 程度）。絶滅なし。死因は餓死 96%、老衰 4%。
- Random（10 シード）: tick 564〜693 で全滅。出生は計 3 件。帰無モデルとして機能。

## ゴールデンハッシュ

`World::new(seed 42, default).run(200).state_hash()`。遷移関数を意図的に変えたときはこの値を更新し、ここに日付と理由を残す。

| 日付 | 値 | 理由 |
|---|---|---|
| 2026-10-17 | 0xaac57b73180a3d65 | 初版 |
| 2026-10-17 | 0xd1890b311b92678e | ADR-0008: 出生・寿命・死因・params・初期年齢乱数化・抽選キー遅延化、スナップショット形式 2 |
| 2026-10-17 | 0x6e767bd160525c8e | ADR-0010 2a-1: `state_hash` の符号化に台帳の `food_intervened` / `energy_intervened` / `interventions` を追加、スナップショット形式 3。**遷移関数は不変**（旧符号化で再計算すると前行の値に一致することを確認済み） |
| 2026-10-17 | 0xf16cb4be6e7ed69b | ADR-0011: Forager の視界走査を 2 パス化。勝者の分布は不変（`tests/forager.rs` の一様性テスト）、乱数の消費列だけが変わる。行スライス化（commit ee515f4）はハッシュ不変だった |

## 次のタスク

docs/TODO.md と ADR-0010（Phase 2 = Toy 1 のタスク表）を参照。最初に着手するのは 2a-1（`Intervention`）と 2b-2（UI 骨格）で、並行可能。
