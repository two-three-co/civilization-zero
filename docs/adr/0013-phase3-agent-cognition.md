# ADR-0013: Phase 3 — Agent Cognition（他者観測・相互作用・記憶・通信・交渉・学習方策）の設計

- 状態: 提案 → 採用 (2026-10-17)。実装はこの表の順に進め、各タスクの受け入れ基準を満たしたら STATUS.md に移す
- 前提: docs/PRODUCT.md（Phase 3 のデモ）、docs/research/RISKS.md（R1, R4, R5）、ADR-0002（二相 tick）、ADR-0003（整数のみ）、ADR-0004（座標関数の乱数）、ADR-0005（方策は純関数）、ADR-0008（イベント形式）、ADR-0010/0012（台帳・fork）

## 文脈

Phase 2 までの世界は「個体が食料を探し、食べ、分裂し、死ぬ」物理だけで、個体は互いを**見ていない**。
Phase 4（制度）が扱う所有権・規範・制裁は、個体が他者を観測し、やりとりし、覚え、伝え、約束する能力の上にしか成立しない。
Phase 3 の目的は、その**社会的プリミティブ**と、設計者が書いた方策ではない**学習する方策**を、決定論と再現性の契約を壊さずにカーネルへ入れることである。

Phase 3 は制度を実装しない。「所有権が生まれた」と言える指標の設計と検証は Phase 4 の仕事で、Phase 3 はそのための材料（相互作用の台帳、記憶、通信、学習）を揃える。

## 完了条件

1. **ブラウザのデモ（PRODUCT.md Phase 3）**: 公開 URL で、(a) 個体を選ぶと記憶（最近の相互作用）・最後の発信・受けた提案が見える、(b) 地図上で移転・加害・発信が起きた場所がその tick に可視化される、(c) 2 個体間の交渉スレッド（提案 → 受諾/拒否 → 移転）が台帳から表示される、(d) 「固定方策 vs 学習方策」を同じ世界から fork で並べ、人口・摂取量・移転数のカーブを比較できる。
2. **計測**: 学習方策の世界 1,300 体で、ブラウザ内 ≥ 60 tick/s を画面に表示。バイナリスナップショットは 1,300 体で ≤ 1 MB。
3. **事前登録実験 EXP-002**（下記）を 10 シード以上で実施し、結果と限界を記録する（学習が「効く」かの健全性チェックであり、創発の主張ではない）。
4. **決定論**: 既定の Forager だけの世界は 3-1〜3-5 を通してゴールデンハッシュ `0xf16cb4be6e7ed69b`（3-2 で config と台帳を除く `dynamics_hash` に再定義し `0x658ee873d1866501`。遷移は不変）を保つ（新しい物理は Forager に対して不活性であること。符号化のみの変更は ADR-0010 2a-1 と同じ旧符号化の再計算で証明する）。学習方策の世界には別のゴールデンを置く。CI の wasm ハッシュ照合は維持。
5. **台帳**: 相互作用（移転・加害・発信・提案）は常にイベントとして台帳に載り、再生で照合される。全行動の `Acted` 記録は設定で ON にでき、サイズを計測して記録する。

## 決定（設計）

### D1. 方策はカーネル内のモジュールのまま（別クレートにしない）

方策は `World::step` の Decide フェーズから呼ばれる純関数であり、遷移関数の一部である。別クレートにすると循環依存になる。
`simulation-core/src/policy/` をディレクトリ化し `forager.rs` / `random.rs` / `reflex.rs` / `learner.rs` に分ける。ARCHITECTURE の「`cognition` クレート」はこの構成に置き換える。

### D2. 他者観測は tick 開始時に作る空間インデックスから（3-1）

Decide の前に一様グリッドのバケット（セル → 個体 index 列、id 昇順）を O(n) で再構築し、`Observation::neighbors()` が `social_radius`（既定 2）内の個体を **(距離, id) の昇順**で返す。
見える属性は `Perception` 設定で絞る（位置は常に、方策種・在庫・エネルギー・最後の発信は個別に ON/OFF）。これは H2 の「通信能力・記憶能力」をパラメータにするため。
決定論: インデックスは Decide 中は読み取り専用、順序は仕様。並列 decide との一致テストを維持。

### D3. 相互作用の行動は汎用プリミティブ 4 つ（3-2）。制度名の行動は作らない（R1）

| 行動 | 事前条件 | コスト | 効果 | 失敗 |
|---|---|---|---|---|
| `Transfer { to, n }` | `to` が S(t) で隣接（Manhattan ≤ 1）、在庫 ≥ 1 | `transfer_cost` | min(n, 在庫, 相手の空き) を相手へ。食料の総量は不変 | `NotAdjacent`, `EmptyInventory`, `TargetFull`（0 個移転） |
| `Store(n)` | 在庫 ≥ 1 | 0 | 自セルに min(n, 在庫) を置く（容量上限なし。死亡時の落下と同じ） | `EmptyInventory` |
| `Harm { target }` | 隣接 | `harm_cost`（エネルギー、台帳 `energy_spent`） | 相手の health −`harm_damage`。複数は加算 | `NotAdjacent` |
| `Signal(message)` | — | `signal_cost`（既定 0） | `last_signal = (tick, message)`。`signal_radius` 内の個体が次 tick に観測 | — |

- 解決順は既存の固定フェーズに従う: Consume → Gather → **Interact（Transfer / Store / Harm / Signal、行為者 id 順）** → Move → Upkeep → **Memory（3-3）** → Death → Birth → Env。移転・加害・発信は S(t) の位置で判定する（Gather と同じ同時手番）。
- 加害による死は新しい死因 `DeathCause::Violence`（その tick に加害を受けて health ≤ 0。餓死より優先）。
- `Harm` は「制裁」そのものではない。制裁は Phase 4 で、台帳上の違反に対して個体が `Harm`（や移転拒否）を選ぶ相互作用として表現する（R4）。
- 台帳: `Event::Transferred { from, to, amount }`、`Event::Harmed { by, target, damage }` は**常に**記録。`Event::Signaled` は `record_signals`、`Event::Acted { agent, intent, outcome }` は `record_actions` で ON（既定 OFF、サイズを計測）。

### D4. 記憶は固定長リング、カーネルが更新する（3-3）

`Agent.memory` に `recent: [Interaction; K]`（既定 K = 8、設定可）を追加。`Interaction { tick, other: AgentId, kind, value }`、`kind` は `GaveTo / ReceivedFrom / HarmedBy / Harmed / SawHarm / Heard / ProposalFrom / Accepted / Rejected / Promised` の u8。
更新は Apply の末尾でその tick のイベントから決定論的に行う（Decide は読むだけ、ADR-0005 を保つ）。`SawHarm` は `social_radius` 内で起きた加害を目撃者に書く（規範の第三者執行の材料）。
スナップショットに入る（1,300 体 × 8 × 約 12 B ≈ 125 KB の増加見込み。計測する）。

**実装（3-3、2026-10-17）と設計からの差分**:
- 記憶は `Agent.memory` の中ではなく兄弟フィールド **`Agent.recent: Vec<Interaction>`**。`Memory` は方策が所有し `Intent` で返す値（ADR-0005）なので、カーネルが書く記録を同じ構造体に入れると「方策は世界を変えない」の境界が曖昧になり、`Intent.memory` の書き戻しで記憶を潰す事故も起こりうる。
- 固定長配列 `[Interaction; K]` ではなく `Vec` を `memory_slots`（u8、既定 8、0 で無効、上限 64）で打ち切る oldest-first のリング。K は設定値で型に埋め込めず、空なら 1 B で済む。
- `kind` は 3-3 時点で `GaveTo / ReceivedFrom / Harmed / HarmedBy / SawHarm / Heard` の 6 種。`SawHarm` の `value` は被害者 id、`Heard` の `value` は記号。3-4 で `ProposalFrom / Accepted / Rejected / Promised` を追加（`value` の意味は D5 の実装注記）。
- 書き込み順は 3b の解決順: 行為者 id 順に、1 行為につき 行為者 → 相手 → 目撃者（被害者から `social_radius` 内）/ 聴取者（発信者から `signal_radius` 内）を (距離, id) 順。発信者自身は聴取者に含めない。Phase 5b（死亡処理の前）で反映するので、その tick に死ぬ個体にも書かれる（スナップショットには残らない）。
- ハッシュ: `last_signal` と `recent` は**非空のときだけ** `dynamics_hash` にバイトを足す。Forager 世界では両方とも空なのでゴールデン `0x658ee873d1866501` は不変（空なら旧符号化と同一バイト列になるため、旧符号化の再計算は不要）。
- 計測（Apple M1, 2026-10-17）: スナップショットの個体部分は Forager 19.7 B/体（`None` + 空 Vec で +2 B）、Random（記憶 5.5 件/体）46 B/体、満杯 8 件で 64 B/体（記憶を消すと 15〜20 B）。1 件 約 6 B（postcard varint）。128² 世界は資源 33 KB が支配的で、1,357 体の Forager スナップショットは 56.8 → 59.6 KB（+5%）、全個体が満杯でも約 120 KB（≈ 2.0 倍。tick・id が 16,384 を超えると varint が伸び 2 倍を少し超えうる）。見込みの 125 KB 増より小さい。

