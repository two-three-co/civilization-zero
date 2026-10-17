# 実装進捗（新しいセッションはまずここを読む）

最終更新: 2026-10-17 · **Phase 1 完了（ライフサイクル含む）** · **Phase 2 = Toy 1 完了（配信中: https://two-three-co.github.io/civilization-zero/ ）。Phase 3（Agent Cognition, ADR-0013）進行中: 3-1〜3-7、3-9a 済**（ADR-0010）· カーネル版 `simulation-core 0.1.0`
リポジトリ: https://github.com/two-three-co/civilization-zero（public）· CI: GitHub Actions（fmt / clippy / test debug+release / wasm ビルド）

## 状態の区別

- **実装済み・テスト通過・計測済み**: 下表の Phase 1 項目、ADR-0008 のライフサイクル、ADR-0009 の wasm 計測。
- **設計のみ**: Phase 2 の 2a-3（保留）、Phase 3 以降。
- **未着手**: Nomos / Genesis の全コード。`web/` は本物のカーネル（wasm）で観察・巻き戻し・介入・分岐比較・再現性検証ができ、5 分ツアー付き（PRODUCT.md の Toy 1 基準 1〜4 を公開 URL で満たす）。

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
| 10 | シリアライズ | 済（形式 7、JSON + バイナリ） | スナップショット + ハッシュ照合、ADR-0006 |
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
| 2a-4 | 再生 = 再実行（`multiverse::replay`）。台帳の介入を同じ tick に適用し、出生・死亡を台帳と照合、不一致は `Diverged { tick }` | 済 (2026-10-17) | `crates/multiverse/tests/replay.rs`（12 の tick で `state_at` = 直接実行、改竄・他シードの台帳を検出）。`cz replay`: バイナリスナップショット（tick 1,000）+ 台帳 → 2,000 まで **84 ms**、2,684 イベント照合、直接実行とハッシュ一致 |
| 2a-5 | `Multiverse` / `Branch`（head + tick 0 からの台帳 + 周期スナップショット）、`fork(from, at, interventions)` と系譜（`parent`, `forked_at`）、`cz fork` | 済 (2026-10-17) | `tests/fork.rs`（同一介入 2 回 fork でハッシュ一致、fork の fork の系譜、無効 fork の拒否）。`cz fork`: **fork 0.07 ms**、`state_at` は間隔 500 で最大 33 ms、間隔 100 で 6.7 ms。ADR-0012 |
| 2a-6 | バイナリスナップショット（postcard、マジック + 形式版 + ハッシュ照合）、`cz` は拡張子で切替 | 済 (2026-10-17) | `tests/snapshot_bin.rs`: 1,357 体で **56.8 KB**（JSON 304 KB）、往復 **0.69 ms**（JSON 1.63 ms）、改竄・切り詰め・JSON 誤投入を拒否。ADR-0006 改訂 |
| 2b-1 | `crates/cz-web`（wasm-bindgen）: `WasmWorld` が `Multiverse` を包み、cursor 方式で `seek` / `step`（head 以前は台帳照合付き再実行、追いつくと head を延伸）、`intervene(json)`、`events_at`、`snapshot_bytes`、`state_hash_hex`。`web/` の `kernel.worker.ts` がモックを置換（`?mock=1` で旧モック） | 済 (2026-10-17) | `tests/native.rs`（wrapper = カーネル、seek→step の合流、介入は head のみ）、`tests/wasm.rs` を **`wasm-pack test --node` で実行し native のゴールデンハッシュと一致**（CI）。wasm 205 KB（wasm-opt -O3）。`npm run build` が wasm ビルドを含む。**ブラウザで確認**: Play で tick 600・1,303 体、タイムラインで tick 300 / 100 に戻したときの state hash が native の `cz run --ticks 300 / 100`（`0x821ac350e76261c0` / `0x7068a87e706be374`）と一致、再度戻しても同一 |
| 2b-3 | 実測表示: Worker が `step` の所要時間をフレームに載せ、UI が直近 1 秒の sim tick/s・描画 fps・Worker ms/tick を表示。スパークラインを人口 + 食料の 2 系列に | 済 (2026-10-17) | **ブラウザ実測（M1、内蔵ブラウザ、約 1,330 体）**: 4 tick/フレームで 240 tick/s・60 fps、30 tick/フレームで **1,802 tick/s・60 fps**、Worker 0.22 ms/tick（165 ns/体·tick。native の 1.65 倍）。目標 ≥ 60 tick/s |
| 2b-4 | 巻き戻し: Worker が `seek` の所要時間を計測、UI に「last seek: N ms（re-ran M）」を表示。ドラッグ中のシークは最新のみ処理（間引き）。任意 tick への `Go` 入力 | 済 (2026-10-17) | 最悪ケース（間隔 250 → 249 tick 再実行）**45〜56 ms**、124 tick 再実行 30 ms、スナップショット上 0 ms。目標 ≤ 200 ms |
| 2b-5 | 介入パネル（干ばつ・資源量スケール・個体追加・個体除去）、プロトコルに `intervene` / `history`、履歴は **台帳から読み戻し**（`WasmWorld::interventions_json`）。head 以外では Apply 無効（分岐は 2b-6） | 済 (2026-10-17) | UI で tick 300 に干ばつ（regrow 0 / 10）を適用 → 履歴「tick 300 drought」→ tick 600 / 900 のハッシュが native `cz run --drought-at 300 --drought-amount 0 --drought-interval 10` と一致（`0x4392fb01743338c5` / `0x4be57883bb1d1a06`）。`tests/native.rs::intervention_history_comes_from_the_ledger` |
| 2b-6 | 分岐の並列比較: `WasmWorld::fromSnapshot`（fork はワーカー境界をバイナリスナップショットで越える。`Multiverse::fork` と bit 一致）、プロトコル v2（`snapshot` 要求、`init` に `snapshot`、`Frame.origin`）、「Fork here」（任意 tick）、2 ワーカー同期再生・同期シーク、分割表示、A/B の人口・食料・ハッシュ比較表、スパークライン重ね描き、「Verify: fork again」（同一スナップショット + 同一介入で作り直して同 tick のハッシュ照合） | 済 (2026-10-17) | `tests/native.rs::a_fork_built_from_a_snapshot_matches_multiverse_fork_and_is_reproducible`。**ブラウザ**: fork 21 ms、2 世界同期で 1,800 tick/s・60 fps、tick 400 で A / B のハッシュが native `cz run --ticks 400`（無介入 / `--drought-at 300`）と一致、Verify = MATCH（90 ms） |
| 2b-7 | アプリ内「Five-minute tour」（5 ステップを世界の状態から自動判定: 観察 → 巻き戻し → 介入 → 分岐 → Verify）、[docs/TUTORIAL.md](TUTORIAL.md)、ビルドにコミットを刻印（フッタ「built from commit …」）、`.github/workflows/pages.yml`（GitHub Pages）、CI が `web/dist` を `toy1-dist` アーティファクトとして保存 | 済 (2026-10-17)、**配信中** | ローカルでサブパス配信を検証後、GitHub Pages に配信し本番 URL でブラウザ検証（kernel バッジ、ツアー 5/5、Verify = MATCH） |
| 2a-3 | Store/Transfer + `Acted` | 未（Toy 1 に不要） | — |

