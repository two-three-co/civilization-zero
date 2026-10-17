# Architecture（Phase 3 完了時点）

> 実装と一致している内容のみを書く。未実装の層は「予定」と明記する。製品ゴールは docs/PRODUCT.md。

## 層構造と依存方向

```
viewer/ (static HTML)      ← trace JSON / wasm を読むだけ。カーネルに依存しない
web/ (Vite + React + TS)   ← Toy 1 UI。Worker 経由で cz-web の wasm を駆動。カーネルの型には依存しない
crates/cz-web  (cdylib)    ← wasm-bindgen 境界。multiverse と simulation-core に依存。数値は u32、ハッシュは hex 文字列
crates/cz-cli  (bin: cz)   ← 実験ランナー。simulation-core と multiverse に依存
crates/multiverse          ← Multiverse Runtime: 再生・分岐・系譜（ADR-0012）。simulation-core にのみ依存
crates/cz-wasm-probe       ← wasm32 可否の計測用 C-ABI（ADR-0009）。製品 API ではない
crates/simulation-core     ← Deterministic World Kernel。外部依存は serde / serde_json / postcard のみ
                              (rayon は feature "parallel" のときだけ)
```

下位層は上位層を知らない。Nomos Engine / Genesis は**未実装**で、独立クレートとして simulation-core の上に載せる予定。

## Rust ワークスペース

| クレート | 役割 | 状態 |
|---|---|---|
| `simulation-core` | 空間・資源・身体・行動解決・ライフサイクル・乱数・スナップショット | 実装済み |
| `cz-cli` | `cz run` / `cz resume` / `cz sweep`、計測、trace 出力 | 実装済み |
| `cz-wasm-probe` | カーネルを wasm32 で動かす最小 C-ABI（`probe_init/step/hash`） | 実装済み（計測用） |
| `multiverse` | 再生（再実行 + 台帳照合）、`Multiverse` / `Branch`（head + 台帳 + 周期スナップショット）、fork と系譜 | 実装済み（ADR-0012） |
| `web/`（npm, 非 Cargo） | Toy 1 UI: Vite + React + TS、UI ↔ Worker プロトコル（v5）、カーネル Worker（wasm）とモック Worker、社会マーカーとインスペクタ、交渉スレッドと取引の一覧、「Compare: fixed vs learning」（fork + `SetPolicy`）と A/B の率カーブ（ADR-0013 3-9a/b/c） | 実装済み（2b-1/2b-2、3-9a/b/c） |
| `cz-web` | wasm-bindgen パッケージ: `WasmWorld` = `Multiverse` + cursor（巻き戻し用の復元ビュー）。社会層の読み出し `social / ledger_json / agent_json / agent_events_json`、方策構成付きコンストラクタ（`policy_mix`: forager / random / mixed / reflex / learner / mixed-learning、`cz run --policy` と同じ割当と突然変異の既定）、`agents()` の policy 列は `policy_code`（0..=3）、交渉スレッドの読み手 `thread.rs`（`thread_json / deals_json`: 2 者間の台帳と、各受諾を窓内の移転から kept / open / broken と判定。表示のための読み方で世界には何も返さない） | 実装済み（2b-1、3-9a/b/c） |
| （`simulation-core/src/policy/`） | 方策は遷移関数の一部なので別クレートにしない（ADR-0013 D1）。`policy.rs` が根（Forager / Random / Reflex と共通のアクチュエータ）、`policy/learner.rs` が Q 学習者（3-7）。他者観測・相互作用・記憶・通信もカーネル内 | 実装済み（3-6, 3-7）。forager / random / reflex のファイル分割は未 |
| `nomos` | Institution DSL / 解釈器 | Phase 4 |

## カーネルのデータモデル