### D5. 通信と交渉は「記録されるが強制されないメッセージ」（3-3, 3-4）

`Signal` のペイロードは `Message` enum:
- `Say(symbol: u8)` — 意味を事前に与えない記号（R1: 語彙の意味は学習に委ねる）
- `Propose { to, give: u8, want: u8, by: u16 }` — 「今 give 渡す、by tick までに want 返せ」
- `Accept { to, offer_tick }` / `Reject { to }`

**実装（3-3）**: `Say` のみ。`Observation::neighbors()` の `Neighbor.signal` は**前 tick の発信だけ**（`last_signal.tick + 1 == tick`）を `signal_radius`（既定 2）内で見せる。古い発信は観測から消えるが `Agent.last_signal` には残る。聴取者には同 tick の Memory フェーズで `Heard` が書かれる。`Event::Signaled` は `record_signals`（既定 OFF）。Random 方策は `Say(0..8)` を含む 11 行動を一様に抽選する。

カーネルは配達（相手の記憶に `ProposalFrom` 等を書く）と台帳記録だけを行い、**約束の履行は強制しない**。履行は `Transfer` という物理行動であり、不履行は台帳から判定できる「社会的事実」になる。これが Phase 4 の制度（違反の定義と制裁）への接続点で、R4（制度は物理を変えない）と整合する。
資源は食料 1 種類のまま。したがって交換は「今と後」の移転（貸し借り・分かち合い）であり、互恵と信頼の材料になる。資源の多様化は、それが必要だと実験が示してから（Phase 4 以降）。

**実装（3-4、2026-10-17）と設計からの差分**:
- **宛先付きの配達**。`Propose { to, give, want, by }` / `Accept { to, offer_tick }` / `Reject { to }` は `to`（自分以外の生存個体）が S(t) で `signal_radius` 内にいるときだけ届く。いなければ `Failure::NoSuchTarget`（不在・死亡・自分宛）または新設の **`Failure::OutOfEarshot`**。`NotAdjacent` を流用しなかったのは、`action.rs` の表で `NotAdjacent` が「Manhattan ≤ 1」と定義されており、既定 2・設定可の到達半径と混ぜると失敗の集計（`TickReport.failures`）が隣接の失敗と区別できなくなるから。`signal_cost` は試行時に払う（`Transfer` / `Harm` と同じ）。
- **交渉メッセージは `last_signal` を書かず、傍観者にも届かない**。`Say` は発話（`Neighbor.signal` で近傍全員が次 tick に見る）、交渉は私信（宛先の記憶と台帳にだけ残る）。設計の「`signal_radius` 内の個体が次 tick に観測」は `Say` にのみ適用した。宛先付きの提案が第三者にも見えると、情報制限（D2）を素通りして交渉内容が公開情報になり、記憶を持たない個体（`memory_slots = 0`）が提案を観測だけで知るという不整合も生むため。
- **`by` は提案 tick からの相対期限**（履行期限 = 提案 tick + `by`）。u16 では絶対 tick を長い世界で表せず、相対にすれば台帳の `Proposed` / `Accepted` が自分の tick と合わせて自己完結する。
- **受諾の成立条件 = 受諾者が提案を記憶していること**。`Accept` は受諾者の `recent` に `ProposalFrom { other: to, tick: offer_tick }` があるときだけ成立し、なければ **`Failure::NoSuchOffer`**。理由: `Promised` と自己完結な `Accepted` イベントを書くにはカーネルが契約条件を知る必要があり、それを保持している状態は受諾者の記憶だけである（カーネルは台帳を持たない）。「持っていないものは渡せない」（`EmptyInventory`）と同じく「受け取っていない提案は受諾できない」を物理的不能として扱う。帰結として、`memory_slots = 0` の世界では約束が成立せず、リングから押し出された提案は受諾できない——記憶容量が成立しうる取引を制限する（H2 のパラメータとして意図どおり）。同じ提案を 2 度受諾することはカーネルは止めない（判定は Phase 4 の仕事）。`Reject` は `offer_tick` を持たないので検査せず、届けば記録する。
- **記憶の `value`**: `ProposalFrom` と `Accepted` は契約条件 `Terms { give, want, by }` を `Terms::pack()`（give が 0..8 bit、want が 8..16、by が 16..32）で詰めた値——受諾する側の方策は記憶からしか条件を知れないため。`Rejected` は 0。**`Promised` は「自分が `other` に渡すと約束した食料数」**: 提案者には `give`（今渡す）、受諾者には `want`（期限までに返す）。両者で意味が同じ 1 つの数にし、期限は台帳（`Accepted` の `offer_tick + by`）に置いた。方策に必要なのは「誰にいくら負っているか」であり、期限から違反を判定するのは台帳を読む側（Phase 4）だから。
- **書き込み順**は 3-3 の規約（行為者 → 相手）: 受諾 1 件につき 受諾者の `Promised` → 提案者の `Accepted` → 提案者の `Promised`。提案は宛先の `ProposalFrom` のみ（提案者には何も書かない）。すべて Phase 5b で反映。
- **台帳は新イベント `Proposed { tick, from, to, give, want, by }` / `Accepted { tick, from, to, offer_tick, give, want, by }` / `Rejected { tick, from, to }`** を**常に**記録する（届いたときだけ）。`Signaled` を流用しなかった理由: (1) `Signaled` は `record_signals`（既定 OFF）で制御され、不履行の判定材料は常時必要、(2) 届かなかった提案が「提案」として台帳に載ると Phase 4 が存在しない義務を判定してしまう、(3) `Accepted` に条件を複写すれば 1 レコードで「誰が誰にいくらをいつまでに」が読め、`Proposed` との結合が不要。台帳カウンタ `proposals / acceptances / rejections`（届いた件数）を追加、`signals` は `Say` の件数のまま。
- **Phase 4 への接続（D5 の追補）**: 受諾者の不履行 = `Accepted { from: B, to: A, offer_tick, want, by }` に対し、tick ≤ `offer_tick + by` の `Transferred { from: B, to: A }` の合計が `want` 未満。提案者の不履行 = 受諾後に `Transferred { from: A, to: B }` の合計が `give` 未満（「今」の期限の定義は Phase 4）。カーネルはどちらも検査しない。`tests/negotiation.rs::a_broken_promise_changes_nothing_physical` が、期限が過ぎても失敗・イベント・状態変化が一切ないことと、台帳だけからこの判定ができることを示す。
- **Random 方策は 14 行動**を抽選する: `Propose`（`signal_radius` 内の一様な相手、give/want ∈ 0..3、by ∈ 1..16）、`Accept` / `Reject`（記憶の最新の `ProposalFrom` に対して）。帰無モデルは学習者ができることをすべてできるべきで、交渉の配達・検査の経路を serial = parallel の proptest で通すためでもある。Random 世界の軌跡は変わる（EXP-001 の注記）。Forager は交渉しないのでゴールデン `0x658ee873d1866501` は不変。
- スナップショット形式 6（`Ledger` の形が変わった）。計測は STATUS の「3-4 後」。

### D6. 学習方策は 2 種類、どちらも整数演算（ADR-0003）