### 配信

- **配信 URL: https://two-three-co.github.io/civilization-zero/**（GitHub Pages、`main` への push ごとに `pages` ワークフローが再配信。リポジトリは 2026-10-17 に public 化、Pages は GitHub Actions ソース、変数 `PAGES_ENABLED=true`）。
- 静的ファイルのみ（`web/dist`: HTML + JS + CSS + wasm + Worker、約 420 KB、サーバー不要）。フッタの「built from commit」でカーネル版を特定できる。PRODUCT.md 基準 4。
- 配布の代替: 各 CI 実行の `toy1-dist` アーティファクト（unzip して静的サーバーで配信）。

## Phase 2 完了判定（ADR-0010 の完了条件）

| 条件 | 状態 |
|---|---|
| PRODUCT.md の Toy 1 受け入れ基準 1〜4 | 1 ツアー付きで 4 操作を自力で体験可 / 2 分割表示と比較表・重ね描き / 3 Verify ボタンでハッシュ一致 / 4 静的配信 — すべて満たす |
| 計測: 1,300 体で ≥ 60 tick/s、巻き戻し ≤ 200 ms を画面に表示 | 1,802 tick/s（30 tick/フレーム）、最悪シーク 56 ms。画面に実測値表示 |
| 決定論: 同一介入 2 回でハッシュ一致、CI に native-wasm ハッシュ一致テスト | Verify = MATCH。`wasm-pack test --node` が CI で `GOLDEN_200` を照合 |
| ユーザーがブラウザで確認するデモ = Toy 1、配信 URL を記録 | 上記 URL |

**Phase 2 = Toy 1 完了（2026-10-17）。** 残件 2a-3（Store/Transfer + `Acted`）は Phase 3 の通信・交換と一緒に扱う。

## Phase 3（Agent Cognition）進捗 — ADR-0013 のタスク表に対応

