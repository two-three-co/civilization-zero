# TODO

優先度順。各項目は「受け入れ基準」を満たしたら STATUS.md に移す。
Phase 2 の計画は ADR-0010（Toy 1）に従う。

## Phase 1 残り（Phase 2 と並行可）

- [ ] `apply` の採集請求ソート（実行時間の約 7%、ADR-0011 後）をセル単位のバケットに置換 — 計測して効果があれば。
- [ ] **乱数統計検定**: PractRand または TestU01 SmallCrush を `RngStream` に対して実施（ADR-0004）。
- [ ] EXP-001 のパラメータ格子版（再生量・代謝・出生閾値を振って帯域と絶滅確率を測る）。
- [ ] `Vec<Intent>` の tick ごとの確保を避ける（`decide_all` のバッファ再利用）— 計測して効果があれば。

## Phase 2: Toy 1（ADR-0010）

- [ ] Store / Transfer 行動（イベント台帳と同時に実装）。
- [ ] `Event::Acted { agent, intent, outcome }` の記録（2a-3、Store/Transfer と同時。全行動記録は設定で ON/OFF、台帳サイズを計測）。
- [ ] 2b-6 のために `WasmWorld` に fork API（cursor 位置での介入 = 新ブランチ）を追加し、Worker 1 つ = `Multiverse` 1 つ（複数ブランチ）か、ブランチごとに Worker かを決める。
- [ ] 台帳のバイナリ形式（postcard は内部タグ付き enum を復号できないので別ワイヤ型が要る。JSON Lines で足りている間はやらない）。
- [ ] Toy 1 UI の続き（2b-3〜2b-7）: 実測 tick/s の表示、巻き戻し ≤ 200 ms の計測表示、介入パネル、分岐の並列表示、静的配信。
- [ ] wasm サイズ（205 KB）: serde_json を境界から外す（介入とイベントを構造体で渡す）か、`--profile wasm` 相当の設定を wasm-pack に通せば縮む見込み。計測してから。
- [ ] ブラウザでの性能計測（1,000〜2,000 体で ≥ 60 tick/s）。

## Phase 3 以降

- [ ] 他者観測（空間インデックス）、Signal 行動、簡易交渉プロトコル。
- [ ] 学習方策（表形式 RL または小型 NN）と Forager/Random の比較。
- [ ] 突然変異 ON の事前登録実験（Phase 6）。