- **`Reflex`（進化する反射方策、3-6）**: 観測から固定の特徴ベクトル（空腹度・足元/視界の食料・隣人数・聞いた記号・記憶フラグなど、約 16 次元の整数）を作り、行動候補ごとに固定小数点（i32、1/1024 単位）の重みで線形スコアを計算して argmax（同点は `Decide` ストリームの抽選）。重みは `PolicyParams` に入り**継承**され、`mutation_enabled`（Reflex では既定 ON）で世代間に進化する。生涯内学習はしない。Forager と同じ決定を再現する初期重みを用意し、学習の出発点とテストに使う。
- **`Learner`（生涯内の表形式 Q 学習、3-7）**: 状態 = 離散化した観測（空腹 4 × 足元食料 2 × 視界食料 2 × 隣人 2 × 直近記号 4 = 128 状態）、行動 8 種、Q は i16（固定小数点）。報酬 = その tick のエネルギー差分（ミリ単位 /100）+ 出生ボーナス。更新 `Q += (r + γ·maxQ' − Q) >> 4`（α = 1/16、γ ≈ 29/32）、ε-greedy の乱数は `Decide` ストリーム。Q 表は個体の状態（スナップショットに入る。128 × 8 × 2 B = 2 KB/体 → 1,300 体で 2.6 MB raw は大きい）。**既定は 32 状態 × 8 行動 = 512 B/体**（≈ 0.65 MB）にし、128 状態は設定で選ぶ。子は親の Q を継承する（設定で白紙にもできる）。
- 浮動小数点は使わない。これにより native / wasm / 並列のすべてで bit 一致が保たれ、Toy 1 の Verify がそのまま学習世界にも効く。
- 報酬は自己のエネルギーだけなので、移転は学習者にとって損である。分かち合いが「割に合う」条件（互恵、制裁）を作るのは Phase 4 の制度であり、Phase 3 で分かち合いが出なくても失敗ではない。

**実装（3-5 = 枠のみ、2026-10-17）と設計からの差分**:
- `PolicyKind::{Reflex, Learner}` を追加。**決定規則は未実装**で、両方とも `forager()` に委譲する明示的なプレースホルダ（`policy.rs` の `reflex()` / `learner()`）。3-6 は「初期重みの Reflex が Forager と同一決定」を受け入れ基準にしているので、この委譲はその出発点でもある。`tests/set_policy.rs` は Reflex 世界・Learner 世界の身体（位置・エネルギー・在庫・年齢）と資源・台帳が Forager 世界と 600 tick 一致することを**明示的に**検査し、プレースホルダが実装と誤認されないようにしている。
- 重みと Q 表は `PolicyParams.table: Option<PolicyTable>`（`Reflex { weights: Vec<i32> }` = `REFLEX_FEATURES`(16) × `Action::KIND_COUNT`(8) の 1/1024 固定小数点、`Learner { q: Vec<i16> }` = `learner_states`(既定 32、1..=1024) × 8）。`PolicyParams` は `Copy` でなくなり `Clone`。固定方策は `None` で、**`None` は `dynamics_hash` にバイトを足さない**ので Forager のゴールデンは不変。postcard では `None` が 1 B なので Forager のスナップショットは +1 B/体（1,357 体で 59,583 → 60,944 B）。
- `World::check_invariants` が「`policy` と `table` の形が合うこと」を検査する（Forager / Random は `None`、Reflex は長さ 128、Learner は `learner_states × 8`）。スナップショット読み込みはハッシュ照合の前に不変条件を通るので、方策と表の不一致・表の長さ違いは `Invalid` で拒否される。
- 出生: 子は親の `params` を `clone()` して受け取る。`mutation_enabled` は `hunger_permille` だけを動かし、**重みの突然変異は 3-6**（表はそのまま継承）。Learner の子は `learner_inherit_q`（既定 true）で親の Q を継承するか白紙で始まる。
- 設計の `REFLEX_FEATURES ≈ 16` と Q の 32 / 128 状態は、3-5 では**形だけ**を確定した。3-6 / 3-7 で特徴の意味や離散化を決めるときに長さが変われば `SNAPSHOT_FORMAT` を上げる。
- CLI: `--policy reflex|learner|mixed-learning`（mixed-learning は Forager / Reflex / Learner の輪番）、`--learner-states N`、`reflex` と `mixed-learning` は `--no-mutate` がなければ突然変異 ON（D6 の「Reflex では既定 ON」）。

**実装（3-6 = Reflex、2026-10-17）と設計からの差分**:
- 特徴は `policy::feature` の 16 次元（`REFLEX_FEATURES` は 16 のまま、形式は 7 のまま）: `BIAS`(常に 1) / `HUNGRY` / `HAS_FOOD` / `PACK_FULL` / `ROOM`（あと 1 単位食べられる）/ `FOOD_HERE` / `FOOD_IN_SIGHT`（視界で最も豊かな他セルの食料。**在庫に空きがあるときだけ走査し、満杯なら 0**。Forager と同じ条件でしか走査しないことが性能の要）/ `HAS_TARGET` / `NEIGHBOURS`・`ADJACENT`（`social_radius` 内と距離 1 以内の他個体数、上限 15）/ `HEARD`（前 tick に聞いた記号 + 1、なければ 0）/ `HARMED_RECENTLY`・`RECEIVED_RECENTLY`・`SAW_HARM_RECENTLY`・`PROMISED_RECENTLY`（16 tick 以内の記憶フラグ）/ `ENERGY_LEVEL`（`max_energy` の 1/8 刻み、0〜8）。
- スコアは `score[k] = Σ_f weights[f·8 + k] · x[f]`（i64。重みは i32 の 1/1024 固定小数点、`±2^20` にクランプ）、行動**種類**の argmax。同点は `Decide` ストリームの `below(n)` 1 回で決め、**同点のときだけ乱数を消費**する。初期重みは最上位に同点を作らないので、乱数の消費列まで Forager と同一になる。
- 学習するのは行動の種類だけで、パラメータは固定のアクチュエータが埋める: Move は Forager の規則 4〜5 そのもの（`forage_move`、目標セルの記憶も同じ）、Gather は Forager と同じく target を自セルにする、Transfer(1) / Harm は `Observation::neighbors()` の順（距離、id）で最初の距離 ≤ 1 の個体（`policy::nearest_adjacent`、index 直読み）、いなければ Rest、Store(1)、Consume(1)、Signal は前 tick に聞いた記号の反復（なければ `Say(0)`）。**記号の選択は進化しない**: 候補を `Say(0..8)` に広げると表の形が 16 × 15 になり形式が変わるので先送り（必要になれば 3-8 の前に判断）。
- **Forager 等価の初期重み `forager_weights()`**: Forager の規則順を優先順位の梯子 Consume（条件成立で ≥ 1.0）> Gather（0.5）> Move（0）> 他（−1.0）に、各条件を 0/1 特徴の線形しきい値にした。Consume は `2·hungry + 2·has_food + 4·pack_full + 8·room − 11 > 0` ⇔ `room ∧ ((hungry ∧ has_food) ∨ pack_full)`（規則 1 と 3。規則 3 は満杯が条件なので Gather と競合せず、順序の矛盾は生じない）、Gather は `food_here − pack_full − 0.5 > 0`（規則 2）。`PolicyTable::blank(Reflex)` と `SetPolicy { params: Default }` はこの重みから始める（3-5 の「零の重み」から変更。零の重みは全種類が同点で毎 tick 抽選になる）。
- 突然変異: `mutation_enabled` のとき `hunger_permille` に続けて 128 個の重みそれぞれに `±mutation_step_permille`（1/1024 単位）の一様整数を足す（子 id の `Mutation` ストリームだけの関数、1 値 1 抽選、クランプあり）。`check_invariants` が重みの範囲も検査し、スナップショットの読み込みで範囲外を拒否する。`PolicyParams::mutated` は Q 表には触れない。
- Forager 側は規則 4〜5 を `scan_best`（走査 1 回目）と `forage_move`（同点抽選と移動）に分離しただけで演算順序は不変。**ゴールデン `0x658ee873d1866501` は不変**（native / wasm）。
- **計測（Apple M1、2026-10-17、1,316 体 128²）**: decide は Forager 87 ns/体·tick、**Reflex 初期重み 147 ns（基準 ≤ 150 を満たす。余裕 2%）**、**Reflex 進化後（突然変異 ON で 1,000 tick、1,484 体）182 ns（基準超過 +21%）**。超過分は crowd 特徴（index の累積和で数えて約 20 ns。初期重みでは重みが零なので計算を省ける）、非零の特徴行が増える分のスコア計算、社会行動のアクチュエータ。最適化の経緯: 210 → 168（crowd を `bounds.diamond` の走査から `SpatialIndex::crowd` の累積和に、`ENERGY_LEVEL` の i128 除算を除去）→ 164（記憶の単一走査、走査を在庫条件付きに）→ 147（crowd の重み行が零なら省略）、進化後 201 → 182（`nearest_adjacent` を index 直読みに）。step は Forager 161 µs/tick、Reflex 238 µs/tick（≈ 4,200 tick/s で製品目標の 1,800 tick/s は満たす）。**基準の扱い（進化後の重みで ≤ 150 を要求するか、≤ 200 に改定するか、crowd を index 構築時に前計算するか）は下の「決定（2026-10-17）: decide 基準の改定」で決めた。**
- スナップショット（tick 1,000）: Forager 60,944 B（20.7 B/体）、Reflex 初期重み 259,065 B（1,357 体、166.7 B/体。零の重みは 1 B、非零は 2〜3 B）、Reflex 進化後 375,089 B（1,484 体、230.6 B/体）。3-7 の Learner と合わせて「1,300 体 ≤ 1 MB」の範囲内。