| # | タスク | 状態 | 根拠 |
|---|---|---|---|
| 3-1 | 空間インデックス（`src/spatial.rs`、tick 開始時に `agents` から計数ソートで構築、派生状態で保存しない）、`Observation::neighbors()`（`social_radius` 内を (距離, id) 順、自分を除く）、`Perception { social_radius, see_policy, see_inventory, see_energy }`（既定 2 / off / on / off、`serde(default)` で旧 config も読める） | 済 (2026-10-17) | `tests/neighbors.rs`（4 テスト + proptest 64 ケースで総当たりと一致）、`spatial.rs` 単体 2 本。**既定世界の遷移は不変**（旧符号化で state_hash を再計算し `0xf16cb4be6e7ed69b` に一致）。計測: 構築 **10 µs**（1,310 体, 128²）、step 132 → **147〜154 µs/tick（+11〜16%、2 回計測、基準 +20% 以内）**、decide 0.97 → 1.10〜1.18 ms/10 tick |
| 3-2 | 行動 `Transfer { to, n }` / `Store(n)` / `Harm { target }`（S(t) の位置で隣接判定、行為者 id 順、試行コスト）、失敗 `NotAdjacent` / `TargetFull` / `NoSuchTarget`、`DeathCause::Violence`（加害を受けた tick の死、餓死より優先）、イベント `Transferred` / `Harmed`（常時）/ `Acted`（`record_actions`）、台帳 `transfers` / `food_transferred` / `food_stored` / `harms`、config `transfer_cost` 100 / `harm_cost` 2,000 / `harm_damage` 20。Random 方策は 10 行動を一様に抽選（社会行動の対象は隣接個体）。`cz run --record-actions`、表に d.viol / xfer / harm 列。スナップショット形式 4（先頭の `format` だけ先に読んで旧版を `Format(n)` で拒否） | 済 (2026-10-17) | `tests/interaction.rs`（8 テスト: 移転・部分移転・4 種の失敗と試行コスト・S(t) 隣接と id 順・貯蔵・加害の累積と Violence・Acted の記録と JSON・Random 世界の不変条件）。**Forager 世界の遷移は不変**（旧定義のハッシュを再計算して `0xb7b914705736f95f` に一致）。計測: step 148 µs/tick（3-1 と同じ）、**`Acted` 台帳は 1,000 体 × 2,000 tick で 2,634,945 イベント・220 MB（84 B/イベント、記録なし 680 KB の 324 倍）** → 診断用。Random は加害コストで tick 300 までに絶滅 |
| 3-3 | 行動 `Signal(Message::Say(u8))`（コスト `signal_cost` 既定 0）、`Agent.last_signal { tick, message }`、`Observation::neighbors()` の `Neighbor.signal`（**前 tick の発信だけ**を `signal_radius` 内で観測、既定 2）、カーネルが書く記憶 `Agent.recent: Vec<Interaction { tick, other, kind, value }>`（`kind` = GaveTo / ReceivedFrom / Harmed / HarmedBy / SawHarm / Heard、上限 `memory_slots` 既定 8、0 で無効）。Apply の 5b（死亡処理の前）でその tick の移転・加害・目撃（被害者から `social_radius` 内）・聴取（発信者から `signal_radius` 内）を起きた順に反映。`Event::Signaled`（`record_signals`、既定 OFF）、台帳 `signals`、`step` は空間インデックスを Decide と Apply で共有（`apply_with`）。Random は `Say(0..8)` を含む 11 行動を抽選。スナップショット形式 5。`cz run` の表に sig 列、`cz log` に signaled | 済 (2026-10-17) | `src/memory.rs`、`tests/memory.rs`（5 本: 発信は同 tick に聴取・次 tick だけ観測・半径外は不可、記憶の順序と上限と `memory_slots = 0`、Forager 世界は記憶も発信も空、スナップショット往復とハッシュへの寄与、**proptest 32 ケースで serial = parallel の記憶一致・上限・oldest-first**）。**Forager のゴールデン `0x658ee873d1866501` は不変**（空の `recent` / `None` の `last_signal` は符号化にバイトを足さない）。計測（M1）: step **154 µs/tick**（1,316 体、3-1 の 147〜154 の範囲内）、バイナリ往復 0.75 ms。スナップショット 1,357 体 56.8 → **59.6 KB（+5%）**、個体部分は Forager 19.7 B/体、Random（記憶 5.5 件/体）46 B/体、満杯 8 件で 64 B/体（記憶なし 15〜20 B）→ 全個体満杯でも約 120 KB（≈ 2.0 倍、基準 ≤ 2 倍。tick・id が大きいと varint が伸び少し超えうる）。ADR の見込み 125 KB 増より小さい |
| 3-4 | 交渉メッセージ `Signal(Propose { to, give, want, by } / Accept { to, offer_tick } / Reject { to })`。宛先は S(t) で `signal_radius` 内の自分以外の生存個体（外なら新設の `Failure::OutOfEarshot`、不在・死亡・自分宛は `NoSuchTarget`、`signal_cost` は試行時に払う）。`Accept` は受諾者の `recent` に `ProposalFrom { other: to, tick: offer_tick }` があるときだけ成立（なければ `NoSuchOffer`。`memory_slots = 0` の世界では約束は成立しない）。記憶は Phase 5b で `ProposalFrom` / `Accepted`（value = `Terms::pack()` = give \| want≪8 \| by≪16）/ `Rejected`（0）/ `Promised`（自分が `other` に渡すと約束した量: 提案者は `give`、受諾者は `want`）。交渉メッセージは `last_signal` を書かず傍観者にも届かない。台帳イベント `Proposed` / `Accepted`（契約条件と `offer_tick` を複写）/ `Rejected` は届いたときだけ**常時**記録、カウンタ `proposals / acceptances / rejections`（`signals` は `Say` のまま）。**履行は強制しない**（返済は通常の `Transfer`、不履行は台帳から判定。Phase 4 の接続点、ADR-0013 D5 追補）。Random は 14 行動（Propose / Accept / Reject を含む）。スナップショット形式 6。`cz run` の表に prop / acc / rej 列、`cz log` に proposed / accepted / rejected | 済 (2026-10-17) | `tests/negotiation.rs`（8 本: 提案 → 受諾 → 移転 → 返済が台帳と両者の `recent` に順に並ぶ、期限が過ぎても物理・記憶・失敗・イベントが一切変わらず台帳だけから不履行を計算できる、拒否、半径外・不在・自分宛・未受領・誤 tick・無記憶・死者の失敗と試行コスト、S(t) での判定、Forager ゴールデン不変、Random 世界の往復（JSON / バイナリ）と形式 5 の拒否、**proptest 32 ケースで serial = parallel の記憶・台帳・イベント一致、上限、値の整形**）、`memory.rs` 単体 3 本。**Forager のゴールデン `0x658ee873d1866501` は不変**。計測（M1、c628dd6 との同日前後比較）: step 151.5 → **155.3 µs/tick（+2.5%、1,316 体）**、parallel decide 137.6 → 146.8 µs/tick、decide 単体 1.110 → 1.097 ms（不変）、state_hash 269 → 265 µs、バイナリ往復 750 → 725 µs。Forager 世界は交渉の経路を通らないので、差は apply の match 腕の増加と `TickReport` の拡張、または測定ばらつき（3-1 で 147〜154 と記録した幅と同程度）。スナップショット 1,357 体 **59,583 B（3-3 の 59.6 KB と同じ。台帳カウンタ 3 つ = +3 B）** |
| 3-9a | UI: 社会マーカーとインスペクタ。`crates/cz-web`: `WasmWorld::new(..., policy)`（`forager / random / mixed`、`cz run --policy` と同じ割当規則）、`social()`（直前 tick の移転・加害を台帳から、発信を各個体の `last_signal` から、ビューの個体位置に置いた 7 値/件の `Int32Array`。`record_signals` 不要、config 不変）、`ledger_json()`（台帳の累計）、`agent_json(id)`（`recent` / `last_signal` 込みの `Agent`、不在なら `null`）、`agent_events_json(id, from, to)`（台帳をその個体で絞る、半開区間）。`web/`: プロトコル **v3**（`Frame.social / ledger`、要求ごとの `watch { id, window }` → `Frame.watched`、`WorldInit.policy`。Transferable 方式とモック Worker は維持）、地図マーカー（移転 = 線と点、加害 = 線と ✕、発信 = 輪、`MarkerTrail` が 0.6 s 残す）、World A カードに台帳の累計と直近 1 s の tick あたり xfer / harm / sig、Branches 表に移転・加害・発信の行、インスペクタに最後の発信・記憶 `recent`（tick 付き、新しい順）・台帳の直近 100 tick、ヘッダの世界選択（`?policy=`、既定は forager のまま） | 済 (2026-10-17) | `tests/native.rs` 9 本（+4: 方策構成が CLI 規則と一致しハッシュ一致、`social()` が `TickReport` から組んだ期待値と 60 tick すべて一致し seek 後・fork 後も同じ、`agent_json` = `serde_json(Agent)`、`agent_events_json` の絞り込み・半開区間・死後の `died`）、`tests/wasm.rs` 3 本（+1: wasm 上で社会面が整合し同一世界 2 回でハッシュ一致。Forager のゴールデン `0x658ee873d1866501` 不変）。**ブラウザ確認（Apple M1、内蔵ブラウザ、127.0.0.1:5173 の dev サーバー、2026-10-17）**: `?policy=mixed` で tick 100 の台帳累計が `cz run --policy mixed` の tick 100 行と一致（xfer 119 / harm 722 / sig 3,349 / d.viol 24、alive 1,018）、tick 300 の state hash `5b28b80cd79e961c` が CLI と一致。地図に輪・✕・線の 3 種のマーカー、個体 #399（random）を選ぶと last signal `say 6 @ tick 99`、記憶 4 件（tick 35〜41、提案・受諾・拒否）、台帳 6 件、死後は記憶なし・台帳に `died of starvation at age 1333`。**30 tick/フレーム、約 1,310 体（mixed、tick 9,300）: 1,797〜1,801 tick/s・60 fps・Worker 0.23〜0.24 ms/tick**（Phase 2 の 1,802 / 60 / 0.22 と同等）。fork 27 ms、2 世界同期 15 tick/フレームで 932 tick/s・62 fps、Verify = MATCH（25 ms）。フレーム非依存の計測（同ページで `WorldClient` を直接駆動、約 1,300 体）: 30 tick の step 往復 6.3 ms（watch あり/なしで同じ）、`drawFrame` 0.41 ms、描画なしで 4,700 tick/s。wasm 226 → **278 KB**（3-4 込みで 286 KB。serde_json 直列化の分、TODO）。**公開 URL（https://two-three-co.github.io/civilization-zero/ 、commit 8fa9ac1、CI run 39010081547 / pages run 39010081508 success）でも確認**: `?policy=mixed` で tick 180 に 3 種のマーカーが多数、個体 #399 の記憶・台帳がローカルと同一、再生中に 4 → 30 tick/フレームへ変更しても再生が続く。**30 tick/フレーム: mixed 1,308〜1,322 体で 1,798〜1,799 tick/s・60 fps・0.24〜0.25 ms/tick、forager（既定）1,315 体で 1,800〜1,801 tick/s・60 fps・0.25 ms/tick**（いずれも同時に測った rAF が 61 Hz で、間引きなし） |
| 3-5 | **方策の枠のみ**: `PolicyKind::{Reflex, Learner}`（決定は 3-6 / 3-7 まで `forager()` に委譲する明示的なプレースホルダ）、`PolicyParams.table: Option<PolicyTable>`（`Reflex { weights: Vec<i32> }` = 16 特徴 × 8 行動の 1/1024 固定小数点、`Learner { q: Vec<i16> }` = `learner_states`(既定 32、1..=1024) × 8。固定方策は `None` でハッシュにバイトを足さない）、`PolicyParams` は `Copy` → `Clone`、`check_invariants` が方策と表の整合を検査、出生で表を継承（Learner は `learner_inherit_q` で白紙も可、重みの突然変異は 3-6）、`Intervention::SetPolicy { who: All \| Id \| Fraction { permille }, kind, params: Default \| Keep }`（`Fraction` は `Stream::Intervention` の seed 付き抽選、`Ids(Vec)` ではなく `Id` で `Intervention` の `Copy` を保つ）、CLI `--policy reflex\|learner\|mixed-learning`（reflex / mixed-learning は `--no-mutate` がなければ突然変異 ON）、`--learner-states N`、`cz run --set-policy KIND [--set-policy-at TICK]`、`cz fork --set-policy KIND`。スナップショット形式 7 | 済 (2026-10-17) | `tests/set_policy.rs`（7 本: Reflex / Learner 世界の身体・資源・台帳が Forager 世界と 600 tick 一致しハッシュだけ異なる、All / Id / Fraction と Default / Keep、Fraction の決定性と 0 / 1000、無効介入の無変更、JSON 形、Q の継承 / 白紙と重みの継承、両形式の往復・形式 6 の拒否・方策と表の不整合の拒否、`learner_states` の検証と旧 config の読み込み）、介入 proptest に `SetPolicy` を追加（32 ケース）、`multiverse/tests/fork.rs::switching_policies_at_a_fork_is_reproducible`（`All` と `Fraction 300` を同一 tick で 2 回 fork してハッシュ一致、台帳からの再生でも一致）。**Forager のゴールデン `0x658ee873d1866501` は不変**（native と wasm）。計測（M1、8fa9ac1 と同日の前後比較）: step/serial/1000 161.9 → **157.1 µs/tick**（同一コードの今日 2 回の計測が 155.3 / 161.9 なので差は検出できない）、decide 1.113 → 1.128 ms、state_hash 269 → 264 µs、バイナリ往復 757 → 735 µs、JSON 往復 2.38 → 2.45 ms（`"table":null` が 1 体ごとに増えた）。**スナップショット（seed 42、tick 1,000、約 1,316 体）: Forager 60,074 B（+1 B/体）、Reflex 232,786 B（零の重み 128 個 ≈ 130 B/体）、Learner 32 状態 400,917 B（零の Q 256 個 ≈ 258 B/体、≤ 1 MB）、Learner 128 状態 1,411,607 B（> 1 MB なので既定にしない）、mixed-learning 254,871 B**。零でない学習済みの Q は varint が伸びるので 3-7 で再計測する。`cz run --set-policy learner --set-policy-at 500` と `cz fork --set-policy learner` は同じハッシュ `0xf54d74fa36ddf734` に到達し fork は MATCH、切替後のスナップショットは `cz resume --check` で復元できる |
| 3-7 | Learner 方策（ADR-0013 D6）: `policy/learner.rs`。**状態** = 7 ビットの格子 128 状態（重要な順に hungry / has_food / food_here / food_in_sight（在庫に空きがあるときだけ走査）/ adjacent（距離 1 以内に他個体）/ upper（空腹帯の上半分 = 空腹 4 段階）/ heard（前 tick に Say を聞いた））を `fine % learner_states` で表の行に折る（既定 32 行 = 下位 5 ビット、128 行で全ビット）。**決定** = 行の argmax（同点は Reflex と同じ抽選）、**ε-greedy** は新 config `learner_epsilon_permille`（既定 50、`--learner-epsilon`）で `Decide` ストリームから 1 回引き、探索なら 8 種を一様に。パラメータは Reflex と同じアクチュエータ（Move = Forager の規則 4〜5 など）。方策は純関数のままで **`Intent.state`**（決定した行）を返すだけ。**Q 更新はカーネルの Apply Phase 5a**（Upkeep 直後、Memory・Death・Birth の前）: 新フィールド **`Agent.last_step: Option<LearnerStep { state, action, reward }>`**（カーネル所有、`recent` の兄弟、`None` はハッシュにバイトを足さない）に前 tick の記録を持ち、`Q[s,a] += (r·16 + (29·max Q[s'] >> 5) − Q[s,a]) >> 4`（α 1/16、γ 29/32、固定小数点 16 = 報酬 1 単位 = 100 ミリエネルギー、i64 で計算し i16 に飽和）、続けて今 tick の記録を書く。**報酬 = Upkeep 終了時までのエネルギー差分 /100**（出生で子へ渡す `child_energy` は後の Phase 7 で引かれるので報酬に入らない = 出生ボーナス `child_energy/100`）。新生児・`SetPolicy` 直後・手組み Intent で連鎖は切れ、死の tick の記録は使われない（終端の更新なし）。零の初期 Q は全種類同点なので各状態で各種類を 1 回ずつ試す。継承は 3-5 のまま（`mutated()` は Q に触れない）、`check_invariants` が `last_step` の整合（Learner のみ、行・種類の範囲）を検査。スナップショット形式 8 | 済 (2026-10-17) | `tests/learner.rs`（11 本: 状態ビットと空腹 4 段階の境界値・満杯で走査なし・隣接の半径規則・聴取は前 tick だけ、行に従う決定と 8 種のアクチュエータ・`HEARD` が 32 行では同じ行に折れ 128 行で分かれる、ε = 1000 で 8 種が一様（8,000 tick、5σ）・ε = 0 で唯一の最大が常に選ばれる・ε = 500 で非貪欲が約 43.75%、カーネルの更新値が `q_step(前の値, 前 tick の報酬, 次行の最大)` に一致し 1 セルだけ変わる・手組み Intent で連鎖が切れる・死で消える、出生 tick の報酬が 35（−365 ではない）で子は `None` と親の表、Rest の自己ループで Q が純関数列と一致して単調減少、`SetPolicy Keep` が記録を消して表を保ち次 tick に再開、両形式の往復・形式 7 の拒否・`state ≥ 行数` / `action ≥ 8` / Forager の記録 / ε > 1000 の拒否・旧 config の読み込み、8 行と 128 行の世界で子の表が親と同一（突然変異 ON）、Forager 世界に記録なし + ゴールデン不変、**proptest 12 ケース（ε・行数 {1, 2, 8, 32, 128, 300}・突然変異・tick 数）で serial = parallel = 再実行、不変条件、往復**）、`policy/learner.rs` 単体 4 本（固定点への上下からの単調収束と帯、i16 飽和、`credit` の境界、行の折り畳み）、`multiverse/tests/replay.rs::learner_worlds_replay_bit_exactly_from_snapshot_and_ledger`（mixed-learning 世界に `Fraction 500 Keep` と `All Default` の `SetPolicy` を挟み 9 つの tick の `state_at` が直接実行と `World` ごと一致、tick 300 からの再生 = head、バイト列から復元しても同じ）。`tests/set_policy.rs` の「Learner = Forager」は「Learner は自分の履歴・非零の表・記録を持つ」に書き換え、`multiverse/tests/fork.rs` は切替 tick の台帳に学習者の社会イベントが続くことを許容。**Forager のゴールデン `0x658ee873d1866501` は不変**（native と wasm、`last_step: None` と `Intent.state: None` はバイトを足さない）。**計測（M1、7537e85 と同日、`cargo bench -p simulation-core --features parallel`）: decide **Learner（1,000 tick 学習後、1,443 体）173.7 ns/体·tick（基準 ≤ 200 を満たす。余裕 13%）**、Forager 85.3、Reflex 初期重み 146.1、進化後 180.9。step/learner/1000 **344 µs/tick（238.5 ns/体·tick、≈ 2,900 tick/s）**、Forager 159.9 µs/tick、Reflex 238.0、parallel decide 147.5。CLI 3,000 tick（`--check` なし）: Forager 6,222 tick/s・122 ns/体·step、**Learner 3,160 tick/s・236 ns**、Reflex（突然変異 ON）3,027・241。**スナップショット（seed 42、tick 1,000、バイナリ）: Learner 32 行・学習済み 540,313 B（1,443 体、≈ 351 B/体 → 1,300 体で ≈ 0.49 MB、基準 ≤ 1 MB）**、JSON 2,014,104 B、Forager 61,391 B（1,316 体、`None` の +1 B/体）、Reflex 進化後 335,473 B（1,305 体）、mixed-learning 264,515 B（1,395 体）、**Learner 128 行 1,006,851 B（869 体、≈ 1.1 KB/体 → 1,300 体で ≈ 1.5 MB、> 1 MB。3-5 の決定どおり既定にしない）**。学習済み snapshot（バイナリ・JSON）を `cz resume --check` で 100 tick 継続すると直接実行の 1,100 tick と同じハッシュ `0xf09baefa0ff5cb13`。**単一シードの観察（seed 42、既定、EXP-002 ではない）**: Learner 世界は表が白紙の間に tick 100〜200 で 754 体が餓死して 306 体まで減り、tick 400 で 1,234、tick 1,000 以降は約 1,400 体（3,000 tick で 1,410。Forager は 1,305）、加害死 約 100 / 100 tick、加害 850〜1,200 / 100 tick、発信 3〜6 千 / 100 tick。ε = 0 でも 1,485 体（tick 1,000）。128 行は学習が遅く tick 1,000 で 869 体 |
| 3-6 | Reflex 方策（ADR-0013 D6）: `policy::feature` の 16 特徴（bias / hungry / has_food / pack_full / room / food_here / food_in_sight（在庫に空きがあるときだけ走査）/ has_target / neighbours / adjacent / heard / harmed・received・saw_harm・promised の記憶フラグ（16 tick）/ energy_level）、`score[k] = Σ weights[f·8+k]·x[f]`（i64、重みは 1/1024 固定小数点）、argmax（同点は Decide ストリームの抽選、同点のときだけ乱数消費）、固定アクチュエータ（Move = Forager の規則 4〜5、Gather は target を自セルに、Transfer / Harm は (距離, id) 順の隣接個体・いなければ Rest、Signal は前 tick に聞いた記号の反復）。**Forager 等価の初期重み `forager_weights()`**（Consume ≥ 1.0 > Gather 0.5 > Move 0 > 他 −1.0 の梯子）が `PolicyTable::blank(Reflex)` と `SetPolicy Default` の出発点。重みの突然変異（`mutation_enabled` で 128 個それぞれ ±`mutation_step_permille`、`±2^20` でクランプ、`check_invariants` で検査）。`SpatialIndex::crowd`（累積和で半径内と隣接の人数）、`policy::nearest_adjacent`。Forager の規則 4〜5 を `scan_best` / `forage_move` に分離（演算順序不変） | 済 (2026-10-17)。**性能基準は進化後の重みで未達** | `tests/reflex.rs`（8 本 + proptest 12 ケース: Forager 世界の全個体・複数 tick で Reflex の決定が Forager と一致、Reflex 世界が Forager 世界と 1,000 tick 身体・資源・台帳一致、特徴ベクトルの値、Forager の決定規則、重みが決定を変える・アクチュエータの対象、同点抽選の一様性と決定性、子の重みの突然変異と serial = parallel、往復）、`policy.rs` 単体 3 本、`spatial.rs` 単体 1 本。`tests/set_policy.rs` は初期重みと重みの突然変異に合わせて更新。**Forager のゴールデン不変**。計測（M1）: decide **Reflex 初期重み 147 ns/体·tick（基準 ≤ 150）**、**進化後 182 ns（基準超過 +21%）**、Forager 87 ns。step Reflex 238 µs/tick（1,316 体、Forager 161）。スナップショット Reflex 167 B/体（初期重み）、231 B/体（進化後） |
| 3-8, 3-9b/c, 3-10 | EXP-002、交渉スレッド UI、fixed vs learning 比較 UI、文書 | 未 | — |

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

