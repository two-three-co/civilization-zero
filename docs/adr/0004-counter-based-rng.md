# ADR-0004: カウンタ型（座標関数）乱数

- 状態: 採用 (2026-10-17)

## 決定
共有の RNG 状態を持たず、抽選値を `mix64(seed, tick, entity, stream, counter)` の純関数として得る（SplitMix64 ファイナライザの連鎖）。`Stream` は用途タグ（WorldGen / AgentSpawn / Decide / GatherPriority）。

## 理由
- 共有 RNG だと「誰が先に引いたか」で値が変わり、並列化・コード変更で履歴がずれる。
- 座標関数なら、ある用途に抽選を追加しても他の用途の値は不変（変更の局所性）。fork 時に RNG 状態を保存する必要もない。

## 検討
- `rand_chacha`: 暗号強度で品質は上だが、ストリーム分割を自前で設計する点は同じ。依存を避けた。
- Philox / Threefry: 同等の設計。SplitMix64 で統計品質が不足すると判明したら置換する（`rng.rs` のみの変更、ゴールデンハッシュは更新）。

## 限界
SplitMix64 系は BigCrush を通過するとされるが、本実装では統計検定を実施していない（一様性の簡易テストのみ）。研究結果を報告する前に PractRand 等で確認する（TODO）。
