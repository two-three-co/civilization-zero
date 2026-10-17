# Architecture（Phase 3 完了、Phase 4 の 4-3 時点）

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

下位層は上位層を知らない。Nomos Engine は別クレートにせず simulation-core 内のモジュール `src/nomos.rs`（+ `src/nomos/index.rs`）に置く（ADR-0014 D2。Decide が規範情報を読み Apply が判定するので遷移関数の一部）。4-1 で社会的事実の追跡（先占・侵害・約束の監査）、4-2 で規則・係争・制裁の判定と布告（`Enact / Repeal / Charter`）、4-3 で観測の規範情報（`Observation.norms` と学習方策の規範特徴 / ビット、Harm のアクチュエータ、`Perception.see_standing`）まで実装済み。動議・投票（4-5）は未。Genesis は**未実装**。

## Rust ワークスペース

| クレート | 役割 | 状態 |
|---|---|---|
| `simulation-core` | 空間・資源・身体・行動解決・ライフサイクル・乱数・スナップショット | 実装済み |
| `cz-cli` | `cz run` / `cz resume` / `cz sweep`、計測、trace 出力 | 実装済み |
| `cz-wasm-probe` | カーネルを wasm32 で動かす最小 C-ABI（`probe_init/step/hash`） | 実装済み（計測用） |
| `multiverse` | 再生（再実行 + 台帳照合）、`Multiverse` / `Branch`（head + 台帳 + 周期スナップショット）、fork と系譜 | 実装済み（ADR-0012） |
| `web/`（npm, 非 Cargo） | Toy 1 UI: Vite + React + TS、UI ↔ Worker プロトコル（v5）、カーネル Worker（wasm）とモック Worker、社会マーカーとインスペクタ、交渉スレッドと取引の一覧、「Compare: fixed vs learning」（fork + `SetPolicy`）と A/B の率カーブ（ADR-0013 3-9a/b/c） | 実装済み（2b-1/2b-2、3-9a/b/c） |
| `cz-web` | wasm-bindgen パッケージ: `WasmWorld` = `Multiverse` + cursor（巻き戻し用の復元ビュー）。社会層の読み出し `social / ledger_json / agent_json / agent_events_json`、方策構成付きコンストラクタ（`policy_mix`: forager / random / mixed / reflex / learner / mixed-learning、`cz run --policy` と同じ割当と突然変異の既定）、`agents()` の policy 列は `policy_code`（0..=3）、交渉スレッドの読み手 `thread.rs`（`thread_json / deals_json`: 2 者間の台帳と、各受諾を窓内の移転から kept / open / broken と判定。表示のための読み方で世界には何も返さない） | 実装済み（2b-1、3-9a/b/c） |
| （`simulation-core/src/policy/`） | 方策は遷移関数の一部なので別クレートにしない（ADR-0013 D1）。`policy.rs` が根（Forager / Random / Reflex と共通のアクチュエータ、`Observation` と規範の問い合わせ）、`policy/learner.rs` が Q 学習者（3-7）。他者観測・相互作用・記憶・通信もカーネル内。4-3（ADR-0014 D6）で Reflex は 21 特徴、Learner は 10 ビット、Harm は係争中の隣人を優先 | 実装済み（3-6, 3-7, 4-3）。forager / random / reflex のファイル分割は未 |
| （`simulation-core/src/nomos.rs`, `nomos/index.rs`） | Nomos Engine（ADR-0014 D2: 別クレートにしない）。4-1: `config.nomos` オンで追跡する社会的事実 = 先占（`Agent.arrived`、到着順）、侵害（先占者以外の採取。台帳のカウンタと先占者の記憶 `EncroachedBy`）、約束の監査（`World.nomos.deals`: 受諾から期限まで移転を数え、期限 tick に履行 / 不履行を決着）。4-2: Institution DSL（`Norm { kind: Encroach \| BrokenPromise \| Harm, standing_ticks, sanction: HARM \| REJECT のビット集合, except_sanction }`、`Charter`）、施行中の規則 `Rule`（kind ごとに 1 つ、id は再利用しない）、係争のエピソード `Standing`（(agent, rule) ごとに 1 つ）、判定（侵害 → `Encroach`、不履行 → `BrokenPromise`、加害 → `Harm`）、制裁の識別（係争中の個体への加害 / 拒否を `Sanctioned` と読む。最古の該当エピソードに帰属）、派生 `NormIndex`（tick 開始時の係争フラグ）、布告 `Enact / Repeal / Charter`。4-3: `Observation.norms` として Decide にも渡り、学習方策が読む（`policy.rs`）。動議・投票（4-5）は未 | 4-1〜4-3 実装済み（2026-10-17）。4-4 以降は未 |