- **World** = `config` + `tick` + `ResourceGrid` + `Vec<Agent>`（id 昇順を不変条件として維持）+ `next_agent_id`（単調増加、再利用なし）+ `Ledger`。
- **空間**: 有界の整数グリッド（`u16` 座標）、4近傍、境界は壁。1セルに何体でも入れる（Phase 1 では移動衝突なし）。
- **資源**: SoA（`food: Vec<u32>`, `capacity: Vec<u32>`）。パッチ生成、`regrow_interval` ごとに定量再生。
- **Agent**: `id, pos, energy(milli), health, inventory, age, policy, params, memory{target}, last_signal, recent, last_step`。`policy` は `Forager / Random / Reflex / Learner`（Reflex は 3-6: `policy::feature` の 16 特徴の線形スコアと重みの突然変異、初期重みは Forager 等価。Learner は 3-7: `policy::learner` の 7 ビット状態（hungry / has_food / food_here / food_in_sight / adjacent / upper / heard）を `learner_states` 行に折った Q 表の ε-greedy、ε は config `learner_epsilon_permille`）。`params` は `PolicyParams { hunger_permille, table: Option<PolicyTable> }` で、`table` は Reflex の重み（16 × 8 の i32 固定小数点）か Learner の Q 表（`learner_states` × 8 の i16、固定小数点 16 = 報酬 1 単位 = 100 ミリエネルギー）、固定方策では `None`（ハッシュにバイトを足さない）。`policy` と `table` の整合は `check_invariants` が検査する。`memory` は方策が所有し `Intent` で返す。`last_signal`（最後の `Say` と tick）と `recent`（直近 `memory_slots` 件の相互作用 `{tick, other, kind, value}`、oldest-first。`kind` = GaveTo / ReceivedFrom / Harmed / HarmedBy / SawHarm / Heard / ProposalFrom / Accepted / Rejected / Promised）はカーネルだけが書く（ADR-0013 D4, D5）。`last_step: Option<LearnerStep { state, action, reward }>` は Learner の前 tick の決定（Q 表の行・行動の種類・その tick の報酬）で、カーネルが Apply で書き、次の tick の Apply で Q を更新するのに使う。他の方策・新生児・`SetPolicy` 直後は `None`（ハッシュにバイトを足さない。ADR-0013 D6 3-7）。交渉メッセージ（`Message::Propose / Accept / Reject`）は `signal_radius` 内の宛先の `recent` にだけ届き、`Accept` は宛先が記憶している提案（`ProposalFrom`）にしか成立しない。約束の履行は強制されない（返済は通常の `Transfer`）。
  `params` は親から継承（`table` も継承。Learner の子は `learner_inherit_q` が false なら白紙の Q で始まる）、突然変異は既定 OFF で `hunger_permille` と Reflex の重みを動かし、Q 表には触れない（ADR-0008、ADR-0013 D6）。
- **Ledger**: 食料・エネルギーの流入/流出、出生数、死因別死亡数を累計し、保存則を任意の tick で検証可能にする。
- **Event**: `Born` / `Died` / `Intervened` / `Transferred` / `Harmed` / `Proposed` / `Accepted` / `Rejected` / `Signaled` / `Acted` の自己完結レコード（tick 入り、serde 内部タグ）。`Signaled` は `record_signals` のときだけ、`Proposed / Accepted / Rejected`（届いた交渉メッセージ。`Accepted` は契約条件 `give / want / by` と `offer_tick` を複写）は常に。`Intervened` は `World::intervene()` の戻り値、他は `TickReport.events`。`Acted`（全個体の行動と結果）は `record_actions` のときだけ。Phase 2 の台帳形式そのもので、Phase 4 の違反判定（不履行 = `Accepted` に対応する `Transferred` が期限までにない）の材料。
- **EventLog**（ADR-0010 2a-2）: 追記専用の `Vec<Event>`。tick 非減少を検証、`range(from..to)` は二分探索。JSON Lines（先頭にヘッダ行 `{"format":1,"kernel":…}`）の書き出し/読み戻しは `Write` / `BufRead` を受け取り、カーネルはファイルを開かない。
- **Intervention**（ADR-0010 2a-1、ADR-0013 D7）: `Drought` / `ScaleFood` / `SpawnAgents` / `RemoveAgent` / `SetPolicy { who: All | Id | Fraction { permille }, kind, params: Default | Keep }`。tick の合間に `World::intervene()` で適用。無効なら状態を変えずに `Err`。乱数は `SetPolicy` の `Fraction` が `Stream::Intervention` を `(seed, tick, id)` で引くのが唯一（再生で同じ集合になる）。食料・エネルギーの差分は台帳の `food_intervened` / `energy_intervened` に計上され、保存則の検査式に入る。

ECS ライブラリは使わない（ADR-0001）。

## 1 tick の実行順序（ADR-0002, ADR-0008）

