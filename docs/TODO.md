# TODO

優先度順。各項目は「受け入れ基準」を満たしたら STATUS.md に移す。
Phase 2 の計画は ADR-0010（Toy 1）に従う。

## Phase 1 残り（Phase 2 と並行可）

- [ ] `apply` の採集請求ソート（実行時間の約 7%、ADR-0011 後）をセル単位のバケットに置換 — 計測して効果があれば。
- [ ] **乱数統計検定**: PractRand または TestU01 SmallCrush を `RngStream` に対して実施（ADR-0004）。
- [ ] EXP-001 のパラメータ格子版（再生量・代謝・出生閾値を振って帯域と絶滅確率を測る）。
- [ ] `Vec<Intent>` の tick ごとの確保を避ける（`decide_all` のバッファ再利用）— 計測して効果があれば。

## Phase 2: Toy 1（ADR-0010）— 完了。残っているのは任意の磨き込みのみ

- [ ] Store / Transfer 行動（イベント台帳と同時に実装）。
- [ ] `Event::Acted { agent, intent, outcome }` の記録（2a-3、Store/Transfer と同時。全行動記録は設定で ON/OFF、台帳サイズを計測）。
- [ ] 台帳のバイナリ形式（postcard は内部タグ付き enum を復号できないので別ワイヤ型が要る。JSON Lines で足りている間はやらない）。
- [ ] 2b-6 の磨き込み（任意）: 分岐を 2 本以上、fork した B をさらに fork、B の介入履歴の表示、B 側の個体インスペクタ（3-9c の比較で B の学習者を見たいときは `?policy=learner` で A を学習世界にする）。
- [ ] 3-9c の残り（任意。公開 URL での比較は 2026-10-17 に確認済み、STATUS 参照）: 曲線の履歴は 1 フレーム 1 点（1,200 点を超えると間引いて描画、tick/フレームを下げた長時間再生ではメモリが伸びる）。
- [ ] wasm サイズ（205 KB → 3-3 で 226 KB → 3-9a で 278 KB、3-4 と合わせて 286 KB → 3-9b で 337 KB → 3-9c で 345 KB。3-9a の +52 KB は `agent_json` / `ledger_json` / `agent_events_json` の serde_json 直列化が主、3-9b の +51 KB は `thread_json` / `deals_json` の `Thread` / `Deal` 直列化）: serde_json を境界から外す（介入・イベント・個体を構造体や typed array で渡す）か、`--profile wasm` 相当の設定を wasm-pack に通せば縮む見込み。計測してから。

## Phase 3: Agent Cognition（ADR-0013 のタスク表に従う）