## カーネルのデータモデル

- **World** = `config` + `tick` + `ResourceGrid` + `Vec<Agent>`（id 昇順を不変条件として維持）+ `next_agent_id`（単調増加、再利用なし）+ `Ledger` + `nomos`（`Nomos { charter, rules: Vec<Rule>, next_rule_id, motion: Option<Motion>（4-5 まで常に None）, next_motion_id, standing: Vec<Standing>, deals: Vec<OpenDeal> }`、ADR-0014 D2 / D3 / D4 / D5。`config.nomos` がオフなら `Nomos::default()` のままで `dynamics_hash` にバイトを足さない。オンなら `charter` は生成時に `config.charter`、`rules` は id 昇順で `kind` ごとに最大 1 つ、`standing` は (agent, rule) 昇順で個体は生存・規則は施行中）。
- **NormIndex**（派生、ADR-0014 D2）: tick 開始時に `SpatialIndex` の直後に `nomos.standing / rules / motion` から作る個体 index ごとのフラグ `ACCUSED / HARMABLE / REJECTABLE / VOTED`（`Vec<u8>`）、kind ごとの施行中規則、開いている動議の種類。保存も hash もしない。規則も動議もなければ長さ 0 で、問い合わせは定数時間で「なし」。構築は `standing` と `agents`（どちらも id 昇順）のマージ走査 1 回（HashMap も二分探索もなし）。`World::step` が 1 回作り、Decide（`Observation.norms`、読み取り専用）と Apply（S(t) の係争の読み: その tick の違反で新たに係争中になった個体は、その tick の制裁の対象にならない）で共有する（4-3）。
- **Observation**（ADR-0013 D2、ADR-0014 D6）: `agent / me（agents 内の添字）/ resources / bounds / config / tick / agents / index / norms`。問い合わせ `food_here() / neighbors() / crowd()` に 4-3 で規範の問い合わせが加わった: `occupied()`（自セルに自分より早く到着した他個体がいる = 採ると侵害。物理の事実で `nomos` オフなら常に偽）、`my_standing()`（自分のフラグ byte）/ `accused()` / `adjacent_violator()`（距離 1 以内で HARM 制裁を許す規則の係争中の個体、`nearest_adjacent` と同じ走査順）/ `violator_adjacent()`（その真偽だけを NormIndex のセルごとの HARMABLE 件数 `harmable_on(pos)` から走査なしで読む。特徴 `VIOLATOR_ADJACENT` と Learner のビット 6 はこちらを、Harm のアクチュエータは id の要る `adjacent_violator()` を読む。4-9a）/ `rule(kind)` / `open_motion()` / `i_voted()`。後者 7 つは `config.perception.see_standing`（既定 true）が偽なら常に「なし」（EXP-004 の盲目の帰無モデル。台本方策は `obs.norms` を直接読める）。
- **空間**: 有界の整数グリッド（`u16` 座標）、4近傍、境界は壁。1セルに何体でも入れる（Phase 1 では移動衝突なし）。
- **資源**: SoA（`food: Vec<u32>`, `capacity: Vec<u32>`）。パッチ生成、`regrow_interval` ごとに定量再生。
- **Agent**: `id, pos, energy(milli), health, inventory, age, policy, params, memory{target}, last_signal, recent, last_step, arrived`。`arrived: Option<u64>`（ADR-0014 D3）は今のセルに立ち始めた tick = 先占の順位: Apply Phase 4 の移動で `tick + 1`、出生は親の値を継承（家族は共同の先占者）、`SpawnAgents` は介入の tick、創設個体は `None`（最古。`None < Some`）。`config.nomos` がオフなら常に `None` で、`Some` のときだけハッシュに入る。方策は `Observation::occupied()`（Reflex の `OCCUPIED`、Learner のビット 5）として読む（4-3）。`policy` は `Forager / Random / Reflex / Learner`（Reflex は 3-6 + 4-3: `policy::feature` の 21 特徴（16 の身体・視界・隣人・聴取・記憶 + `OCCUPIED / RULE_ACTIVE / ACCUSED / VIOLATOR_ADJACENT / SANCTIONED_RECENTLY`）の線形スコアと重みの突然変異、初期重みは Forager 等価で規範特徴の行は零。Learner は 3-7 + 4-3: `policy::learner` の 10 ビット状態（hungry / has_food / food_here / food_in_sight / adjacent / occupied / violator_adjacent / accused / upper / heard、重要な順）を `learner_states` 行に折った Q 表の ε-greedy、既定 32 行は下位 5 ビット、ε は config `learner_epsilon_permille`。両者のアクチュエータは共通で、Harm は `adjacent_violator()` を優先し、なければ最寄りの隣接個体）。`params` は `PolicyParams { hunger_permille, table: Option<PolicyTable> }` で、`table` は Reflex の重み（21 × 8 = 168 の i32 固定小数点）か Learner の Q 表（`learner_states` × 8 の i16、固定小数点 16 = 報酬 1 単位 = 100 ミリエネルギー）、固定方策では `None`（ハッシュにバイトを足さない）。`policy` と `table` の整合は `check_invariants` が検査する。`memory` は方策が所有し `Intent` で返す。`last_signal`（最後の `Say` と tick）と `recent`（直近 `memory_slots` 件の相互作用 `{tick, other, kind, value}`、oldest-first。`kind` = GaveTo / ReceivedFrom / Harmed / HarmedBy / SawHarm / Heard / ProposalFrom / Accepted / Rejected / Promised / EncroachedBy / Sanctioned / SanctionedBy / SawSanction）はカーネルだけが書く（ADR-0013 D4, D5。`EncroachedBy { other: 採集者, value: 量 }` は ADR-0014 D3、`nomos` オンの世界で先占者にだけ書かれ、他の記憶と `memory_slots` を分け合う。`Sanctioned / SanctionedBy / SawSanction` は ADR-0014 D4: 加害が制裁なら `Harmed / HarmedBy / SawHarm` の代わりに、拒否が制裁なら宛先の `Rejected` の代わりに `SanctionedBy`（value 0）と行為者に `Sanctioned`（value 0）。規則がなければ従来どおり）。`last_step: Option<LearnerStep { state, action, reward }>` は Learner の前 tick の決定（Q 表の行・行動の種類・その tick の報酬）で、カーネルが Apply で書き、次の tick の Apply で Q を更新するのに使う。他の方策・新生児・`SetPolicy` 直後は `None`（ハッシュにバイトを足さない。ADR-0013 D6 3-7）。交渉メッセージ（`Message::Propose / Accept / Reject`）は `signal_radius` 内の宛先の `recent` にだけ届き、`Accept` は宛先が記憶している提案（`ProposalFrom`）にしか成立しない。約束の履行は強制されない（返済は通常の `Transfer`）。
  `params` は親から継承（`table` も継承。Learner の子は `learner_inherit_q` が false なら白紙の Q で始まる）、突然変異は既定 OFF で `hunger_permille` と Reflex の重みを動かし、Q 表には触れない（ADR-0008、ADR-0013 D6）。
