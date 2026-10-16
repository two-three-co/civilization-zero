# 実装進捗（新しいセッションはまずここを読む）

最終更新: 2026-10-17 · 現在 Phase 1 · カーネル版 `simulation-core 0.1.0`

## 状態の区別

- **実装済み・テスト通過・計測済み**: 下表の Phase 1 項目。
- **設計のみ**: Phase 2 以降（docs/ARCHITECTURE.md の「予定」、docs/research/*）。
- **未着手**: Multiverse / Nomos / Genesis の全コード。

## Phase 1 チェックリスト

| # | タスク | 状態 | 根拠 |
|---|---|---|---|
| 1 | Cargo workspace | 済 | `Cargo.toml`（simulation-core, cz-cli） |
| 2 | simulation-core | 済 | `crates/simulation-core` |
| 3 | World 構造体 | 済 | `world.rs` |
| 4 | Agent / Resource モデル | 済 | `agent.rs`, `resources.rs` |
| 5 | SimulationClock | 済（`World.tick` + `regrow_interval`） | 専用型は不要と判断 |
| 6 | Move / Gather / Consume / Rest | 済 | `action.rs` の表、`tests/actions.rs` |
| 7 | 初期行動方針 | 済（Forager + Random 帰無モデル） | `policy.rs` |
| 8 | シード付き乱数 | 済 | `rng.rs`, ADR-0004 |
| 9 | 決定論的 step | 済 | `tests/determinism.rs` |
| 10 | シリアライズ | 済 | スナップショット + ハッシュ照合 |
| 11 | 自動テスト | 済 | 25 テスト + proptest 48 ケース、debug/release 両方通過 |
| 12 | ベンチマーク | 済 | criterion、下記 |
| 13 | 2D ビューア | 済 | `viewer/index.html`、ブラウザで表示確認済み |
| 14 | ドキュメント | 済 | README, ARCHITECTURE, ADR×7, research/* |
| – | Store / Transfer / Communicate | **未** | Phase 2/3 で導入（R5 参照） |
| – | 出生・寿命 | **未** | 現状は個体数が減るだけ（RISKS.md R6） |
| – | CI | 設定のみ（`.github/workflows/ci.yml`）、GitHub 未接続のため未実行 | |

## 計測値（Apple M1, rustc 1.99.1, release, 2026-10-17）

`cargo bench -p simulation-core --features parallel`（criterion、密度 ≈ 0.06 体/セル、200 tick 後の定常状態）

| 個体数 | グリッド | serial step | parallel decide step | serial ns/体 |
|---|---|---|---|---|
| 1,000 | 128² | 12.8 µs | 42.5 µs（スレッド起動が支配的） | 12.8 |
| 10,000 | 400² | 137 µs | 164 µs | 13.7 |
| 100,000 | 1280² | 2.26 ms | 1.28 ms | 22.6 |

- `state_hash` (1,000 体, 128²): 220 µs。`snapshot` JSON 往復: 1.13 ms。
- CLI 実走 `cz run --ticks 2000 --check`（1,000 体、不変条件を毎 tick 検証込み）: 22,800 tick/s。

解釈: 1,000 体規模では並列化は不要（むしろ遅い）。10 万体で decide 並列化が効き始める。Apply 側の内訳は未プロファイル。

## 観察された挙動（seed 42、デフォルト設定）

- Forager: 最初の約 100 tick で 1,000 → 610 体に減少（資源パッチから遠い個体が餓死）。以後 2,000 tick まで死亡ゼロで食料流量が定常（採集 ≈ 消費 ≈ 110/tick）。
- Random: 約 1,100 tick で全滅。帰無モデルとして機能している（設計方策との差は健全性チェックであり、創発の証拠ではない）。

## ゴールデンハッシュ

`World::new(seed 42, default).run(200).state_hash() == 0xaac57b73180a3d65`
遷移関数を意図的に変えたときはこの値を更新し、ここに日付と理由を残す。

| 日付 | 値 | 理由 |
|---|---|---|
| 2026-10-17 | 0xaac57b73180a3d65 | 初版 |

## 次のタスク

docs/TODO.md を参照。
