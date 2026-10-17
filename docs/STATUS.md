# 実装進捗（新しいセッションはまずここを読む）

最終更新: 2026-10-17 · **Phase 1 完了（ライフサイクル含む）** · 次は Phase 2 = Toy 1（ADR-0010）· カーネル版 `simulation-core 0.1.0`
リポジトリ: https://github.com/two-three-co/civilization-zero（private）· CI: GitHub Actions（fmt / clippy / test debug+release / wasm ビルド）

## 状態の区別

- **実装済み・テスト通過・計測済み**: 下表の Phase 1 項目、ADR-0008 のライフサイクル、ADR-0009 の wasm 計測。
- **設計のみ**: Phase 2（ADR-0010 のタスク表）、Phase 3 以降。
- **未着手**: Multiverse / Nomos / Genesis の全コード、`crates/cz-web`。

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
| 10 | シリアライズ | 済（形式 2） | スナップショット + ハッシュ照合 |
| 11 | 自動テスト | 済 | 37 テスト（proptest 48 ケース含む）、debug/release 両方通過、CI 緑 |
| 12 | ベンチマーク | 済 | criterion、下記 |
| 13 | 2D ビューア | 済 | `viewer/index.html`（trace 形式 2、年齢表示）、ブラウザで確認済み |
| 14 | ドキュメント | 済 | PRODUCT, README, ARCHITECTURE, ADR×10, research/*, EXP-001 |
| 15 | 出生・寿命・死因・イベント | 済 | ADR-0008、`tests/lifecycle.rs`、EXP-001 |
| 16 | 個体数の収束計測（10 シード） | 済 | `cz sweep`、EXP-001: 全シード収束、CV 0.033 |
| – | Store / Transfer / Communicate | **未** | Phase 2 で台帳と同時に（ADR-0010 2a-3） |
| – | GitHub private リポジトリ + CI 実行確認 | 済 | run 38984941736 success（2m09s） |

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
| 1,000 | 128² | ≈1,310 | **194 µs** | 144 µs | 1.35× | **148** |
| 10,000 | 400² | ≈12,900 | 2.24 ms | 0.92 ms | 2.4× | 174 |
| 100,000 | 1,280² | ≈137,000 | 28.4 ms | 9.49 ms | 3.0× | 207 |

手順 B が CLI の長時間平均（152 ns/体·tick）と一致する。手順 A は再生直後の食料が多い tick（採集が多く走査が少ない）に偏っており、約 4 倍楽観的だった。以後は手順 B を正とする。

### CLI（`cz run` / `cz sweep`、定常帯域、`--check` なし）

| 条件 | tick/s | ns/体·tick |
|---|---|---|
| 1,000 体から 3,000 tick（平均 ≈1,320 体） | 5,000 | 152 |
| sweep 20,000 tick × 10 seed | 4,850〜5,400 | — |
| `--check` 付き（毎 tick 不変条件検証）3,000 tick | 4,180 | 181（抽選キー遅延化の前） |

### ブラウザ内 wasm（ADR-0009、`--profile wasm`、73.7 KB）

| 条件 | tick/s | ns/体·tick |
|---|---|---|
| 1,000 体から 1,000 tick（→1,312 体） | 1,308 | 579 |
| 1,000 体から 2,000 tick | 1,189 | 638 |
| 10,000 体 400² を 200 tick | 161 | 531 |

native と同条件で `state_hash` 一致（`0x1c677f12c51ae3b1`）。

### その他

- `state_hash`（≈1,310 体, 128²）: 266 µs。JSON スナップショット往復: 1.67 ms（約 0.3 MB）。
- 並列 decide（rayon, M1 8 コア）は手順 B で 1.35×（1.3k 体）〜 3.0×（137k 体）。1〜2k 体の Toy 1 では使わない（ADR-0009）。手順 A で「1k 体では逆効果」に見えたのは、走査の少ない tick ではスレッド起動コストが支配的になるため。

### 性能に関する注意（正直な記録）

Phase 1 初版で報告した 12.8 ns/体·tick は、出生前の 610 体がパッチ上で採集している tick 200 の状態での値だった。
定常帯域では個体の多くが食料を探して視界（半径 4 = 41 セル）を走査しており、`policy::decide` が実行時間の 82%（macOS `sample`）。
抽選キーの遅延化（ADR-0008）で 15% 改善したが主因は走査自体。調査は TODO（Toy 1 の 60 tick/s 目標には現状でも 20 倍の余裕がある）。

## 観察された挙動（EXP-001、既定設定）

- Forager（10 シード）: 1,000 体 → tick 300 に 1,367〜1,507 体のピーク → 約 1,250〜1,365 体の帯域に収束（tick ≤ 400 で ±10% 帯域入り、以後 20,000 tick まで ±6% 程度）。絶滅なし。死因は餓死 96%、老衰 4%。
- Random（10 シード）: tick 564〜693 で全滅。出生は計 3 件。帰無モデルとして機能。

## ゴールデンハッシュ

`World::new(seed 42, default).run(200).state_hash()`。遷移関数を意図的に変えたときはこの値を更新し、ここに日付と理由を残す。

| 日付 | 値 | 理由 |
|---|---|---|
| 2026-10-17 | 0xaac57b73180a3d65 | 初版 |
| 2026-10-17 | 0xd1890b311b92678e | ADR-0008: 出生・寿命・死因・params・初期年齢乱数化・抽選キー遅延化、スナップショット形式 2 |

## 次のタスク

docs/TODO.md と ADR-0010（Phase 2 = Toy 1 のタスク表）を参照。最初に着手するのは 2a-1（`Intervention`）と 2b-2（UI 骨格）で、並行可能。