### ブラウザ内 wasm — Toy 1 UI（`crates/cz-web`、wasm-opt -O3、205 KB、ADR-0011 後のカーネル）

| 条件 | 計測値 |
|---|---|
| 再生 30 tick/フレーム、約 1,330 体 | **1,802 tick/s、60 fps**、Worker 0.22 ms/tick（165 ns/体·tick） |
| 再生 4 tick/フレーム | 240 tick/s、60 fps（フレーム律速）、Worker 0.45 ms/tick |
| 巻き戻し、249 tick 再実行（間隔 250 の最悪） | 45〜56 ms |
| 巻き戻し、124 tick 再実行 / スナップショット上 | 30 ms / 0 ms |
| 0 → 300 tick の延伸（`Go`） | 185 ms（出生の多い序盤、0.6 ms/tick） |
| fork（スナップショット転送 + 新ワーカー + 介入） | 21 ms |
| 2 世界の同期再生、30 tick/フレーム | 1,800 tick/s、60 fps（1 世界と同じ。別ワーカーで並列） |
| Verify（同一 fork を作り直して tick 400 で照合） | 90 ms、MATCH |
| **3-9a 後（プロトコル v3、wasm 278 KB）** mixed 世界、30 tick/フレーム、約 1,310 体（tick 9,300） | **1,797〜1,801 tick/s、60 fps**、Worker 0.23〜0.24 ms/tick（内蔵ブラウザのペインが前面のときの実測。ペインが隠れると rAF が 1〜2 Hz に間引かれ画面の値は 0 になるので、以後の計測は同時に rAF 周波数を測って有効性を確かめている） |
| 3-9a 後、**公開 URL**（commit 8fa9ac1）、30 tick/フレーム、rAF 61 Hz を同時確認 | **forager（既定）1,315 体: 1,800〜1,801 tick/s、60 fps、0.25 ms/tick**。mixed 1,308〜1,322 体: 1,798〜1,799 tick/s、60 fps、0.24〜0.25 ms/tick。mixed の序盤（tick 180、4 tick/フレーム）: 247 tick/s・62 fps・0.35 ms/tick、tick あたり xfer 0.83 / harm 4.20 / sig 19.39 でマーカー多数 |
| 3-9a 後、mixed 世界、fork（tick 100、干ばつ）→ 2 世界同期 15 tick/フレーム | fork 27 ms、932 tick/s・62 fps、Verify = MATCH（25 ms） |
| 3-9a 後、フレーム非依存（同ページで `WorldClient` を直接駆動、30 tick/step × 40） | mixed 1,316 体: 往復 6.3 ms/step（watch あり/なし同じ）、Worker 0.199 ms/tick、4,700 tick/s。forager 1,299 体: 6.15 ms/step、0.194 ms/tick、4,878 tick/s。`drawFrame`（食料 + 個体 + マーカー + 軌跡）0.41 ms/フレーム |