- **Ledger**: 食料・エネルギーの流入/流出、出生数、死因別死亡数を累計し、保存則を任意の tick で検証可能にする。4-1 で `gathers`（成功した採集件数、常時）、`encroachments`、`promises_due` / `promises_broken`（取引単位。`nomos` オンのみ）。4-2 で `NormKind` ごとの `violations`（判定した違反。記録の有無に関係なく）/ `sanctions` / `episodes_closed` / `episodes_sanctioned`（期限・死亡・除去・廃止で閉じたエピソードと、そのうち制裁が 1 件以上あったもの）。H1 の `S = episodes_sanctioned / episodes_closed`、`S' = sanctions / violations`、`E = encroachments / gathers` はここから出す。台帳のカウンタは `state_hash` にだけ入る。
- **TickReport**（`World::step` の戻り値、状態ではない）: その tick の件数と `events`。4-1 で方策別の `gathers` / `encroachments`（`[u32; PolicyKind::COUNT]`）、決着した取引 `promises_settled: Vec<OpenDeal>`（不足側は `proposer_short / acceptor_short`）、学習者が決定した行のヒストグラム `learner_rows`（学習者が決定した tick だけ `learner_states` 個）。4-2 で `violations / sanctions / episodes_closed / episodes_sanctioned`（`[u32; NormKind::COUNT]`、台帳の差分）。`cz sweep` はこれらを窓で積算する。
- **Event**: `Born` / `Died` / `Intervened` / `Transferred` / `Harmed` / `Proposed` / `Accepted` / `Rejected` / `Signaled` / `Acted` / `Sanctioned` / `Violated` の自己完結レコード（tick 入り、serde 内部タグ）。`Signaled` は `record_signals` のときだけ、`Proposed / Accepted / Rejected`（届いた交渉メッセージ。`Accepted` は契約条件 `give / want / by` と `offer_tick` を複写）は常に。`Intervened` は `World::intervene()` の戻り値、他は `TickReport.events`。`Acted`（全個体の行動と結果）は `record_actions` のときだけ。`Sanctioned { tick, by, target, rule, via: Harm \| Reject }`（係争中の個体に届いた加害 / 拒否の読み。直前に記録される `Harmed / Rejected` と対）は常に、`Violated { tick, agent, rule, norm, against, grounds: Encroachment { cell, amount } \| BrokenPromise { offer_tick, short } \| Harm { damage } }` は `record_violations`（既定 OFF）のときだけ（ADR-0014 D4, D9）。
- **EventLog**（ADR-0010 2a-2）: 追記専用の `Vec<Event>`。tick 非減少を検証、`range(from..to)` は二分探索。JSON Lines（先頭にヘッダ行 `{"format":1,"kernel":…}`）の書き出し/読み戻しは `Write` / `BufRead` を受け取り、カーネルはファイルを開かない。
- **Intervention**（ADR-0010 2a-1、ADR-0013 D7、ADR-0014 D8）: `Drought` / `ScaleFood` / `SpawnAgents` / `RemoveAgent` / `SetPolicy { who: All | Id | Fraction { permille }, kind, params: Default | Keep }` / `Enact { norm }` / `Repeal { rule }` / `Charter { charter }`。tick の合間に `World::intervene()` で適用。無効なら状態を変えずに `Err`（布告 3 種は `config.nomos` オフの世界で `Invalid`、`Repeal` は施行中でない id で `Invalid`）。乱数は `SetPolicy` の `Fraction` が `Stream::Intervention` を `(seed, tick, id)` で引くのが唯一（再生で同じ集合になる）。食料・エネルギーの差分は台帳の `food_intervened` / `energy_intervened` に計上され、保存則の検査式に入る。`Enact` は次の id の規則を施行し（同 kind の規則があれば先に廃止 = 置換）、`Repeal` と置換・`RemoveAgent` はその規則・個体の係争エピソードを閉じて台帳に数える。布告は 1 件の `Intervened` としてだけ台帳に載る（`Enacted / Repealed` のイベントは 4-5 の動議の閉会のもの）。