- [x] 3-1 空間インデックス + `Observation::neighbors()` + `Perception` 設定（済 2026-10-17: ゴールデン不変、step +11〜16%）
- [x] 3-2 `Transfer` / `Store` / `Harm`、`DeathCause::Violence`、`Transferred` / `Harmed` イベント、`Acted` 記録（済 2026-10-17: ゴールデン不変を旧定義で証明、Random は社会行動も抽選）
- [x] 3-3 `Signal(Say)`、`last_signal` の観測、記憶リング `recent[K]` のカーネル更新（済 2026-10-17: `Agent.recent` は `memory` の兄弟フィールド、空なら符号化に入らずゴールデン不変、スナップショット形式 5）
- [x] 3-4 交渉メッセージ `Propose / Accept / Reject`（済 2026-10-17: `signal_radius` 内の宛先にだけ届き、受諾は記憶の `ProposalFrom` が要件、`Promised` は約束した量、`Proposed / Accepted / Rejected` を常時記録。履行は強制しない。スナップショット形式 6、ゴールデン不変）
- [x] 3-5 `PolicyKind::{Reflex, Learner}` の枠、`Intervention::SetPolicy`、CLI の方策拡張（済 2026-10-17: 枠のみ。`PolicyParams.table`（重み 16×8 i32 / Q `learner_states`×8 i16）、`SetPolicy { All | Id | Fraction }`、`--policy reflex|learner|mixed-learning`、`--set-policy`、スナップショット形式 7。両 kind の決定は 3-6 / 3-7 まで Forager に委譲）
- [x] 3-6 Reflex 方策（済 2026-10-17: 16 特徴 × 8 行動の線形スコア、Forager 等価の初期重みで全決定が一致、重みの突然変異。decide 147 ns（初期重み、基準内）/ 182 ns（進化後、当初基準 150 超過 → 2026-10-17 に進化後・Learner は ≤ 200 へ改定、ADR-0013 D6））
- [x] 3-7 Learner 方策（表形式 Q、整数、継承）（済 2026-10-17: `policy/learner.rs`。7 ビット 128 状態（hungry / has_food / food_here / food_in_sight / adjacent / upper / heard）を `learner_states` で折る（既定 32 = 下位 5 ビット）、ε-greedy（config `learner_epsilon_permille` 既定 50）、Q 更新はカーネルの Apply Phase 5a で `Intent.state` と `Agent.last_step` から（報酬 = Upkeep までのエネルギー差分 /100、出生の移転は差し引かない、固定小数点 16、i16 飽和）、スナップショット形式 8、Forager のゴールデン不変。計測は STATUS）
- [x] 3-8 EXP-002 事前登録 → 実行 → 記録（済 2026-10-17: 仕様コミット da4bcab → 実行 → 結果。腕 Random / Forager / Reflex 突然変異 OFF・ON / Learner 32・128 行 / Reflex 50k tick、seed 0〜9、20,000 tick。Forager は EXP-001 再実行と全行一致、Reflex OFF ≡ Forager、Random 10/10 絶滅。学習方策は絶滅なしで Forager の帯域を**上回った**（Reflex ON 1572、Learner 32 行 1623、Forager 1312。ADR D9 の「Learner ≤ Forager」は反証）。加害死が Learner の死因の 43%（探索 ε = 5% 由来）、Transfer は床を超えるが分かち合いではない（訪問の薄い行の同点）。`docs/research/experiments/EXP-002-learning-vs-fixed.md`、データは `data/EXP-002/`。`cz sweep` に窓の率・死因・行動構成・Q / 重みの統計・qpolicy を追加）
- [x] 3-9a UI: 社会マーカー（`Frame.social`）・台帳集計（`Frame.ledger`）・インスペクタの記憶/相互作用・世界の方策構成選択（済 2026-10-17: プロトコル v3、cz-web に `social / ledger_json / agent_json / agent_events_json` と `policy` 引数。ローカルと公開 URL で確認済み）
- [x] 3-9b UI: 交渉スレッド（済 2026-10-17: `crates/cz-web/src/thread.rs` が 2 者間の台帳を台帳順に並べ、各 `Accepted` を取引として窓内の移転から kept / open / broken を判定。`thread_json` / `deals_json`、プロトコル v4（`Watch.partner` → `Watched.thread`、`deals` 要求）、`?seed=`、インスペクタの Thread と Deals カード。判定は cz-web 側、UI は表示のみ。ブラウザは `?policy=mixed&seed=23` の #909 ↔ #363 で確認）
- [x] 3-9c UI: fixed vs learning 比較（済 2026-10-17: Branches の「Compare: fixed vs learning」= 現 tick で fork + `SetPolicy { All, kind, Default }`（B の kind は Learner 既定、Reflex / Forager / Random も可）、「Compare · A solid, B dashed」の 5 カーブ（人口・摂取量/体/tick・移転/tick・加害/tick・発信/tick、直近 50 tick の台帳差分）、介入パネルの Set policy（All / Fraction / Id × Default / Keep）、cz-web の `policy_mix` に `reflex / learner / mixed-learning`（CLI と同じ割当、evolving mix は突然変異 ON）、`agents()` の policy 列を `policy_code` 0..=3 に、地図 4 色、インスペクタに Learner の last step と Q 行、Learner 世界のゴールデン `cb8bc0a9772c09a4` を native / wasm で固定、プロトコル v5。ブラウザ計測は STATUS）
- [x] 3-10 文書（済 2026-10-17: ADR-0013 の完了判定、STATUS の Phase 3 完了判定、RISKS R8〜R11、TUTORIAL 9 節。**Phase 3 完了**）

## Phase 4: Institutions（Nomos Engine）— [ADR-0014](adr/0014-phase4-institutions.md) のタスク表に従う