### ブラウザ内 wasm — 計測プローブ（ADR-0009、`--profile wasm`、73.7 KB、ADR-0011 前のカーネル）

| 条件 | tick/s | ns/体·tick |
|---|---|---|
| 1,000 体から 1,000 tick（→1,312 体） | 1,308 | 579 |
| 1,000 体から 2,000 tick | 1,189 | 638 |
| 10,000 体 400² を 200 tick | 161 | 531 |

native と同条件で `state_hash` 一致（`0x1c677f12c51ae3b1`）。

### その他

- `state_hash`（≈1,310 体, 128²）: 261 µs。スナップショット往復: JSON 1.63 ms（304 KB）、**バイナリ 0.69 ms（56.8 KB）**。
- 3-3 後（2026-10-17, commit の bench）: step **154 µs/tick**（1,316 体）、parallel decide 146 µs/tick、`spatial_index` 11.2 µs、`state_hash` 267 µs、スナップショット往復 JSON 2.31 ms（346 KB）/ **バイナリ 0.75 ms（59.6 KB、1,357 体）**。個体部分: Forager 19.7 B/体、Random（記憶 5.5 件/体）46 B/体、満杯 8 件 64 B/体。10k 体 1.54 ms/tick（parallel 0.86）、100k 体 20.6 ms/tick（parallel 10.3）。
- 3-4 後（2026-10-17, M1, `cargo bench -p simulation-core --features parallel` を c628dd6 と同日に前後で実行）: step/serial/1000 1.5147 → **1.5532 ms/10 tick（151.5 → 155.3 µs/tick、+2.5%）**、parallel_decide/1000 1.3756 → 1.4681 ms、10k 体 15.25 → 15.60 ms/10 tick（parallel 8.76 → 9.09）、100k 体 205.3 → 211.1 ms（parallel 108.7 → 116.5）、decide/serial/1000 1.110 → 1.097 ms、`state_hash` 268.7 → 265.0 µs、`spatial_index` 11.30 → 11.43 µs、スナップショット往復 JSON 2.320 → 2.319 ms / **バイナリ 750.4 → 725.2 µs**。バイナリスナップショット 1,357 体 **59,583 B**（形式 6、3-3 比 +3 B）。計測中は別セッションの vite dev server が常駐していた。
- 3-5 後（2026-10-17, M1, `cargo bench -p simulation-core --features parallel` を 8fa9ac1 と同日に前後で実行）: step/serial/1000 1.6187 → **1.5710 ms/10 tick（161.9 → 157.1 µs/tick）**、parallel_decide/1000 1.5465 → 1.5434 ms、10k 体 16.21 → 15.99 ms/10 tick（parallel 9.01 → 9.12）、100k 体 216.3 → 221.3 ms（parallel 119.0 → 123.3）、decide/serial/1000 1.113 → 1.128 ms、`state_hash` 268.9 → 263.8 µs、`spatial_index` 11.78 → 11.87 µs、スナップショット往復 JSON 2.384 → 2.452 ms / バイナリ 757.0 → 735.3 µs。同じコード（9d2f6d7 相当）を今日 2 回計測して 155.3 と 161.9 µs/tick だったので、±4% は計測ばらつき。バイナリスナップショットは上の 3-5 行（Forager 60,074 B、Learner 400,917 B、Reflex 232,786 B、1,316 体前後）。
- 3-7 後（2026-10-17、M1、7537e85 と同日、`cargo bench -p simulation-core --features parallel -- '^decide/'` と `'^step/.*/1000$'`、別セッションの wasm-pack / vite が常駐）: decide/serial/1000 1.1246 ms（85.3 ns/体·tick）、decide/reflex/1000 1.9264 ms（146.1 ns）、decide/reflex_evolved/1000 2.3800 ms（180.9 ns、1,484 体）、**decide/learner/1000 2.5119 ms（173.7 ns/体·tick、1,443 体、1,000 tick 学習後。基準 ≤ 200）**。step/serial/1000 1.5985 ms/10 tick（159.9 µs/tick）、parallel_decide 1.4753 ms（147.5 µs/tick）、step/reflex/1000 2.3803 ms（238.0 µs/tick）、**step/learner/1000 3.4415 ms（344.2 µs/tick、238.5 ns/体·tick）**。Learner の step − decide ≈ 93 µs/tick は Forager の 47 µs/tick の約 2 倍: Q 更新（1 行の max + 1 セル）と `start_energy` の複写に加え、学習者は社会行動（加害・移転・発信）を選ぶので 3b・5b の経路を通る。CLI 3,000 tick: Forager 6,222 tick/s（122 ns/体·step）、Learner 3,160 tick/s（236 ns）、Reflex 3,027 tick/s（241 ns）。スナップショットは上の 3-7 行。
- 3-6 後（2026-10-17、Reflex、1,316 体 128²）: decide Forager 87 ns/体·tick、**Reflex 初期重み 147 ns**（基準 ≤ 150）、**Reflex 進化後（突然変異 ON で 1,000 tick、1,484 体）182 ns（基準超過）**。最適化の経緯: 210 → 168（crowd を index の累積和で数える、i128 除算の除去）→ 164（記憶の単一走査、走査は在庫に空きがあるときだけ）→ 147（crowd の重み行が零なら計算しない）、進化後 201 → 182（隣接個体の探索を index 直読みに）。step: Forager 161 µs/tick、Reflex 初期重み 238 µs/tick。スナップショット（tick 1,000）: Forager 60,944 B（20.7 B/体）、Reflex 初期重み 259,065 B（166.7 B/体）、Reflex 進化後 375,089 B（1,484 体、230.6 B/体）。
- Multiverse（ADR-0012）: fork 0.07 ms、`state_at` は最大でスナップショット間隔分の再実行（間隔 500 → 33 ms、100 → 6.7 ms）、再生 11,900 tick/s（照合込み）。
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

