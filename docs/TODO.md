# TODO

優先度順。各項目は「受け入れ基準」を満たしたら STATUS.md に移す。

## Phase 1 残り

- [ ] **出生（分裂）**: `energy >= birth_threshold` で子を生成し、エネルギーを親子で分配（保存則維持、`next_agent_id` 使用）。基準: 保存則テスト通過、長期実行で個体数が定常に収束する seed がある。
- [ ] **寿命**: `age >= max_age` で死亡。基準: 死因を `TickReport` で区別。
- [ ] **Store / Transfer**: セル上の貯蔵（所有者なし）と個体間移転。基準: 食料保存則、二重移転なしのテスト。
- [ ] **Apply のプロファイル**: 100k 体で Gather ソートと視界走査の比率を計測。基準: flamegraph を docs/ に添付。
- [ ] **乱数統計検定**: PractRand または TestU01 SmallCrush を `RngStream` に対して実施（ADR-0004）。
- [ ] **GitHub リポジトリ作成と CI 実行確認**。
- [ ] Vec<Intent> の tick ごとの確保を避ける（`decide_all` のバッファ再利用）— 計測して効果があれば。

## Phase 2: Multiverse Foundations

- [ ] イベント台帳 `(tick, agent, intent, outcome)` の追記型ログ（Phase 4 の違反判定の基盤）。
- [ ] 台帳からの再生（replay）と、スナップショット+台帳での任意 tick 復元。
- [ ] Fork: スナップショットから分岐、`WorldConfig` の一部を介入として差し替え。
- [ ] 実験スケジューラ: seed × 条件の格子を並列実行、Parquet 出力。
- [ ] H4 用の合成実験（既知効果の回復）。

## Phase 3 以降

- [ ] 他者観測（空間インデックス）、Signal 行動、簡易交渉プロトコル。
- [ ] 学習方策（表形式 RL または小型 NN）と Forager/Random の比較。
- [ ] ビューアを React + Vite + PixiJS に移行、WebSocket 配信（ADR-0007）。