```
S(t) ──Index──▶ SpatialIndex（派生状態、保存しない。Decide と Apply で共有）
     ──Decide(純関数, 並列化可)──▶ Intent[]  ──Apply──▶ S(t+1)
         ↑ RngStream(seed,tick,agent,Decide)、Observation::neighbors() は Index を読むだけ（前 tick の発信は Neighbor.signal）
           Learner の Intent は決定した Q 表の行（Intent.state）も運ぶ。方策は表を読むだけで、更新は Apply の 5a
Apply:
  2 Consume   在庫→エネルギー（私的、競合なし）
  3 Gather    S(t) の位置で解決。同一セルの請求者を抽選キー順に逐次配分
  3b Transfer / Store / Harm / Signal   S(t) の位置で隣接判定（Manhattan ≤ 1）、行為者 id 順に逐次解決。Say の聴取者は signal_radius 内。
             Propose / Accept / Reject は signal_radius 内の宛先にだけ届く（外なら OutOfEarshot、不在なら NoSuchTarget）。Accept は宛先からの提案を
             受諾者が記憶していることが要件（なければ NoSuchOffer）。届いたものは Proposed / Accepted / Rejected として常に台帳へ（ADR-0013 D3, D5）
  4 Move      同時適用（セル容量無制限なので衝突しない）
  5 Upkeep    代謝+行動コスト → Rest 回復 → 飢餓ダメージ → 加齢
  5a Learn    Learner: 前 tick の記録 last_step {s, a, r} を今 tick の行 s'（Intent.state）で更新 Q[s,a] += (r·16 + 29·max Q[s'] >> 5 − Q[s,a]) >> 4、
             今 tick の記録 {s', 行動の種類, Upkeep までのエネルギー差分 /100} を書く（出生の移転は含まない。ADR-0013 D6 3-7）
  5b Memory   その tick の移転・加害・目撃（被害者から social_radius 内）・聴取・提案（ProposalFrom）・受諾（受諾者と提案者に Promised、提案者に Accepted）・
             拒否（Rejected）を起きた順に各個体の recent へ（上限 memory_slots、カーネルのみが書く。ADR-0013 D4）
  6 Death     health<=0 はその tick に加害を受けていれば Violence、そうでなければ Starvation。それ以外で age>=max_age は OldAge。食料はセルへ、エネルギーは償却
  7 Birth     生存者で energy>=birth_threshold の個体を id 順に処理。子 id は next_agent_id から連番、末尾に追加
  8 Env       regrow_interval ごとに再生
```

行動ごとの事前条件・コスト・失敗条件は [`action.rs`](../crates/simulation-core/src/action.rs) 冒頭の表を正とする。
`Failure` は**物理的実行不能**のみを表す（届かないメッセージ `OutOfEarshot`、受け取っていない提案の受諾 `NoSuchOffer` もここ）。約束の不履行は `Failure` ではなく台帳から読む事実。規則違反（Phase 4）は別概念として導入する。

## 乱数（ADR-0004）

共有 RNG を持たない。各抽選は `(seed, tick, entity, stream)` の純関数（SplitMix64 系のカウンタ型）。
ストリーム: WorldGen / AgentSpawn / Decide / GatherPriority / Mutation。エージェントの処理順・スレッド数に結果が依存しない。

## 状態遷移の再現性

保証範囲: 同一クレートバージョン + 同一 `WorldConfig`（seed 含む）→ 同一状態列。
根拠: 浮動小数点なし、HashMap 走査なし、乱数は座標関数、並列 decide は順序保存 collect。
検証: `tests/determinism.rs`（同一シード比較、ゴールデンハッシュ、debug/release 両方で同値、serial=parallel）。
ゴールデンは `World::dynamics_hash`（`state_hash` から config を除いたもの）に対して取る。config のフィールド追加は符号化の変更であって遷移の変更ではないため。
wasm32 と native の一致は ADR-0009 で計測し、CI（`wasm-pack test --node`）で照合し続けている。

## シリアライズ（ADR-0006）

- スナップショット: `{format: 8, state_hash, world}` を JSON（可読）またはバイナリ（マジック `CZS\0` + postcard、実行時の既定）で。形式番号は `World` の直列化形が変わるたびに上げ、旧版は `Format(n)` で拒否する。読み込み時に形式版 → config 検証 → 不変条件 → ハッシュ照合（両形式共通）。
- 再生と分岐（`multiverse`）: `state_at(T)` は「tick T の開始状態」で、最寄りのスナップショットから台帳の介入を適用しつつ再実行し、出生・死亡を台帳と照合する。分岐は親の開始状態 + 子の介入、台帳は過去をコピー。
- `state_hash`: 正準エンコーディング上の FNV-1a 64bit。`std::hash` には依存しない。
- trace（ビューア用, format 2）: `cz run --trace` が出す軽量形式。カーネルの公開形式ではない。

## フロントエンド通信

