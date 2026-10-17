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
- [ ] 2b-6 の磨き込み（任意）: 分岐を 2 本以上、fork した B をさらに fork、B の介入履歴の表示、B 側の個体インスペクタ。
- [ ] wasm サイズ（205 KB → 3-3 で 226 KB → 3-9a で 278 KB、3-4 と合わせて 286 KB。3-9a の +52 KB は `agent_json` / `ledger_json` / `agent_events_json` の serde_json 直列化が主）: serde_json を境界から外す（介入・イベント・個体を構造体や typed array で渡す）か、`--profile wasm` 相当の設定を wasm-pack に通せば縮む見込み。計測してから。

## Phase 3: Agent Cognition（ADR-0013 のタスク表に従う）

- [x] 3-1 空間インデックス + `Observation::neighbors()` + `Perception` 設定（済 2026-10-17: ゴールデン不変、step +11〜16%）
- [x] 3-2 `Transfer` / `Store` / `Harm`、`DeathCause::Violence`、`Transferred` / `Harmed` イベント、`Acted` 記録（済 2026-10-17: ゴールデン不変を旧定義で証明、Random は社会行動も抽選）
- [x] 3-3 `Signal(Say)`、`last_signal` の観測、記憶リング `recent[K]` のカーネル更新（済 2026-10-17: `Agent.recent` は `memory` の兄弟フィールド、空なら符号化に入らずゴールデン不変、スナップショット形式 5）
- [x] 3-4 交渉メッセージ `Propose / Accept / Reject`（済 2026-10-17: `signal_radius` 内の宛先にだけ届き、受諾は記憶の `ProposalFrom` が要件、`Promised` は約束した量、`Proposed / Accepted / Rejected` を常時記録。履行は強制しない。スナップショット形式 6、ゴールデン不変）
- [ ] 3-5 `PolicyKind::{Reflex, Learner}` の枠、`Intervention::SetPolicy`、CLI の方策拡張
- [ ] 3-6 Reflex 方策（固定小数点、Forager 等価の初期重み、突然変異 ON）
- [ ] 3-7 Learner 方策（表形式 Q、整数、継承）
- [ ] 3-8 EXP-002 事前登録 → 実行 → 記録
- [x] 3-9a UI: 社会マーカー（`Frame.social`）・台帳集計（`Frame.ledger`）・インスペクタの記憶/相互作用・世界の方策構成選択（済 2026-10-17: プロトコル v3、cz-web に `social / ledger_json / agent_json / agent_events_json` と `policy` 引数。ローカルと公開 URL で確認済み）
- [ ] 3-9b/c UI: 交渉スレッド（`Proposed / Accepted / Rejected / Transferred` の時系列。`agent_events_json` が材料）、fixed vs learning 比較
- [ ] 3-10 文書（ゴールデン表、STATUS、RISKS 追補、TUTORIAL 追補）

## Phase 6 以降

- [ ] 突然変異 ON の事前登録実験、固定小数点 NN の検討（ADR-0013 で先送り）。