`World::new(seed 42, default).run(200).dynamics_hash()`（tick・id・資源・個体のみ。3-2 以降。3-1 時点は台帳込み、それ以前は `state_hash` = config も含む値）。遷移関数を意図的に変えたときはこの値を更新し、ここに日付と理由を残す。config や台帳のフィールド追加では変わらない。

| 日付 | 値 | 理由 |
|---|---|---|
| 2026-10-17 | 0xaac57b73180a3d65 | 初版 |
| 2026-10-17 | 0xd1890b311b92678e | ADR-0008: 出生・寿命・死因・params・初期年齢乱数化・抽選キー遅延化、スナップショット形式 2 |
| 2026-10-17 | 0x6e767bd160525c8e | ADR-0010 2a-1: `state_hash` の符号化に台帳の `food_intervened` / `energy_intervened` / `interventions` を追加、スナップショット形式 3。**遷移関数は不変**（旧符号化で再計算すると前行の値に一致することを確認済み） |
| 2026-10-17 | 0xf16cb4be6e7ed69b | ADR-0011: Forager の視界走査を 2 パス化。勝者の分布は不変（`tests/forager.rs` の一様性テスト）、乱数の消費列だけが変わる。行スライス化（commit ee515f4）はハッシュ不変だった |
| 2026-10-17 | 0xb7b914705736f95f | ADR-0013 3-1: ゴールデンを **config を含まない `dynamics_hash`** に再定義（config に `perception` を追加したため `state_hash` は変わる）。**遷移は不変**: 旧符号化（perception なしの config JSON）で `state_hash` を再計算すると前行の値に一致 |
| 2026-10-17 | 0x658ee873d1866501 | ADR-0013 3-2: `dynamics_hash` を **台帳も含まない**物理状態（tick・id・資源・個体）だけに再定義（台帳に社会行動の集計と死因 `Violence` を追加したため）。**遷移は不変**: 3-1 時点の定義で再計算すると前行の値に一致。以後、config と台帳の追加はゴールデンを変えない |
| 2026-10-17 | 0x658ee873d1866501（不変） | ADR-0013 3-3: `Agent.last_signal` / `Agent.recent` は**非空のときだけ**符号化に入るので、発信も記憶もない Forager 世界では値が変わらない。Random 世界は `Say` が抽選に加わり軌跡が変わる（EXP-001 の注記） |
| 2026-10-17 | 0x658ee873d1866501（不変） | ADR-0013 3-4: 交渉メッセージは `Say` と同じく Forager が発しない行動で、台帳カウンタ `proposals / acceptances / rejections` は `dynamics_hash` に入らない（`state_hash` にだけ入る）。`tests/negotiation.rs::forager_worlds_never_negotiate_and_keep_the_golden_hash` で確認。Random 世界は 14 行動の抽選になり軌跡が変わる（EXP-001 の注記） |
| 2026-10-17 | 0x658ee873d1866501（不変） | ADR-0013 3-7: `Agent.last_step` は **`Some` のときだけ**符号化に入り（Learner だけが持つ）、`Intent.state` は Forager では `None` で Apply の学習経路に入らない。config の `learner_epsilon_permille` は `dynamics_hash` に入らない。`tests/determinism.rs::golden_hash_is_stable`、`tests/learner.rs::forager_worlds_carry_no_learner_state_and_keep_the_golden_hash`、wasm の照合で確認。Learner 世界は表と記録がハッシュに入り、決定も Forager と異なる |
| 2026-10-17 | 0x658ee873d1866501（不変） | ADR-0013 3-5: `PolicyParams.table` は **`Some` のときだけ**符号化に入り、Forager / Random は `None`。`PolicyKind` の値 `Forager = 0` / `Random = 1` は変わらず末尾に `Reflex` / `Learner` を足した。config の `learner_states` / `learner_inherit_q` は `dynamics_hash` に入らない。`tests/determinism.rs::golden_hash_is_stable` と wasm の照合で確認。Reflex / Learner 世界は kind と表がハッシュに入るので Forager 世界とは別の値になる（3-5 では身体の軌跡は Forager と同一） |

## 次のタスク

**Phase 3（Agent Cognition）**: 設計は [ADR-0013](adr/0013-phase3-agent-cognition.md)、タスクは docs/TODO.md。3-1〜3-7、3-9a 済。次は 3-8（EXP-002: 事前登録 → 10 シード実行 → 記録。Learner 32 行は decide 174 ns で基準内、単一シードでは Forager と同程度の個体数帯域に達するが、これは EXP-002 ではない）か 3-9b/c（UI。3-9c は cz-web の `policy_mix` に learner を足し、`agents()` の policy 列を 0..=3 に広げる）。3-6 の decide 基準（≤ 150 ns/体·tick）は初期重みでは満たすが進化後の重みで 181 ns と超過しており、3-8 の前に基準の扱いを決める（ADR-0013 D6）。Learner の 128 行はスナップショットが 1 MB を超えるので EXP-002 の腕に入れるなら計測値を添えて判断する。
