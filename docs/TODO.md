# TODO

優先度順。各項目は「受け入れ基準」を満たしたら STATUS.md に移す。
Phase 2 の計画は ADR-0010（Toy 1）に従う。

## Phase 1 残り（Phase 2 と並行可）

- [ ] **decide フェーズのプロファイル**: 定常状態で 150 ns/体·tick、`policy::decide` が実行時間の 82%（macOS `sample`）。視界走査（半径 4 = 41 セル）が主因と推定。基準: flamegraph を docs/ に添付し、改善案を ADR に。Toy 1 の 60 tick/s 目標に直結。
- [ ] **乱数統計検定**: PractRand または TestU01 SmallCrush を `RngStream` に対して実施（ADR-0004）。
- [ ] EXP-001 のパラメータ格子版（再生量・代謝・出生閾値を振って帯域と絶滅確率を測る）。
- [ ] `Vec<Intent>` の tick ごとの確保を避ける（`decide_all` のバッファ再利用）— 計測して効果があれば。

## Phase 2: Toy 1（ADR-0010）

- [ ] Store / Transfer 行動（イベント台帳と同時に実装）。
- [ ] イベント台帳: `Event::Acted { agent, intent, outcome }` と `Event::Intervened { … }` を追加し、tick ごとの JSON Lines に追記。
- [ ] 介入 API: `Intervention` enum（再生量変更＝干ばつ、資源量スケール、個体の追加/除去）を台帳に記録して適用。
- [ ] 分岐: スナップショットから `fork(intervention)`、同一介入の 2 回適用でハッシュ一致を検証するテスト。
- [ ] バイナリスナップショット（postcard 等を比較検討、ADR-0006 の再評価）。
- [ ] WASM パッケージ（wasm-bindgen、Web Worker 1 世界 1 ワーカー）。
- [ ] Toy 1 UI（Vite + React + Canvas）: タイムライン、介入パネル、分岐の並列表示、人口・食料カーブ。
- [ ] ブラウザでの性能計測（1,000〜2,000 体で ≥ 60 tick/s）。

## Phase 3 以降

- [ ] 他者観測（空間インデックス）、Signal 行動、簡易交渉プロトコル。
- [ ] 学習方策（表形式 RL または小型 NN）と Forager/Random の比較。
- [ ] 突然変異 ON の事前登録実験（Phase 6）。
