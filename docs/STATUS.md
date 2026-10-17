# 実装進捗（新しいセッションはまずここを読む）

最終更新: 2026-10-17 · **Phase 1 完了（ライフサイクル含む）** · **Phase 2 = Toy 1 完了（配信中: https://two-three-co.github.io/civilization-zero/ ）。Phase 3（Agent Cognition, ADR-0013）進行中: 3-1〜3-4 済**（ADR-0010）· カーネル版 `simulation-core 0.1.0`
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
| 10 | シリアライズ | 済（形式 6、JSON + バイナリ） | スナップショット + ハッシュ照合、ADR-0006 |
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
| 3-5 〜 3-10 | 方策の枠と SetPolicy、Reflex、Learner、EXP-002、UI、文書 | 未 | — |

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

## 次のタスク

**Phase 3（Agent Cognition）**: 設計は [ADR-0013](adr/0013-phase3-agent-cognition.md)、タスクは docs/TODO.md。3-1〜3-4 済。次は 3-5（`PolicyKind::{Reflex, Learner}` の枠、`Intervention::SetPolicy`、CLI の方策拡張）。3-9a（UI の社会マーカー・インスペクタの記憶表示）は並行作業中、3-9b（交渉スレッド表示）は 3-4 の台帳イベント `Proposed / Accepted / Rejected / Transferred` から作れる。
