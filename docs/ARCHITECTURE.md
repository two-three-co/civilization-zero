# Architecture（Phase 1 完了時点）

> 実装と一致している内容のみを書く。未実装の層は「予定」と明記する。製品ゴールは docs/PRODUCT.md。

## 層構造と依存方向

```
viewer/ (static HTML)      ← trace JSON / wasm を読むだけ。カーネルに依存しない
crates/cz-cli  (bin: cz)   ← 実験ランナー。simulation-core に依存
crates/cz-wasm-probe       ← wasm32 可否の計測用 C-ABI（ADR-0009）。製品 API ではない
crates/simulation-core     ← Deterministic World Kernel。外部依存は serde / serde_json のみ
                              (rayon は feature "parallel" のときだけ)
```

下位層は上位層を知らない。Multiverse Runtime / Nomos Engine / Genesis は**未実装**で、
それぞれ独立クレート（`multiverse`, `nomos`, …）として simulation-core の上に載せる予定。

## Rust ワークスペース

| クレート | 役割 | 状態 |
|---|---|---|
| `simulation-core` | 空間・資源・身体・行動解決・ライフサイクル・乱数・スナップショット | 実装済み |
| `cz-cli` | `cz run` / `cz resume` / `cz sweep`、計測、trace 出力 | 実装済み |
| `cz-wasm-probe` | カーネルを wasm32 で動かす最小 C-ABI（`probe_init/step/hash`） | 実装済み（計測用） |
| `multiverse` | イベント台帳・介入・fork | Phase 2 |
| `cz-web` | wasm-bindgen パッケージ + Toy 1 UI | Phase 2 |
| `cognition` | 学習する方策・記憶・通信 | Phase 3 |
| `nomos` | Institution DSL / 解釈器 | Phase 4 |

## カーネルのデータモデル

- **World** = `config` + `tick` + `ResourceGrid` + `Vec<Agent>`（id 昇順を不変条件として維持）+ `next_agent_id`（単調増加、再利用なし）+ `Ledger`。
- **空間**: 有界の整数グリッド（`u16` 座標）、4近傍、境界は壁。1セルに何体でも入れる（Phase 1 では移動衝突なし）。
- **資源**: SoA（`food: Vec<u32>`, `capacity: Vec<u32>`）。パッチ生成、`regrow_interval` ごとに定量再生。
- **Agent**: `id, pos, energy(milli), health, inventory, age, policy, params, memory{target}`。
  `params`（`PolicyParams { hunger_permille }`）は親から継承、突然変異は既定 OFF（ADR-0008）。
- **Ledger**: 食料・エネルギーの流入/流出、出生数、死因別死亡数を累計し、保存則を任意の tick で検証可能にする。
- **Event**: `Born` / `Died` / `Intervened` の自己完結レコード（tick 入り、serde 内部タグ）。前 2 つは `TickReport.events` で、`Intervened` は `World::intervene()` の戻り値で返す。Phase 2 の台帳形式そのもの。
- **Intervention**（ADR-0010 2a-1）: `Drought` / `ScaleFood` / `SpawnAgents` / `RemoveAgent`。tick の合間に `World::intervene()` で適用。乱数を使わず、無効なら状態を変えずに `Err`。差分は台帳の `food_intervened` / `energy_intervened` に計上され、保存則の検査式に入る。

ECS ライブラリは使わない（ADR-0001）。

## 1 tick の実行順序（ADR-0002, ADR-0008）

```
S(t) ──Decide(純関数, 並列化可)──▶ Intent[]  ──Apply──▶ S(t+1)
         ↑ RngStream(seed,tick,agent,Decide)
Apply:
  2 Consume   在庫→エネルギー（私的、競合なし）
  3 Gather    S(t) の位置で解決。同一セルの請求者を抽選キー順に逐次配分
  4 Move      同時適用（セル容量無制限なので衝突しない）
  5 Upkeep    代謝+行動コスト → Rest 回復 → 飢餓ダメージ → 加齢
  6 Death     health<=0 は Starvation、それ以外で age>=max_age は OldAge。食料はセルへ、エネルギーは償却
  7 Birth     生存者で energy>=birth_threshold の個体を id 順に処理。子 id は next_agent_id から連番、末尾に追加
  8 Env       regrow_interval ごとに再生
```

行動ごとの事前条件・コスト・失敗条件は [`action.rs`](../crates/simulation-core/src/action.rs) 冒頭の表を正とする。
`Failure` は**物理的実行不能**のみを表す。規則違反（Phase 4）は別概念として導入する。

## 乱数（ADR-0004）

共有 RNG を持たない。各抽選は `(seed, tick, entity, stream)` の純関数（SplitMix64 系のカウンタ型）。
ストリーム: WorldGen / AgentSpawn / Decide / GatherPriority / Mutation。エージェントの処理順・スレッド数に結果が依存しない。

## 状態遷移の再現性

保証範囲: 同一クレートバージョン + 同一 `WorldConfig`（seed 含む）→ 同一状態列。
根拠: 浮動小数点なし、HashMap 走査なし、乱数は座標関数、並列 decide は順序保存 collect。
検証: `tests/determinism.rs`（同一シード比較、ゴールデンハッシュ、debug/release 両方で同値、serial=parallel）。
wasm32 と native の一致は ADR-0009 で計測。

## シリアライズ（ADR-0006）

- スナップショット: `{format: 3, state_hash, world}` の JSON。読み込み時に config 検証 → 不変条件 → ハッシュ照合。
- `state_hash`: 正準エンコーディング上の FNV-1a 64bit。`std::hash` には依存しない。
- trace（ビューア用, format 2）: `cz run --trace` が出す軽量形式。カーネルの公開形式ではない。

## フロントエンド通信

Phase 1: ファイル（trace JSON）のみ。ビューアは静的 HTML（ADR-0007）。
Phase 2（Toy 1）: カーネルを wasm としてブラウザ内で実行し、Web Worker 1 つ = 世界 1 つ。サーバーは置かない（ADR-0009, 0010）。

## テスト構成

| 種類 | 場所 | 内容 |
|---|---|---|
| 単体 | `src/*.rs` の `#[cfg(test)]` | RNG の一様性・独立性、壁、近傍、突然変異の範囲と決定性 |
| シナリオ | `tests/actions.rs` | 競合採集・二重取得なし、壁移動、消費上限、飢餓死、再生 |
| ライフサイクル | `tests/lifecycle.rs` | 出生の分裂と親 id 順採番、死因の優先、死者は繁殖しない、継承と突然変異、イベントの直列化 |
| 決定論/統合 | `tests/determinism.rs` | 同一シード同一履歴、ゴールデンハッシュ、スナップショット往復、改竄検出、5,000 tick 不変条件、世代をまたぐ保存則、serial=parallel |
| プロパティ | `tests/determinism.rs` (proptest) | 任意の config（ライフサイクル ON/OFF、突然変異 ON/OFF）で毎 tick 不変条件、消費なしでエネルギー増加なし、再実行一致 |
| ベンチ | `benches/step.rs` (criterion) | 1k/10k/100k 体の step（1,000 tick 後の定常状態）、ハッシュ、スナップショット |
| 計測 | `cz sweep` | 複数シードの個体数帯域（docs/research/experiments/EXP-001） |
| 共通ヘルパ | `tests/common/mod.rs` | 手組みの世界と明示的 Intent |