ECS ライブラリは使わない（ADR-0001）。

## 1 tick の実行順序（ADR-0002, ADR-0008）

```
S(t) ──Index──▶ SpatialIndex（派生状態、保存しない。Decide と Apply で共有）、NormIndex（nomos オン: S(t) の係争フラグと施行中の規則。Decide が Observation.norms として読み、Apply が制裁の判定に読む。どちらも読み取り専用）
     ──Decide(純関数, 並列化可)──▶ Intent[]  ──Apply──▶ S(t+1)
         ↑ RngStream(seed,tick,agent,Decide)、Observation::neighbors() は Index を読むだけ（前 tick の発信は Neighbor.signal）、
           規範の問い合わせ（occupied / my_standing / adjacent_violator / violator_adjacent / rule / …）は NormIndex（フラグとセルごとの HARMABLE 件数）と自セルの arrived を読むだけ（see_standing が偽なら occupied 以外は「なし」）
           Learner の Intent は決定した Q 表の行（Intent.state）も運ぶ。方策は表を読むだけで、更新は Apply の 5a
Apply:
  2 Consume   在庫→エネルギー（私的、競合なし）
  3 Gather    S(t) の位置で解決。同一セルの請求者を抽選キー順に逐次配分。nomos オン: 採集があったセルごとに先占者（arrived 最小、None が最古、同点は id 小）を求め、
             それより後に到着した採集者を侵害と数える（台帳 encroachments、先占者の記憶 EncroachedBy。同 tick の到着は侵害でない。ADR-0014 D3）。
             Encroach の規則があれば侵害は違反: 採集者の (agent, rule) エピソードを開くか更新（nomos.standing）、Violated（セル順・採集者 id 順。ADR-0014 D4）
  3b Transfer / Store / Harm / Signal   S(t) の位置で隣接判定（Manhattan ≤ 1）、行為者 id 順に逐次解決。Say の聴取者は signal_radius 内。
             Propose / Accept / Reject は signal_radius 内の宛先にだけ届く（外なら OutOfEarshot、不在なら NoSuchTarget）。Accept は宛先からの提案を
             受諾者が記憶していることが要件（なければ NoSuchOffer）。届いたものは Proposed / Accepted / Rejected として常に台帳へ（ADR-0013 D3, D5）。
             nomos オン: 届いた Accept は OpenDeal { proposer, acceptor, offer_tick, accepted_tick, deadline = offer_tick + by, give, want } を nomos.deals に開く。
             Harmed の対象が NormIndex（S(t)）で HARM を許す規則の係争中なら制裁: 最古の該当エピソードに帰属し Sanctioned { via: Harm }、記憶は
             Sanctioned / SanctionedBy / SawSanction。Rejected の宛先が REJECT を許す規則の係争中なら同様に via: Reject（宛先の記憶は Rejected の代わりに SanctionedBy）。
             Harm の規則があれば加害は違反（ただし制裁かつ except_sanction なら免責）: 加害者のエピソードを開くか更新、Violated（行為者順）
  3c Audit    nomos オン: この tick の移転を開いている取引の窓（受諾 tick 〜 期限）に計上し、期限が来た取引を決着（given ≥ give ∧ repaid ≥ want なら履行、
             不足側が不履行。台帳 promises_due / promises_broken、TickReport.promises_settled。強制はしない。ADR-0014 D3）。
             BrokenPromise の規則があれば不足側（生存者のみ。提案者 → 受諾者の順）は違反、Violated（取引順）。
             そのあと期限の来たエピソード（last + standing_ticks ≤ tick）を閉じる（台帳 episodes_closed / episodes_sanctioned）。
             イベントは 3b の物理イベント → Sanctioned（行為者順）→ Violated（record_violations のときだけ）の順で積む。動議の閉会・施行は 4-5 でここに入る
  4 Move      同時適用（セル容量無制限なので衝突しない）。nomos オン: 動いた個体の arrived = tick + 1
  5 Upkeep    代謝+行動コスト → Rest 回復 → 飢餓ダメージ → 加齢
  5a Learn    Learner: 前 tick の記録 last_step {s, a, r} を今 tick の行 s'（Intent.state）で更新 Q[s,a] += (r·16 + 29·max Q[s'] >> 5 − Q[s,a]) >> 4、
             今 tick の記録 {s', 行動の種類, Upkeep までのエネルギー差分 /100 + learner_health_weight × 体力差分} を書く（出生の移転は含まない。ADR-0013 D6 3-7、ADR-0014 D3）
  5b Memory   その tick の侵害（先占者に EncroachedBy）・移転・加害・目撃（被害者から social_radius 内）・聴取・提案（ProposalFrom）・受諾（受諾者と提案者に Promised、提案者に Accepted）・
             拒否（Rejected）を起きた順に各個体の recent へ（上限 memory_slots、カーネルのみが書く。ADR-0013 D4）
  6 Death     health<=0 はその tick に加害を受けていれば Violence、そうでなければ Starvation。それ以外で age>=max_age は OldAge。食料はセルへ、エネルギーは償却。
             nomos オン: 死者のエピソードを閉じる（episodes_closed、制裁があれば episodes_sanctioned）
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

- スナップショット: `{format: 11, state_hash, world}` を JSON（可読）またはバイナリ（マジック `CZS\0` + postcard、実行時の既定）で。形式番号は `World` の直列化形が変わるたびに上げ、旧版は `Format(n)` で拒否する。読み込み時に形式版 → config 検証 → 不変条件 → ハッシュ照合（両形式共通）。
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
| 社会的事実（Phase 4-1） | `tests/nomos_facts.rs`, `nomos.rs` 単体, `crates/cz-web/tests/nomos_audit.rs` | 先占と失効（同セルの創設個体、後着、離れて戻る、壁への失敗移動、除去後の次順位）、同 tick 到着は侵害でない + 同点は id、子は親と共同 + `SpawnAgents` は介入 tick、オフでは追跡なしで身体・資源・イベント同一、台本 8 取引の履行 / 不履行とオフの双子の物理一致、体力項の報酬値、Forager / Learner 世界 300 tick でオン・オフの決定一致、方策別採集と行の訪問、往復・形式 8 の拒否・ハッシュへの寄与・不変条件、proptest で serial = parallel = 再実行。cz-web 側: 監査が `thread.rs` の kept / broken と一致（台本 8/9、Random 世界 151/156。差は受諾前の移転の扱い） |
| 観測の規範情報と学習方策（Phase 4-3） | `tests/nomos_policy.rs`, `policy.rs` / `policy/learner.rs` / `nomos/index.rs` 単体 | 規範特徴 5 つの値（規則あり・係争あり・制裁の記憶）と `see_standing: false` で 4 つが 0（`OCCUPIED` と物理の記憶は残る）、規則の廃止後も記憶の `SANCTIONED_RECENTLY` は地平線まで立つ、`occupied()` の真偽表（同 tick・後着・隣接セル・創設個体）、Learner の 3 ビットと 32 / 64 / 128 / 256 行の折り畳み、Harm の対象の優先順位（係争中の隣人 → 同セルの係争中 → 隣接の最小 id、盲目・REJECT だけの規則・半径 0・規則なしでは従来の最寄り）、初期重みの Reflex は規則下でも Forager と同一決定、`nomos` オフで 64 / 128 / 256 行の Learner が 32 行と 1,000 tick 同一（身体・資源・台帳・先頭 32 行の Q、32 以上は未訪問）、`nomos` オンの 64 行は `OCCUPIED` の行を・規則下の 128 行は `VIOLATOR_ADJACENT` の行を訪れる、盲目の 128 行 Learner は規則なしの世界と同一決定で見える方は分岐、proptest で serial = parallel = 再実行・往復。単体: 突然変異が旧 128 個を先に引く、10 ビットの順序と `FINE_STATES`、NormIndex の `len / motion`。4-9a: 件数の述語 `violator_adjacent()` が走査 `adjacent_violator()` と常に一致（手組みの隅、300 tick の規則ありの世界で全個体・全 tick + serial = parallel の decide、proptest 12 ケース）、NormIndex のセルごとの件数（1 個体 1 回、REJECT だけなら 0） |
| 制度（Phase 4-2） | `tests/nomos_rules.rs`, `nomos.rs` / `nomos/index.rs` 単体, `multiverse/tests/fork.rs`, `multiverse/tests/replay.rs` | 3 述語の判定が台本どおりで、同じ intents を規則なし / `nomos` オフの世界に与えて身体・資源・物理の台帳・物理イベントが一致、`except_sanction` の免責と制裁なし規則、制裁の帰属（最古の該当エピソード、REJECT だけの規則は加害を取らない）、係争の期限・死亡・除去・廃止での閉鎖と S / S' のカウンタ、置換 = 旧規則の廃止 + 新 id、布告の JSON と `nomos` オフでの拒否、形式 10 の往復・形式 9 の拒否・不整合 14 種の拒否、Forager / Learner 世界で規則が決定を変えない、同一 tick の `Enact` 2 回 fork でハッシュ一致と `Repeal` の fork、布告・制裁・違反を挟む再生の bit 一致と改竄検出、proptest で serial = parallel = 再実行・NormIndex = 状態・イベント順・カウンタ一致・往復 |
| プロパティ | `tests/determinism.rs` (proptest) | 任意の config（ライフサイクル ON/OFF、突然変異 ON/OFF）で毎 tick 不変条件、消費なしでエネルギー増加なし、再実行一致 |
| ベンチ | `benches/step.rs` (criterion) | 1k/10k/100k 体の step（1,000 tick 後の定常状態）、decide 単体（Forager / Reflex 初期重み / Reflex 進化後 / Learner 32 行、4-3 で先占規範下の Reflex 進化後と Learner 128 行とそれぞれの盲目（`see_standing: false`）の変種）、ハッシュ、スナップショット、`nomos` オン（規則なし / 先占規範あり、4-3 で Learner 128 行）の step、`NormIndex` 構築（1.3k 体・係争 2,000 件 / 空） |
| 計測 | `cz sweep` | 複数シードの個体数帯域（docs/research/experiments/EXP-001） |
| 共通ヘルパ | `tests/common/mod.rs` | 手組みの世界と明示的 Intent |
