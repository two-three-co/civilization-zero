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
| 3-6 | Reflex 方策（特徴ベクトル・固定小数点スコア・Forager 等価の初期重み・突然変異 ON） | 3-3, 3-5 | 初期重みの Reflex が Forager と 1,000 tick 同一決定（ハッシュ一致）。`tests/forager.rs` 相当の決定規則テスト。1.3k 体で decide ≤ 150 ns/体·tick（計測） |
| 3-7 | Learner 方策（離散状態・整数 Q・決定論的 ε-greedy・継承） | 3-3, 3-5 | 再生で bit 一致。スナップショット 1,300 体 ≤ 1 MB。同じ状態列で Q が単調に更新されるユニットテスト。1.3k 体で decide ≤ 200 ns/体·tick |
| 3-8 | EXP-002（事前登録 → 実行 → 記録）、`cz sweep` の方策拡張と新指標（摂取量・移転・加害） | 3-6, 3-7 | 実験仕様が実行前にコミットされている。10 シードの結果表と限界の記述。予測の当否を明記 |
| 3-9a ✅ | UI: `Frame.social` マーカー、`Frame.ledger` の集計、インスペクタの記憶・発信・相互作用（台帳から）、世界初期化の方策構成。**済 2026-10-17**: `crates/cz-web` に `social()`（直前 tick の移転・加害を台帳から、発信は各個体の `last_signal` から、ビューの位置に置いた `Int32Array`）、`ledger_json()`、`agent_json(id)`（`recent` と `last_signal` を含む `Agent`）、`agent_events_json(id, from, to)`（D8 の `agent_events`）、コンストラクタの `policy`（`forager / random / mixed`、`cz run --policy` と同じ割当規則）。`web/` はプロトコル v3（`Frame.social / ledger`、要求ごとの `watch`、`WorldInit.policy`）、地図マーカー（0.6 s で消える軌跡）、統計（台帳の累計と直近 1 s の tick あたり xfer / harm / sig）、インスペクタ（記憶を tick 付き、台帳の直近 100 tick）、ヘッダの世界選択。`tests/native.rs` +4、`tests/wasm.rs` +1。ブラウザ確認と計測は STATUS（ローカルと公開 URL の両方で確認済み） | 3-3（UI 骨格は並行可） | 公開 URL で、個体を選ぶと最近の相互作用が tick 付きで見える。移転・加害・発信が地図に出る |
| 3-9b | UI: 交渉スレッド表示（2 個体間の Propose/Accept/Transfer の時系列） | 3-4, 3-9a | 台本方策の世界で提案 → 受諾 → 移転 → 返済のスレッドが読める |
| 3-9c | UI: 「Compare: fixed vs learning」（fork + SetPolicy）、人口・摂取量・移転数カーブ、方策選択（世界の初期化） | 3-5, 3-9a | PRODUCT.md Phase 3 デモ (d)。学習世界で ≥ 60 tick/s を画面に表示。Verify = MATCH |
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