Phase 1: ファイル（trace JSON）のみ。ビューアは静的 HTML（ADR-0007）。
Phase 2（Toy 1）: カーネルを wasm としてブラウザ内で実行し、Web Worker 1 つ = 世界 1 つ（`WasmWorld`）。サーバーは置かない（ADR-0009, 0010）。
UI ↔ Worker は `web/src/protocol.ts`（v5: `init / step / seek / frame / intervene / history / snapshot / deals`、各要求に現フレームで応答。フレームは食料 `Uint8Array` + 個体 `Int32Array`（policy 列は `PolicyKind` の 0..=3）+ 社会マーカー `Int32Array`（直前 tick の移転・加害・発信、7 値/件）を Transferable で転送し、台帳の累計 `ledger` と Worker の計測値（step ms、seek ms）を載せる）。介入は台帳の `Intervention` と同じ JSON（`set_policy` を含む）で、履歴は台帳から読み戻す。
要求には `watch { id, window, partner?, threadWindow? }` を添えられ、Worker はその個体の `Agent`（カーネルが書いた `recent` と `last_signal` 込み）と、直近 `window` tick の台帳からその個体を名指しする出来事を `Frame.watched` に同梱する（ADR-0013 D8。UI は個体について何も覚えない）。`partner` があれば 2 者間の交渉スレッド `Watched.thread`（`WasmWorld::thread_json`: 台帳順のイベントと、各受諾の kept / open / broken 判定）も載る（3-9b）。`deals` 要求は窓内の全取引を同じ判定で返す（スレッドを探すため、フレームには載せない）。`init` の `policy`（`forager / random / mixed / reflex / learner / mixed-learning`）は `cz run --policy` と同じ割当規則（evolving な mix は突然変異 ON）、`seed` は `?seed=`（既定 42）。「Compare: fixed vs learning」（3-9c）は fork + `set_policy { all, kind, default }` で、UI は A / B のフレームが運ぶ台帳の累計から人口・摂取量/体・移転・加害・発信の率カーブを描くだけ（新しい要求はない）。
分岐はブランチごとに Worker 1 つ: 親が現在の tick のバイナリスナップショットを書き出し、子は `init({ snapshot })`（`WasmWorld::fromSnapshot`）で原点にして介入を適用する。UI は 2 つの `WorldClient` を同じ tick 数だけ進め、同じ tick へシークする。
巻き戻しは Worker 内の cursor（`state_at` の復元ビュー）で、head 以前の `step` は台帳照合付きの再実行。

## テスト構成

| 種類 | 場所 | 内容 |
|---|---|---|
| 単体 | `src/*.rs` の `#[cfg(test)]` | RNG の一様性・独立性、壁、近傍、突然変異の範囲と決定性 |
| シナリオ | `tests/actions.rs` | 競合採集・二重取得なし、壁移動、消費上限、飢餓死、再生 |
| ライフサイクル | `tests/lifecycle.rs` | 出生の分裂と親 id 順採番、死因の優先、死者は繁殖しない、継承と突然変異、イベントの直列化 |
| 決定論/統合 | `tests/determinism.rs` | 同一シード同一履歴、ゴールデンハッシュ、スナップショット往復、改竄検出、5,000 tick 不変条件、世代をまたぐ保存則、serial=parallel |
| 社会層 | `tests/neighbors.rs`, `tests/interaction.rs`, `tests/memory.rs`, `tests/negotiation.rs` | 近傍観測、Transfer / Store / Harm、発信と記憶、交渉（提案 → 受諾 → 移転 → 返済の台帳と記憶、不履行で物理不変、届かない・不在・未受領の失敗、proptest で serial = parallel） |
| 方策の枠 | `tests/set_policy.rs`, `multiverse/tests/fork.rs` | Reflex / Learner の表（Reflex は Forager と身体が 600 tick 一致、Learner は自分の履歴・非零の表・記録を持つ）、`SetPolicy` の All / Id / Fraction と Default / Keep、無効介入の拒否、JSON 形、Q の継承 / 白紙、形式 8 の往復と不整合の拒否、同一 tick の 2 回 fork でハッシュ一致 |
| Reflex 方策 | `tests/reflex.rs`, `policy.rs` 単体 | Forager 等価（全個体の決定一致、1,000 tick の身体・資源・台帳一致）、特徴ベクトル、決定規則、重みが決定を変える、同点抽選の一様性と決定性、子の重みの突然変異、serial = parallel（proptest） |
| Learner 方策 | `tests/learner.rs`, `policy/learner.rs` 単体, `multiverse/tests/replay.rs` | 状態ビットと空腹 4 段階の境界、行に従う決定と 8 種のアクチュエータ、ε の一様性と ε = 0、カーネルの更新値（前 tick の報酬と次行の最大）、出生の移転は罰にならない、Rest の自己ループで Q が単調減少、`SetPolicy` が連鎖を切る、往復と不整合の拒否、継承と行数、Forager 世界に記録なし + ゴールデン不変、serial = parallel = 再実行（proptest）、単体: 固定点への単調収束・i16 飽和・境界・行の折り畳み、再生: Learner 世界の `state_at` と再生が直接実行と bit 一致 |
| プロパティ | `tests/determinism.rs` (proptest) | 任意の config（ライフサイクル ON/OFF、突然変異 ON/OFF）で毎 tick 不変条件、消費なしでエネルギー増加なし、再実行一致 |
| ベンチ | `benches/step.rs` (criterion) | 1k/10k/100k 体の step（1,000 tick 後の定常状態）、ハッシュ、スナップショット |
| 計測 | `cz sweep` | 複数シードの個体数帯域（docs/research/experiments/EXP-001） |
| 共通ヘルパ | `tests/common/mod.rs` | 手組みの世界と明示的 Intent |