- [x] Phase 4 の設計 ADR（済 2026-10-17: ADR-0014。制度は物理を変えない 2 経路（D1）、`simulation-core/src/nomos/` と `World.nomos` / `NormIndex`（D2）、先占 = 到着順と侵害・不履行の操作的定義（D3）、違反・係争・制裁の判定はカーネルの純関数（D4）、Institution DSL = `Norm` × `Charter` の最小形と表現できないこと（D5）、観測の規範情報と Reflex 21 特徴 / Learner の新ビット順 / 体力項（D6）、動議・投票・憲章・改正と台本 `Enforcer`、学習方策は立法しない（D7）、介入 Enact / Repeal / Charter と fork 比較（D8）、決定論と台帳の契約（D9）、前提条件と EXP-003 候補の採否（D10）、H1 の事前登録の形 = EXP-004（D11）、制度エクスプローラは読み手だけ（D12）。完了条件にデモと計測の門とスナップショット上限）
- [x] 4-1 社会的事実の追跡（済 2026-10-17: `src/nomos.rs`、`WorldConfig.nomos`（既定 false、`--nomos`）、`Agent.arrived`（先占 = 到着順。移動で `tick + 1`、出生は親を継承、`SpawnAgents` は介入 tick、創設個体は `None`）、Apply Phase 3 直後の侵害判定と `EncroachedBy`、約束の監査 `World.nomos.deals`（受諾 tick から期限まで、Phase 3c で決着。`thread.rs` との定義差は ADR-0014 D3 の追記）、台帳 `gathers / encroachments / promises_due / promises_broken`、`TickReport` の方策別採集・侵害・`promises_settled`・`learner_rows`、`learner_health_weight`（`--learner-health-weight`）、`cz sweep` の新列（侵害率の全体と方策別・約束・立っている食料・再生量・`visits`）、スナップショット形式 9、`tests/nomos_facts.rs` + `cz-web/tests/nomos_audit.rs`。ゴールデン 2 本不変、EXP-002 の forager / learner32 series はオン・オフとも全行一致。スモーク計測: 侵害 19〜23 件/tick（3 世界平均 21.3 > 10）→ `record_violations` の既定は OFF）
- [ ] 4-2 `nomos` モジュール: `Norm / Rule / Charter / Standing / Nomos`、`NormIndex`、3 述語の判定・係争・制裁の識別・エピソードのカウンタ、イベント `Violated / Sanctioned`（`record_violations` の既定 OFF = 4-1 の計測）、記憶 `Sanctioned / SanctionedBy / SawSanction`、介入 `Enact / Repeal / Charter` と CLI、スナップショット形式 10（9 は 4-1）、再生照合
- [ ] 4-3 観測と学習方策: `Observation.norms`、Reflex 21 特徴（零の初期重み、突然変異の順序）、Learner の新ビット順（`FINE_STATES` 1024、64〜256 行は `nomos` オフで 32 行と同一決定）、Harm のアクチュエータ、`Perception.see_standing`。EXP-002 の series が全行一致、decide ≤ 200 ns
- [ ] 4-4 EXP-003（ε × `harm_cost` × 体力項の格子 + Forager + Learner 128 行、14 腕 × 10 シード）: 事前登録 → 実行 → `WorldConfig::nomos_preset()` の確定。死の終端報酬は却下（D10）、`--mutate` だけの Forager は Phase 6 へ
- [ ] 4-5 提案と合意: `Message::Motion / Vote`、議場 1 つと憲章（定足数・閾値・期限・改正の可否）、閉会と施行・置換、`Failure` 5 種、Random の 16 行動、`PolicyKind::Enforcer`（陽性対照の台本）、`--policy institution`
- [ ] 4-6 EXP-004（H1: 先占規範と制裁。L0 / L1 / LE0 / LE + 帰無 5 本、主対比 30 シード）: 事前登録 → 実行 → 記録。「創発」「世界初」を書かない
- [ ] 4-7 ∥ cz-web の読み手: `institution_json / timeline_json / violations_json / sanctions_json`、`agents()` の `flags` 列、`Frame.institution`、`watch` の係争、プロトコル v6、モック worker（4-2 の型が決まれば先行可）
- [ ] 4-8 ∥ 制度エクスプローラ: 時系列カード、違反と制裁の一覧、率のカーブ、介入パネル Enact / Repeal / Charter、「Compare: with vs without rule」、インスペクタの係争、地図のマーカー、`institution` ミックス、ツアーの段、TUTORIAL §10（PRODUCT.md Phase 4 のデモ、公開 URL で確認）
- [ ] 4-9 計測とゴールデン: 制度世界のゴールデン（native = wasm、CI）、decide / step / NormIndex / スナップショット（既定 ≤ 1 MB、制度世界 ≤ 2 MB）/ 巻き戻し（RISKS R11、学習世界と制度世界）/ ブラウザ tick/s / wasm ≤ 400 KB
- [ ] 4-10 文書: ADR-0014 の完了判定、STATUS、RISKS R12〜R14、HYPOTHESES、ARCHITECTURE（`nomos` モジュール、Apply 3c、プロトコル v6）、CLAUDE.md の `Failure` の文言、TUTORIAL §10〜11、README

## Phase 6 以降

- [ ] 突然変異 ON の事前登録実験、固定小数点 NN の検討（ADR-0013 で先送り）。
- [ ] `--mutate` だけの Forager（Reflex の `hunger_permille` 低下を重みの進化と分ける。ADR-0014 D10 で EXP-003 から外し、Phase 6 の進化実験の腕にする。他の EXP-003 候補の採否は ADR-0014 D10）。
