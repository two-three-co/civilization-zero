# Architecture (Phase 1 時点)

> 実装と一致している内容のみを書く。未実装の層は「予定」と明記する。

## 層構造と依存方向

```
viewer/ (static HTML)      ← trace JSON を読むだけ。カーネルに依存しない
crates/cz-cli  (bin: cz)   ← 実験ランナー。simulation-core に依存
crates/simulation-core     ← Deterministic World Kernel。外部依存は serde / serde_json のみ
                              (rayon は feature "parallel" のときだけ)
```

下位層は上位層を知らない。Multiverse Runtime / Nomos Engine / Genesis は**未実装**で、
それぞれ独立クレート（`multiverse`, `nomos`, …）として simulation-core の上に載せる予定。

## Rust ワークスペース

| クレート | 役割 | 状態 |
|---|---|---|
| `simulation-core` | 空間・資源・身体・行動解決・乱数・スナップショット | 実装済み |
| `cz-cli` | `cz run` / `cz resume`、計測、trace 出力 | 実装済み |
| `multiverse` | イベント台帳・fork・実験スケジューラ | Phase 2 |
| `cognition` | 学習する方策・記憶・通信 | Phase 3 |
| `nomos` | Institution DSL / 解釈器 | Phase 4 |
| `cz-server` | WebSocket 配信 | Phase 2〜5（必要になった時点） |

## カーネルのデータモデル

- **World** = `config` + `tick` + `ResourceGrid` + `Vec<Agent>`（id 昇順を不変条件として維持）+ `next_agent_id` + `Ledger`。
- **空間**: 有界の整数グリッド（`u16` 座標）、4近傍、境界は壁。1セルに何体でも入れる（Phase 1 では移動衝突なし）。
- **資源**: SoA（`food: Vec<u32>`, `capacity: Vec<u32>`）。パッチ生成、`regrow_interval` ごとに定量再生。
- **Agent**: `id, pos, energy(milli), health, inventory, age, policy, memory{target}`。
  「行動可能範囲」は `vision_radius`（観測）と移動 1 マス/tick で表現。
- **Ledger**: 食料・エネルギーの流入/流出を累計し、保存則を任意の tick で検証可能にする。

ECS ライブラリは使わない（ADR-0001）。

## 1 tick の実行順序（ADR-0002）

```
S(t) ──Decide(純関数, 並列化可)──▶ Intent[]  ──Apply──▶ S(t+1)
         ↑ RngStream(seed,tick,agent,Decide)
Apply:
  2 Consume   在庫→エネルギー（私的、競合なし）
  3 Gather    S(t) の位置で解決。同一セルの請求者を抽選キー順に逐次配分
  4 Move      同時適用（セル容量無制限なので衝突しない）
  5 Upkeep    代謝+行動コスト → Rest 回復 → 飢餓ダメージ → 加齢
  6 Death     health<=0 を除去、所持食料はセルへ、残エネルギーは台帳で償却
  7 Env       regrow_interval ごとに再生
```

行動ごとの事前条件・コスト・失敗条件は [`action.rs`](../crates/simulation-core/src/action.rs) 冒頭の表を正とする。
`Failure` は**物理的実行不能**のみを表す。規則違反（Phase 4）は別概念として導入する。

## 乱数（ADR-0004）

共有 RNG を持たない。各抽選は `(seed, tick, entity, stream)` の純関数（SplitMix64 系のカウンタ型）。
エージェントの処理順・スレッド数に結果が依存しない。

## 状態遷移の再現性

保証範囲: 同一クレートバージョン + 同一 `WorldConfig`（seed 含む）→ 同一状態列。
根拠: 浮動小数点なし、HashMap 走査なし、乱数は座標関数、並列 decide は順序保存 collect。
検証: `tests/determinism.rs`（同一シード比較、ゴールデンハッシュ、debug/release 両方で同値、serial=parallel）。

## シリアライズ（ADR-0006）

- スナップショット: `{format, state_hash, world}` の JSON。読み込み時に config 検証 → 不変条件 → ハッシュ照合。
- `state_hash`: 正準エンコーディング上の FNV-1a 64bit。`std::hash` には依存しない。
- trace（ビューア用）: `cz run --trace` が出す軽量形式。カーネルの公開形式ではない。

## フロントエンド通信

Phase 1: ファイル（trace JSON）のみ。ビューアは静的 HTML（ADR-0007）。
Phase 2 以降: `cz-server` が WebSocket で差分フレームを配信、React + Vite + PixiJS へ移行予定。

## テスト構成

| 種類 | 場所 | 内容 |
|---|---|---|
| 単体 | `src/*.rs` の `#[cfg(test)]` | RNG の一様性・独立性、壁、近傍 |
| シナリオ | `tests/actions.rs` | 競合採集・二重取得なし、壁移動、消費上限、飢餓死、再生 |
| 決定論/統合 | `tests/determinism.rs` | 同一シード同一履歴、ゴールデンハッシュ、スナップショット往復、改竄検出、5,000 tick 不変条件、serial=parallel |
| プロパティ | `tests/determinism.rs` (proptest) | 任意の config で毎 tick 不変条件、消費なしでエネルギー増加なし、再実行一致 |
| ベンチ | `benches/step.rs` (criterion) | 1k/10k/100k 体の step、ハッシュ、スナップショット |