**実装（3-7 = Learner、2026-10-17）と設計からの差分**（`policy/learner.rs`。D1 の `policy/` ディレクトリ化の最初のファイルで、`policy.rs` はモジュールの根のまま）:
- **状態**（`policy::learner::state`、`learner_state()`）: 7 ビットの細かい格子 = 128 状態。重要なビットから順に `HUNGRY`（Forager の規則 1 のしきい値 `energy·1000 < max_energy·hunger_permille`。Reflex の `HUNGRY` と同じ）/ `HAS_FOOD`（在庫 > 0）/ `FOOD_HERE`（足元に食料）/ `FOOD_IN_SIGHT`（視界で最も豊かな他セルに食料。**在庫に空きがあるときだけ走査し、満杯なら 0**。Forager・Reflex と同じ条件でしか走査しないことが性能の要）/ `ADJACENT`（距離 1 以内に他個体 = `nearest_adjacent` が Transfer / Harm の対象を見つける条件そのもの。`social_radius = 0` なら自セルのみ）/ `UPPER`（空腹帯の上半分: 空腹なら energy ≥ しきい値/2、満腹なら energy ≥ (しきい値 + max_energy)/2。`HUNGRY` と合わせて空腹 4 段階）/ `HEARD`（前 tick に `Say` を聞いた）。表の行は **`fine % learner_states`**（`state_row`）: 既定 32 行は下位 5 ビット（身体と視界と隣人）、128 行で全ビット、2 の冪でない行数は決定論的なエイリアス、128 超の行は訪れない。
- 設計の「空腹 4 × 足元 2 × 視界 2 × 隣人 2 × 記号 4」からの差分: (1) **`HAS_FOOD` を追加**。Consume は在庫なしでは物理的に不能で、既定世界の最大の報酬 +35（4,000 − 500 の /100）の最大の予測因子。これがないと同じ行で Consume の価値が在庫の有無で平均され、学習が遅く不安定になる。(2) **直近記号は 4 段階 → 2**（聞いた / 聞いていない）。記号に事前の意味はなく（R1）、表の予算（128 × 8 × 2 B）の 1 ビットを `HAS_FOOD` に回した。記号の意味の学習を試すときは `learner_states` を 256 にして記号ビットを足す（上限 1024 の範囲内）。(3) 隣人 = 距離 1 以内の有無（社会行動が成立する条件）であって、`social_radius` 内の人数ではない。
- **行動選択**（`learner::decide`）: 行の argmax。同点は Reflex と同じ `argmax_kind` の抽選（同点のときだけ乱数消費）。**ε-greedy** は新 config **`learner_epsilon_permille`**（既定 50 = 5%、0..=1000）で、ε > 0 のとき `Decide` ストリームで `chance(ε, 1000)` を 1 回引き、探索なら `below(8)` で種類を一様に選ぶ。乱数の消費順は ε 抽選 → 探索の種類 or 同点抽選 → アクチュエータ（Move の抽選）で固定。**学習するのは行動の種類だけ**で、パラメータは Reflex と同じアクチュエータ（`reflex_act`: Move = Forager の規則 4〜5、Gather は target を自セルに、Transfer(1) / Harm は最寄りの隣接個体・いなければ Rest、Store(1)、Consume(1)、Signal は前 tick に聞いた記号の反復）。零の初期 Q は全種類が同点なので、各状態で各種類を 1 回ずつ試してから好みがつく（楽観的初期化と同じ探索効果。3-5 の「零から始める」を維持）。
- **Q の更新はカーネルが Apply で行う**（ADR-0005: 方策は `Observation -> Intent` の純関数で表を触らない）。方策は **`Intent.state`**（決定した行、他の方策は `None`）を返すだけ。Apply の **Phase 5a**（Upkeep の直後、Memory・Death・Birth の前）で個体ごとに: 前 tick の記録 `Agent.last_step { state: s, action: a, reward: r }` があれば `Q[s, a] += (r·16 + (29·max Q[s'] >> 5) − Q[s, a]) >> 4`（s' = 今 tick の `Intent.state`、i64 で計算し i16 に飽和、`learner::credit` / `q_step`）、続けて今 tick の記録を `{ s', 今の行動の種類, 今 tick の報酬 }` で上書きする。**報酬 = その tick の Upkeep 終了時までのエネルギー差分 /100**（Consume の +4,000、代謝・行動コスト。切り捨て、i16 飽和）。出生で子へ渡す `child_energy` は報酬を確定した後の Phase 7 で引かれるので報酬に入らない = **「出生ボーナス」= `child_energy / 100`**（子に渡したエネルギーを失ったとみなさない）。それ以上の正のボーナスは置かなかった: 出生はエネルギー ≥ しきい値で自動に起きるので、自己エネルギーの最大化が繁殖の最大化と同じ向きになる。
- **Q の固定小数点**: `Q_UNIT = 16`（報酬 1 単位 = 100 ミリエネルギー = Q の 16）。`>> 4` は切り捨てなので、固定小数点なしでは 1 単位未満の TD 誤差が常に 0 に潰れて小さい報酬差を学べない。既定世界の報酬は Consume +35、代謝 −5、移動 −8、採集 −7、加害 −25 程度で、Q の到達範囲は r/(1 − γ) = r·32/3 ≈ ±6,800（固定小数点）なので i16 に収まる。極端な config では `learner::reward` と `q_step` が i16 に飽和し、巻き戻らない（ユニットテストで確認）。更新は切り捨てのため、固定点は「TD 誤差が [0, 16) に入る帯」で、下から上がれば帯の下端、上から下がれば上端に止まる（帯の幅 ≈ 16·32/3 ≈ 171）。単調性はこの帯に入るまで保たれる。
- **前 tick の (状態, 行動, 報酬) の置き場所 = 新フィールド `Agent.last_step: Option<LearnerStep>`**（カーネル所有、`recent` の兄弟）。候補と退けた理由: (a) `Agent.memory` は方策所有で毎 tick `Intent.memory` から書き戻される。ここに学習記録を置くと方策が（偶然や進化で）上書きでき、カーネルが方策所有の値を読んで世界を変える（Q 更新）ことになり、ADR-0005 の境界が曖昧になる。(b) `recent` は `Interaction { other: AgentId, kind, value }` の社会的相互作用のリングで、`memory_slots` で打ち切られる（0 なら学習できなくなる）し、`other` に状態を詰めるのは型の誤用。(c) `PolicyParams.table` は**継承される**パラメータで、記録は継承されない（新生児に前 tick はない）。表に同居させると出生・`refit`・`mutated`・`SetPolicy Keep` の全経路で記録だけを除く特別扱いが要る。`last_step` は `Option` で **`None` は `dynamics_hash` にバイトを足さない**（Forager のゴールデン不変）。postcard では `None` が 1 B なので Forager のスナップショットは +1 B/体。
- 記録の寿命: 新生児は `None`（最初の決定から学習）、死亡で消える（**終端の更新はしない**: 死の tick の記録は使われず、餓死は報酬 −5 の連続と Consume の不発としてしか学ばれない。死に負の報酬を与えるのは EXP-002 の後に判断）、`SetPolicy` は対象全員の `last_step` を消す（Learner → Learner `Keep` でも、切替 tick の連鎖は切る。学んだ Q はそのまま）、`Intent.state` が `None` の tick（手組みの Intent、他 kind）も連鎖を切る。`check_invariants` は `Some` なら Learner であること・`state < learner_states`・`action < 8` を検査し、スナップショット読み込みで不整合を `Invalid` として拒否する。
- 継承: 3-5 のまま（`learner_inherit_q`、`mutated()` は Q に触れない）。子の Q は Phase 5a で今 tick の更新を受けた**後**の親の表のコピー。
- 性能のための選択: 視界走査は在庫に空きがあるときだけ（Forager と同じ）、隣接判定は index のセルを最大 5 つ読んで早期終了（`SpatialIndex::crowd` の累積和より安い）、記憶の走査は 1 回（`heard_last_tick`）、Q 行は i16 8 個の argmax。Apply 側の更新は 1 行の max と 1 セルの更新で、`start_energy` の複写は `Intent.state` が 1 つでも `Some` の世界だけが払う。
- スナップショット形式 **8**（`Agent.last_step`、config `learner_epsilon_permille`。旧 config は ε 既定 50 で読める）。CLI に `--learner-epsilon PERMILLE`。
- テスト: `tests/learner.rs` 11 本（状態ビットと 4 段階の空腹の境界値・満杯で走査なし・隣接の半径規則・聴取は前 tick だけ、行に従う決定と 8 種のアクチュエータ・隣接で行が変わる・`HEARD` が 32 行では同じ行に折れ 128 行では分かれる、ε = 1000 で 8 種が一様（8,000 tick、5σ）・ε = 0 で唯一の最大が常に選ばれる・ε = 500 で非貪欲が約 43.75%、カーネルの更新値が `q_step(前の値, 前 tick の報酬, 次行の最大)` に一致し 1 セルだけ変わる・手組み Intent で連鎖が切れる・死で消える、出生 tick の報酬が 35（−365 ではない）で子は `None` と親の表、Rest の自己ループで Q が純関数列と一致して単調減少、`SetPolicy Keep` が記録を消して表を保ち次 tick に再開、両形式の往復・形式 7 の拒否・`state ≥ 行数` / `action ≥ 8` / Forager の記録 / ε > 1000 の拒否・旧 config の読み込み、8 行と 128 行の世界で子の表が親と同一（突然変異 ON）かつ行が範囲内で 128 行では 32 以上も訪れる、Forager 世界に記録がなくゴールデン不変、**proptest 12 ケース（ε・行数 {1, 2, 8, 32, 128, 300}・突然変異・tick 数）で serial = parallel = 再実行、不変条件、往復**）、`policy/learner.rs` 単体 4 本（固定点への上下からの単調収束、i16 飽和、`credit` の境界、行の折り畳み）、`multiverse/tests/replay.rs::learner_worlds_replay_bit_exactly_from_snapshot_and_ledger`（mixed-learning 世界に `Fraction 500 Keep` と `All Default` の `SetPolicy` を挟み、9 つの tick の `state_at` が直接実行と `World` ごと一致、tick 300 のスナップショットからの再生が head と一致、バイト列から復元しても同じ）。`tests/set_policy.rs` の「Learner は Forager と同一」は「Learner は自分の履歴を持ち、表は非零、記録を持つ」に書き換えた。
- 計測は STATUS の「3-7 後」（decide・step・スナップショット）。

**決定（2026-10-17）: decide 基準の改定**

- 当初の 3-6 基準「1.3k 体で decide ≤ 150 ns/体·tick」は特徴も実装も無い段階で置いた数字で、計測で 2 つのことが分かった。(1) 初期重みでは 147 ns で満たす。(2) 突然変異で重みが密になった世界では 182 ns で超過する。超過分は crowd 特徴（約 20 ns）、社会行動のアクチュエータ、非零の特徴行の分で、どれも「他者を読んで反射する」という Reflex の目的そのもの。これを削って数字を守るのは本末転倒。
- 残る最適化（crowd を index 構築時に前計算、i16 重み、SIMD スコア）は、128² で 1.3k 体なら前計算のコスト（16k セル × 13 ≈ 0.1〜0.2 ms/tick）が節約（1.3k × 20 ns ≈ 26 µs/tick）を上回り、費用対効果が悪い。損益分岐は 1 万体前後で、ブラウザの目標外。
- 製品の門（PRODUCT.md: 1,000〜2,000 体をブラウザで ≥ 60 tick/s）には 40 倍以上の余裕がある。Toy 1 の体験値 1,800 tick/s（30 tick/フレーム × 60 fps）も native では進化後 Reflex ≈ 3,000 tick/s、Learner ≈ 2,900〜3,160 tick/s で満たす（wasm は 3-9c で計測する）。
- **改定後の基準**（Apple M1、1.3k 体 128²、`cargo bench -p simulation-core --features parallel` の decide グループ）: Forager ≤ 100、Reflex 初期重み ≤ 150、**Reflex 進化後と Learner ≤ 200**（3-7 の基準と同じ）。加えて製品の門として、学習世界の step は 1.3k 体で **≥ 1,800 tick/s（native）**、ブラウザ（3-9c）で 15 tick/フレーム × 60 fps（900 tick/s）以上、PRODUCT.md の 60 tick/s を下限とする。150 を進化後の努力目標として残すことはしない（二重基準を避ける）。
- 現在値（2026-10-17）: decide Forager 85〜87、Reflex 初期重み 146〜147、Reflex 進化後 181〜182、Learner 174 ns/体·tick（いずれも基準内）。step: Reflex 238 µs/tick ≈ 4,200 tick/s、Learner 344 µs/tick ≈ 2,900 tick/s。
- 再検討の条件: ブラウザで 1 万体の学習世界を目標にするとき（そのとき crowd の前計算が割に合う）、または EXP-002 の設定で decide が 200 を超えたとき。

### D7. 固定 vs 学習の比較は fork + `Intervention::SetPolicy`（3-5）

`Intervention::SetPolicy { who: All | Ids | Fraction(permille), kind, params: Default | Keep }` を追加する。同じ tick の同じ開始状態から「全員を学習者にした分岐」を作れるので、Toy 1 の fork UI がそのまま比較実験になり、H4 の介入（ランダム化はシード）としても使える。

**実装（3-5、2026-10-17）と設計からの差分**:
- `who` は `All | Id(AgentId) | Fraction { permille }`。**`Ids(Vec)` ではなく `Id` 1 件**にしたのは、`Intervention` と `Event` が `Copy` の固定長レコードであることを保つため（複数 id は介入を複数回適用し、台帳にもその数だけ載る）。
- `Fraction` は各生存個体を独立に確率 permille/1000 で選ぶ。乱数は新ストリーム `Stream::Intervention` を `(seed, tick, id)` で引くので、同じ世界・同じ tick なら必ず同じ集合になり、個体の処理順にも依存しない。「介入は乱数を使わない」という 2a-1 の記述はこれが唯一の例外（台帳からの再生で同じ集合が再現される）。`permille > 1000` は `Invalid`。
- `params: Default` は config の `initial_params` のスカラー + 新しい kind の白紙の表。`Keep` はスカラーを保ち、**新しい kind に合う表だけ**を保つ（Learner → Learner なら学んだ Q をそのまま、Forager → Learner なら白紙の Q、Learner → Forager なら表を捨てる）。
- 身体（エネルギー・体力・在庫・位置・年齢）、方策所有の `memory`、`last_signal`、`recent` は触らない。保存則への影響はなく、台帳は `interventions` を 1 増やすだけ。
- 受け入れ基準の検証: `crates/multiverse/tests/fork.rs::switching_policies_at_a_fork_is_reproducible` が `SetPolicy { All, Learner }` と `SetPolicy { Fraction 300, Reflex, Keep }` を同一 tick で 2 回 fork してハッシュ一致、台帳からの再生（`state_at`）でも一致することを確認する。JSON は `{"op":"set_policy","who":"all","kind":"Learner","params":"default"}` / `"who":{"fraction":{"permille":500}}` / `"who":{"id":7}`。
- CLI: `cz run --set-policy KIND [--set-policy-at TICK]`（既定 tick 0、`All` + `Default`、`--log` に記録）、`cz fork --set-policy KIND`。UI（3-9c）は `WasmWorld::intervene(json)` にそのまま渡せる。

### D8. UI（3-9）は台帳とフレームの拡張だけで作る

- `Frame` に `stats`（その tick の採集・消費・移転・加害・発信数）と `social`（その tick の移転/加害/発信の座標列）を追加。地図上の一時マーカーに使う。
- インスペクタは `events_at` と新規 `agent_events(id, from..to)` で台帳から相互作用を読む（UI の記憶ではなく台帳）。
- 「Compare: fixed vs learning」ボタン = `Fork here` + `SetPolicy`。カーブは人口・摂取量/tick・移転数/tick。

**実装（3-9a、2026-10-17）と設計からの差分**:
- `Frame.stats`（その tick の件数）は置かず、`Frame.ledger`（カーネルの `Ledger` 累計。状態の一部なので巻き戻し・分岐に追随する）を載せ、UI が直近 1 s の累計差分から tick あたりの移転・加害・発信数を出す。採集・消費も `food_consumed` などの累計で読める。
- `Frame.social` はビューが直前に実行した tick（tick − 1）の移転・加害・発信だけで、位置はビューの個体位置（相互作用から最大 1 移動後）。移転・加害は常時記録の台帳から、発信は `record_signals` を要求せず各個体の `last_signal`（状態）から取る。したがって config は変わらず、`state_hash` は `cz run --policy …` と一致したまま。`fromSnapshot` で作った分岐の台帳は原点から始まるので、原点では発信だけが出る。30 tick/フレームでは 30 tick につき 1 tick 分のマーカーしか描かれない（統計は累計なので落ちない）。
- インスペクタは `agent_json(id)`（カーネルが書いた `recent`、既定で最大 8 件）と `agent_events_json(id, from, to)`（台帳。上限なし、死後も残る）の両方を出し、記憶と台帳の違いを見せる。要求ごとの `watch` で選択個体の詳細がフレームに同梱されるので、追加の往復はない。
- 方策構成は `WasmWorld::new(..., policy)`（`forager / random / mixed`。`mixed` は偶数 id が Forager、奇数 id が Random で `cz run --policy mixed` と同じ）。UI の既定は forager のまま（Toy 1 の記録値とゴールデンの経路を保つ）で、ヘッダの選択か `?policy=` で切り替える。3-9c の `SetPolicy` が入るまで、学習方策の選択はない。

**実装（3-9b、2026-10-17）と設計からの差分**:
- スレッドの材料は 3-4 の常時記録イベント（`Proposed / Accepted / Rejected / Transferred`、それに同じ 2 者間の `Harmed`）だけで、新しいイベントも config も要らない。**判定は UI ではなく wasm 境界の Rust（`crates/cz-web/src/thread.rs`）で行い**、UI は返ってきた語彙（kept / open / broken、the give / repayment）を並べるだけ。カーネル（simulation-core）には触れていない。
- **D5 追補の判定を 3-9b の読み方として確定**: 取引 = `Accepted` 1 件（条件を複写しているので単独で立つ）。窓 = `offer_tick ..= offer_tick + by`。提案者の「今」は提案 tick から数える（正直な提案者は返事を待たずに `give` を渡しうるので、D5 の「受諾後」より広い）。`given` = 窓内の提案者 → 受諾者の移転合計、`repaid` = 逆向きの合計。ビューの tick `now` で、両方が条件に達していれば **fulfilled**（画面では kept）、`now > deadline` で不足があれば **broken**（不足側を `broken_by` に。両側もありうる）、それ以外は **open**。カーネルは移転と約束の対応を記録しないので、合計は取引ごとに独立に取る（窓が重なる 2 取引は同じ移転を両方で数える = D5 の式そのもの）。エントリの `deal` / `role` タグは「その向きにまだ不足のある最古の取引」への帰属で、どの約束も要求していない移転はタグなし。これは表示のための読み方であってカーネルの事実ではない。
- `Rejected` は `offer_tick` を持たないので、同じ相手からの直前の提案に紐付ける（`Option`、推定）。カーネルは何も失効させない（期限切れの提案も受諾できる）ので、未応答の提案は `pending` / `lapsed` の表示上の区別だけ。同じ提案の 2 度目の受諾（Random がやる）は別の取引として並ぶ。
- スレッドは「ビューの tick で読んだ台帳」: `now = min(to, view.tick)`、イベントは `from..now`。巻き戻せばスレッドも短くなり、期限内に戻った取引は open に戻る（seek 後も同じ関数 = ビューの関数）。
- 発見のために `deals_json(from, to)`（窓内の全 `Accepted` を同じ判定で）と UI の「Deals · ledger」カード（要求時だけ、フレームには載せない）を足した。Random 歩行者は 14 行動を一様に抽選するので提案 → 受諾 → 移転 → 返済が順にそろう取引は稀で（**既定の seed 42 では random / mixed とも歩行者が絶滅する tick 300 までに 0 件**）、地図から 2 個体を手で選んで見つけることは現実的でない。
- `?seed=` を足した（カーネル worker は元から seed を受け取る。既定は 42 のまま）。seed 23 では random / mixed とも #909 → #363 の取引（tick 17 提案、19 受諾、19 移転、20 返済）が読める。

**実装（3-9c、2026-10-17）と設計からの差分**:
- 「Compare: fixed vs learning」は D8 のとおり **Fork here + `SetPolicy`** で、新しい API はない: A の現在ビューのスナップショットから B を作り、B に `{"op":"set_policy","who":"all","kind":KIND,"params":"default"}` を `intervene` で適用する（D7）。Verify も同じ経路（同一スナップショット + 同一介入）なので学習世界でも MATCH になる — Q 更新が整数演算だから（D6）。B の kind は Learner を既定に Reflex / Forager / Random も選べる（`?policy=learner` の学習世界から固定世界への逆向きの比較もできる）。
- カーブは D8 の「人口・摂取量/tick・移転数/tick」を **人口・摂取量/体/tick・移転/tick・加害/tick・発信/tick の 5 本**にした。摂取量は体あたり（EXP-002 の指標「摂取量/tick/体」と同じ向き。学習世界は人口が違うので総量では比べられない）。率は **直近 50 tick の台帳差分**（`Frame.ledger` の `food_consumed / transfers / harms / signals` の累計をフレームごとに記録し、50 tick 前の点との差を tick 数で割る）なので tick/フレームの設定に依存しない。材料はフレームが元から運ぶ台帳の累計だけで、B の台帳はスナップショットに入って A の値から続く（台帳は状態）。
- 方策構成: cz-web の `policy_mix` が CLI の規則（`mixed-learning` は founding index の `% 3` で Forager / Reflex / Learner、`reflex` と `mixed-learning` は `cz run` の既定どおり突然変異 ON）。したがって `?policy=reflex` の state hash は `cz run --seed 42 --policy reflex`（`--no-mutate` なし）と一致する。
- **Reflex を固定世界の fork で選ぶと身体の軌跡は A と同一**（hash だけ違う）: 突然変異は config（`mutation_enabled`）で、介入は config を変えない（ADR-0010 2a-1）ので、forager 世界の fork では Reflex が Forager 等価の初期重みのまま進化しない。UI のヒントに明記し、進化する Reflex は `reflex` / `mixed-learning` 世界で見る。`SetMutation` のような config 介入は置かなかった（Drought が例外的に再生パラメータを書き換えるのと違い、方策の学習条件を世界の途中で変えるのは比較実験の設計（EXP-002）の側で決めること）。
- 完了条件 4 の「学習方策の世界には別のゴールデン」: `WasmWorld::new(42, 128, 128, 1000, 24, 100, "learner")` を 200 tick 進めた `dynamics_hash` **`cb8bc0a9772c09a4`** を `crates/cz-web/tests/native.rs`（`World::generate(.., Learner)` の直接実行と一致することも検査）と `tests/wasm.rs` の両方で固定した。CI の `wasm-pack test --node` が Forager に加えて Learner の整数 Q 更新経路でも wasm = native を照合する。Learner の規則を意図して変えるときは両方の定数と STATUS のゴールデン表を更新する。
- インスペクタ: Learner は `last_step`（Q 表の行・行動の種類・報酬）と、その行の Q 値 8 個（最大に `*`）を出す。零の初期 Q で未試行の種類が同点で最大になる様子（3-7 の「各状態で各種類を 1 回ずつ試す」）がそのまま読める。Reflex は重みの個数だけ（128 個の表示は 3-10 以降）。
- 性能（Apple M1、内蔵ブラウザ、前面、他の負荷なし、2026-10-17）: A forager ≈ 1,330 体 + B learner ≈ 1,565〜1,600 体の同期再生で、15 tick/フレーム **899 tick/s・60 fps**（A 0.23 ms/tick、B 0.55〜0.57 ms/tick ≈ 350 ns/体·tick = native 238.5 の 1.45 倍）、30 tick/フレームでは B が律速で 1,008〜1,043 tick/s・34〜35 fps（B 0.55〜0.66 ms/tick）。ヘッダの値はフロンティアの差分（6.9 s で 6,090 tick = 884 tick/s）と一致する。製品の門（≥ 60 tick/s）と 15 tick/フレーム × 60 fps の目標を満たす。**学習世界単独**（`?policy=learner`、1,566〜1,602 体、30 tick/フレーム、前面）: **1,100〜1,248 tick/s・37〜42 fps・ワーカー 0.49〜0.59 ms/tick**（フロンティア差分 1,216 tick/s で裏取り。ワーカー律速で、15 tick/フレームなら 60 fps = 900 tick/s の見込み）。ペインが背景だと rAF 1 Hz に間引かれるので、前面で差分と照合した値だけを記録した。

### D9. 実験 EXP-002（3-8）: 学習は Forager に届くか（健全性チェック、事前登録）

- 腕: Random（帰無）、Forager（設計者の上界参照）、Reflex（突然変異 ON）、Learner（32 状態）、Learner-128。
- 条件: 既定世界、20,000 tick、seed 0〜9（Reflex は進化に時間が要るので 50,000 tick も）。
- 指標: 後半 50% の人口の平均と CV、摂取量/tick/体、絶滅率、移転数、加害数。
- 事前の予測（棄却可能）: Random < Learner ≤ Forager。Learner が 10 シード中 8 以上で絶滅しない。Reflex は 50,000 tick で Forager の帯域の 70% 以上に達する。外れたら外れたと記録する。
- 実験仕様は実行前に `docs/research/experiments/EXP-002-learning-vs-fixed.md` に書き、`cz sweep --policy reflex|learner` で実施する。

## タスク分解

| # | タスク | 依存 | 受け入れ基準 |
|---|---|---|---|
| 3-1 ✅ | 空間インデックス + `Observation::neighbors()` + `Perception` 設定。**済 2026-10-17**: `src/spatial.rs`（計数ソート、O(n + cells)）、`tests/neighbors.rs`（順序・半径・端のクリップ・情報制限・proptest で総当たりと一致）。計測: インデックス構築 10 µs、step 132 → 147〜154 µs/tick（+11〜16%、2 回計測）。ゴールデンは config を含まない `dynamics_hash` に再定義（旧符号化の再計算で遷移不変を証明） | – | 近傍が (距離, id) 順で決定論的。serial = parallel。既定世界のゴールデン不変。ベンチ: 1.3k 体で step +20% 以内（計測） |
| 3-2 ✅ | `Transfer` / `Store` / `Harm` 行動、`DeathCause::Violence`、`Transferred` / `Harmed` イベント、`Acted` 記録（設定）。**済 2026-10-17**: `tests/interaction.rs` 8 本、Forager のゴールデン不変（旧定義で証明）、`Acted` は 1,000 体 × 2,000 tick で 220 MB（20,000 tick なら約 2.2 GB → 診断用に限定）。ゴールデンは台帳も除いた物理状態のみに再定義 | 3-1 | 保存則テスト（移転・貯蔵で食料不変、加害コストは `energy_spent`）、隣接判定は S(t)、相手が満杯で部分移転、ゴールデン不変。`record_actions` ON の台帳サイズを計測 |
| 3-3 ✅ | `Signal(Say)`、`last_signal` の観測、記憶リング `recent[K]` とカーネル更新（受領・被害・目撃・聴取）。**済 2026-10-17**: `src/memory.rs`、`tests/memory.rs` 5 本（proptest 32 ケース: serial = parallel で記憶が一致、上限、oldest-first）、Forager のゴールデン不変（空の記憶はバイトを足さない）、スナップショット形式 5。計測: step 154 µs/tick（3-1 の 147〜154 の範囲内）、スナップショット Forager +5%、全個体満杯で約 2.0 倍（D4 参照） | 3-2 | 記憶の内容が決定論的（proptest）、スナップショットの増分を計測（≤ 2 倍）、ゴールデン不変（符号化のみの変更は旧符号化で証明） |
| 3-4 ✅ | 交渉メッセージ `Propose / Accept / Reject` の配達と台帳、`Promised` 記憶。**済 2026-10-17**: 宛先付き配達（`signal_radius`、`OutOfEarshot` / `NoSuchTarget`）、受諾は記憶の `ProposalFrom` を要件に（`NoSuchOffer`）、`Promised` = 自分が渡すと約束した量、常時記録の `Proposed / Accepted / Rejected` イベント、`tests/negotiation.rs` 8 本（提案 → 受諾 → 移転 → 返済の台帳と両者の記憶、不履行で物理不変、拒否、半径外・不在・自分宛・未受領・死者、S(t) での判定、Random 世界の往復と形式 6、proptest 32 ケースで serial = parallel）。ゴールデン不変。Random は 14 行動。スナップショット形式 6。計測は STATUS | 3-3 | 単体テストで「提案 → 受諾 → 移転 → 返済」が台帳と記憶に正しく並ぶ。台本方策（正直な互恵者）で再現。履行は強制されないことのテスト（不履行が起きても物理は進む） |
| 3-5 ✅ | `PolicyKind::{Reflex, Learner}` の枠、`PolicyParams` の拡張（重みベクトル / Q 表）、`Intervention::SetPolicy`、CLI `--policy reflex|learner|mixed-learning`。**済 2026-10-17（枠のみ: 決定規則は 3-6 / 3-7、両 kind は Forager に委譲）**: `PolicyParams.table: Option<PolicyTable>`（`None` はハッシュにバイトを足さずゴールデン不変）、`check_invariants` が方策と表の整合を検査、`SetPolicy { All / Id / Fraction, kind, Default / Keep }`（`Fraction` は `Stream::Intervention` の seed 付き抽選）、config `learner_states` / `learner_inherit_q`、スナップショット形式 7。`tests/set_policy.rs` 7 本 + 介入 proptest の拡張 + `multiverse/tests/fork.rs` の fork 再現性。計測は STATUS | 3-2 | SetPolicy を同一 tick で 2 回 fork してハッシュ一致。スナップショット形式の版上げと互換テスト |
| 3-6 ✅ | Reflex 方策（特徴ベクトル・固定小数点スコア・Forager 等価の初期重み・突然変異 ON）。**済 2026-10-17**: `policy::feature` の 16 特徴、`forager_weights()`、重みの突然変異、`tests/reflex.rs` 8 本 + proptest。Forager のゴールデン不変。計測: decide 初期重み **147 ns**（基準内）、**進化後 182 ns（当初の基準 150 は超過。2026-10-17 の改定で進化後は ≤ 200 とし、満たす。D6 参照）** | 3-3, 3-5 | 初期重みの Reflex が Forager と 1,000 tick 同一決定（ハッシュ一致）。`tests/forager.rs` 相当の決定規則テスト。1.3k 体で decide ≤ 150 ns/体·tick（計測）。**改定（2026-10-17、D6）: 初期重み ≤ 150、進化後 ≤ 200** |
| 3-7 ✅ | Learner 方策（離散状態・整数 Q・決定論的 ε-greedy・継承）。**済 2026-10-17**: `policy/learner.rs`（7 ビット 128 状態の格子を `learner_states` で折る、ε-greedy は config `learner_epsilon_permille` 既定 50、Reflex と同じアクチュエータ）、Q 更新はカーネルの Apply Phase 5a（`Intent.state` が決定した行を運び、`Agent.last_step` が前 tick の (行, 行動, 報酬) を持つ。報酬 = Upkeep までのエネルギー差分 /100、出生の移転は差し引かない、固定小数点 16、i16 飽和）、スナップショット形式 8。`tests/learner.rs` 11 本 + 単体 4 本 + `multiverse/tests/replay.rs` の Learner 世界。Forager のゴールデン不変。計測は STATUS | 3-3, 3-5 | 再生で bit 一致。スナップショット 1,300 体 ≤ 1 MB。同じ状態列で Q が単調に更新されるユニットテスト。1.3k 体で decide ≤ 200 ns/体·tick |
| 3-8 | EXP-002（事前登録 → 実行 → 記録）、`cz sweep` の方策拡張と新指標（摂取量・移転・加害） | 3-6, 3-7 | 実験仕様が実行前にコミットされている。10 シードの結果表と限界の記述。予測の当否を明記 |
| 3-9a ✅ | UI: `Frame.social` マーカー、`Frame.ledger` の集計、インスペクタの記憶・発信・相互作用（台帳から）、世界初期化の方策構成。**済 2026-10-17**: `crates/cz-web` に `social()`（直前 tick の移転・加害を台帳から、発信は各個体の `last_signal` から、ビューの位置に置いた `Int32Array`）、`ledger_json()`、`agent_json(id)`（`recent` と `last_signal` を含む `Agent`）、`agent_events_json(id, from, to)`（D8 の `agent_events`）、コンストラクタの `policy`（`forager / random / mixed`、`cz run --policy` と同じ割当規則）。`web/` はプロトコル v3（`Frame.social / ledger`、要求ごとの `watch`、`WorldInit.policy`）、地図マーカー（0.6 s で消える軌跡）、統計（台帳の累計と直近 1 s の tick あたり xfer / harm / sig）、インスペクタ（記憶を tick 付き、台帳の直近 100 tick）、ヘッダの世界選択。`tests/native.rs` +4、`tests/wasm.rs` +1。ブラウザ確認と計測は STATUS（ローカルと公開 URL の両方で確認済み） | 3-3（UI 骨格は並行可） | 公開 URL で、個体を選ぶと最近の相互作用が tick 付きで見える。移転・加害・発信が地図に出る |
| 3-9b ✅ | UI: 交渉スレッド表示（2 個体間の Propose/Accept/Transfer の時系列）。**済 2026-10-17**: `crates/cz-web/src/thread.rs`（2 者を名指しする台帳イベントを台帳順に、各 `Accepted` を「取引」として提案 tick〜期限の窓の移転から判定: 両側が渡せば kept、期限を過ぎて不足なら broken（不足側を名指し）、それまでは open。拒否は同じ相手からの直前の提案に紐付け、移転はまだ不足のある最古の取引に帰属）、`WasmWorld::thread_json(a, b, from, to)` と、スレッドを見つけるための `deals_json(from, to)`。`web/` はプロトコル **v4**（`Watch.partner` → `Watched.thread`、`deals` 要求、`?seed=`）、インスペクタの「Thread · with #id」（相手の候補は記憶と台帳の相手、交渉相手を先に）と「Deals · ledger」カード。`tests/native.rs` +6（手組みの台帳で履行 / 不履行 / 拒否の紐付け / 重なる取引、Random 世界で `agent_events_json` との整合とビュー依存、seed 23 の取引と seed 42 に順序どおりの取引が無いことを固定）、`tests/wasm.rs` +1。カーネル不変。ブラウザ確認は STATUS（既定の seed 42 では 4 手順そろった取引が無いので `?seed=23`） | 3-4, 3-9a | 台本方策の世界で提案 → 受諾 → 移転 → 返済のスレッドが読める |
| 3-9c ✅ | UI: 「Compare: fixed vs learning」（fork + SetPolicy）、人口・摂取量・移転数カーブ、方策選択（世界の初期化）。**済 2026-10-17**: `crates/cz-web` の `policy_mix` に `reflex / learner / mixed-learning`（`cz run --policy` と同じ割当、evolving な mix は突然変異 ON）、`agents()` の policy 列を `policy_code`（0 forager / 1 random / 2 reflex / 3 learner）に、Learner 世界のゴールデン `cb8bc0a9772c09a4`（seed 42、既定 config、tick 200、native = wasm）。`web/` はプロトコル **v5**（`set_policy` 介入、`PolicyKind` 4 種、`AgentDetail.params.table / last_step`）、Branches カードの「Compare: fixed vs learning」= 現 tick で fork + `SetPolicy { All, kind, Default }`、「Compare · A solid, B dashed」の 5 カーブ、介入パネルの Set policy、地図の 4 色、インスペクタの last step と Q 行。ブラウザ: fork 18〜23 ms、A（forager）+ B（学習）の同期再生 15 tick/フレームで **899 tick/s・60 fps**（B 0.55 ms/tick）、Verify = MATCH（143 ms）、A / B の tick 600 のハッシュが CLI と一致。計測は STATUS | 3-5, 3-9a | PRODUCT.md Phase 3 デモ (d)。学習世界で ≥ 60 tick/s を画面に表示。Verify = MATCH |
| 3-10 | 文書: ゴールデン表、STATUS、RISKS 追補（加害行動が生む退化均衡、報酬ハッキング、解釈可能性）、TUTORIAL 追補 | 全て | 新セッションが STATUS から Phase 3 の状態を再構築できる |

並行可能: 3-9a は 3-2 以降 UI 側で先行できる。3-6 と 3-7 は 3-5 の後で並行できる。3-8 は 3-6/3-7 のどちらかが終わり次第その腕から始められる。

## 却下した / 先送りした案

- **全エージェントを LLM にする**: PRODUCT.md 原則 2 と master prompt により不採用。LLM は Phase 3 に登場しない。
- **ニューラル方策（f32）**: 決定論の契約を弱める（ADR-0003）。固定小数点の線形方策と表形式 Q で始め、表現力が足りないと実験が示したら固定小数点 NN を検討する（Phase 6）。
- **交換の強制（エスクロー）をカーネルに入れる**: 約束の履行を物理にすると、制度（違反と制裁）が実験できなくなる。R4 に反するので不採用。
- **第 2 の資源**: 交換の動機を増やすが物理の拡張が大きい。Phase 3 では貸し借りで足りるかを見る。
- **Q 表の集団共有（文化的学習）**: 省メモリだが「個体の学習」と区別がつかなくなる。設定オプションとして残し、既定は個体ごと。

## リスク

- `Harm` が入ると「全員が殴り合って絶滅」の退化均衡が学習で現れうる。それ自体は結果として記録するが、デモ世界の既定ではコストを高めに置く。
- Learner の報酬（自己エネルギー）では分かち合いは出ない。出ないことを「失敗」と扱わず、Phase 4 の前提条件として記録する。
- 記憶と Q 表でスナップショットが数倍になる。2b-4 の巻き戻し時間（復号を含む）を再計測し、間隔を調整する。
- 方策の内部が見えにくくなる。インスペクタに重みの要約と Q の上位行動を出す（3-9a）。

## 帰結

- 遷移関数が拡張される。Forager 世界のゴールデンは保つ設計だが、符号化変更は起きる（記憶・Q 表のハッシュ化）。各タスクで理由を STATUS の表に記録する。
- Phase 4 は、3-2〜3-4 の台帳（移転・加害・提案・履行/不履行）の上に「違反の定義 → 制裁の選択 → 規範の提案と合意」を載せる。Phase 3 の設計はそれを前提に、物理と社会的事実の境界を越えない。
