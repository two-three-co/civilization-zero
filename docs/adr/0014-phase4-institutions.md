# ADR-0014: Phase 4 — Institutions（Nomos Engine / Institution DSL）の設計

- 状態: 提案 → 採用 (2026-10-17)。**4-1（社会的事実の追跡）実装済み（2026-10-17）**、4-2 以降は未着手。各タスクの着手・完了は STATUS.md の表に記録し、設計からの差分は ADR-0013 と同じく各 D の「実装と設計からの差分」として追記する
- 前提: docs/PRODUCT.md（Phase 4 のデモ = 制度エクスプローラ）、docs/research/RISKS.md（R1, R4, R5, R8〜R11）、docs/research/HYPOTHESES.md（H1〜H3）、docs/research/experiments/EXP-002（前提条件）、ADR-0003（整数のみ）、ADR-0004（座標関数の乱数）、ADR-0005（方策は純関数）、ADR-0008（イベント形式）、ADR-0010 / 0012（台帳・再生・fork）、ADR-0013（社会的プリミティブと学習方策）
- 用語: **Nomos Engine** = この ADR が置く 3 つの部品の総称。(1) 世界の中の社会的事実（先占・約束・違反・係争・制裁）と制度（規則・憲章・動議）の**状態**、(2) それらを台帳と状態から計算するカーネルの**純関数**（判定・集計・施行）、(3) 台帳と状態を読む**読み手**（`cz sweep` の指標、cz-web の JSON、制度エクスプローラ）。**Institution DSL** = (1) のうち規範と憲章のデータ型（D5）。LLM はどこにも登場しない

## 文脈

Phase 3 で個体は互いを見て、渡し、殴り、覚え、伝え、約束し、学ぶようになった（ADR-0013）。台帳には `Transferred / Harmed / Proposed / Accepted / Rejected` が常時載り、記憶には `Promised` が残る。しかし「約束を破った」「他人の場所から採った」は、まだ世界の中では誰にとっても何でもない。cz-web の `thread.rs` が不履行を表示のために判定するだけで（3-9b）、世界では何も起きない。

Phase 4 の問いは「人工社会は、自らを統治する実行可能なルールを発明できるか」の最初の半分 — **ルールが世界の中の社会的事実として存在し、個体がそれを守り・破り・罰することができ、その効果を事前登録した指標で測れる**こと — である。発明（学習による立法）は後半で、本 ADR は手続きだけを用意する（D7）。

EXP-002 が示した前提条件（RISKS R8〜R10）は制度の設計を直接縛る:

1. 制裁のない既定世界で、Learner の死因の 43% が加害死で、その源は ε = 5% の探索である。制裁の指標は「この暴力水準からの差」でしか読めない。
2. 報酬は自己エネルギーのみで、分かち合いは出ない。加害は体力を減らすがエネルギーを減らさないので、**加害による制裁は Learner の報酬に現れない**。制裁が「痛い」ことが学習の前提になる（D6, D10）。
3. 死に終端報酬がない。ただし Q 表は個体と一緒に死ぬので、終端報酬は構造的に効かない（D10 で却下）。

また、H1 の指標「先占者以外の採取率」に要る**先占**は、Phase 3 の台帳にも状態にもない。採集は常時記録されない（`Acted` は 1,000 体 × 2,000 tick で 220 MB）ので、先占は台帳からではなく状態として追跡する必要がある（D3）。

## 完了条件

1. **ブラウザのデモ（PRODUCT.md Phase 4 = 制度エクスプローラ）**: 公開 URL で、(a) 制度の時系列（動議 → 投票の集計 → 施行 / 否決、布告による施行・廃止・憲章変更）が台帳から読める、(b) 違反が「誰が・どの規則を・誰に対して・いつ」と根拠（侵害したセル / 破った約束 / 加害）付きで一覧できる、(c) 制裁が、それが答えた違反（係争）に結び付けて一覧でき、地図にも出る、(d) 侵害率（方策別）・違反/tick・制裁の条件付き確率のカーブが描ける、(e) 「Compare: with vs without rule」で同じ世界から規則あり / なしを fork して (d) を並べ、Verify = MATCH になる。
2. **計測**（Apple M1、機種・コミット・日付付きで STATUS に）: 制度世界（D10 の `nomos_preset`、Learner 128 行 90% + Enforcer 10%、先占規範を施行、約 1,300 体）で native ≥ **1,800 tick/s**、ブラウザで ≥ **60 tick/s** を画面に表示（目標は 2 世界同期 15 tick/フレーム × 60 fps = 900 tick/s、3-9c と同じ）。decide は Reflex 進化後・Learner とも ≤ **200 ns/体·tick**（ADR-0013 D6 の基準。特徴 21 個・規則あり・係争ありで）。Nomos の追加コスト（NormIndex 構築 + 判定 + 集計）は step の **≤ 20%**、NormIndex 構築単体 ≤ 30 µs/tick（係争 ≤ 2,000 件）。バイナリスナップショットは既定世界（Learner 32 行）で ≤ **1 MB** のまま、制度世界（128 行）で ≤ **2 MB**。巻き戻し（復号 + 再実行、R11）は学習世界・制度世界で最悪 ≤ 200 ms。wasm は ≤ 400 KB。
3. **事前登録実験**: EXP-003（制度のない世界の前提条件の格子、D10）と EXP-004（H1、D11）を「仕様コミット → 実行 → 記録」の順で、各腕 ≥ 10 シード、EXP-004 の主対比は 30 シード、帰無モデル付き。結果の記述に「創発」「世界初」「所有権が生まれた」を使わない。
4. **決定論**: `config.nomos` がオフの世界は 4-1〜4-9 を通して遷移不変（Forager のゴールデン `0x658ee873d1866501`、Learner 32 行のゴールデン `0xcb8bc0a9772c09a4`、EXP-002 の F / X0 / X1 / X1-50k / L32 の `_series.csv` が再実行で全行一致）。制度世界には第 3 のゴールデンを置き、CI の wasm 照合に加える。スナップショット + 台帳からの再生が、動議・施行・制裁を挟んでも bit 一致。
5. **台帳**: 制度の出来事（動議・閉会・施行・廃止・憲章・制裁）は常にイベントとして載り、再生で照合される。違反の個別記録と投票の個別記録は設定で ON にでき、サイズを計測して既定を決める（D9）。
6. **原則**: 判定・集計・施行・文の生成はすべて純関数で、LLM は使わない。制度名の行動（Own / Claim / Punish）を作らない（R1）。

## 決定（設計）

### D1. 制度は社会的事実であり、物理を変えない（R4）

制度が世界に作用する経路は 2 つだけ: **(a)** 観測に規範情報を加える（`Observation.norms`、D6）、**(b)** 制裁などの行動を他者に起こさせる（個体が `Harm` や `Reject` を係争中の個体に向けて選ぶ、D4 / D6 / D7）。

カーネルは以下を**しない**: 禁止行動を実行不能にする（`Failure` は物理的・手続き的不能のみ。違反は `Failure` を生まない）、違反者からエネルギーや食料を取る（罰金・国庫）、コストやダメージを規則で変える、違反者の行動を無効化する。`Harm` の物理（`harm_cost`、`harm_damage`、`Violence` 死）は制裁として使われても 1 バイトも変わらない。制裁とは「係争中の個体に向けられた、台帳に載る相互作用」の**読み方**であり（D4）、物理的には他の加害・拒否と同じである。

`nomos` オフの世界（既定）では、本 ADR のコードは `dynamics_hash` に 1 バイトも足さず、決定も遷移も変えない（完了条件 4）。

### D2. 置き場所: `simulation-core/src/nomos/` モジュール、状態は `World.nomos`、派生は `NormIndex`

- ARCHITECTURE の「`nomos` クレート（Institution DSL / 解釈器）」は **simulation-core 内のモジュール**に置き換える（ADR-0013 D1 と同じ理由）。経路 (a) は Decide が規範情報を読むことを要求し、判定は Apply の一部なので、制度は遷移関数の一部である。別クレートにすると simulation-core が上位に依存するか、再生に外部ループが要る。cz-web / cz-cli は読み手として上に載る。
- 状態 `World.nomos: Nomos { charter, rules: Vec<Rule>, next_rule_id: u16, motion: Option<Motion>, next_motion_id: u16, standing: Vec<Standing>, deals: Vec<OpenDeal> }`。スナップショットに入る（形式 9）。`check_invariants` が検査する（係争中の個体は生存し (agent, rule) で整列、規則 id は一意で `kind` ごとに 1 つ、投票者は生存・id 整列・賛否で重複なし、`arrived ≤ tick`）。
- 派生 `NormIndex`: tick 開始時に `SpatialIndex` の直後に `nomos.standing` と `rules` から作る（保存しない、Decide 中は読み取り専用）。個体 index ごとのフラグ（`ACCUSED` = 係争中、`HARMABLE` = HARM 制裁を許す規則で係争中、`REJECTABLE`、`VOTED` = 開いている動議に投票済み）を `Vec<u8>` に持つ。構築は係争 1 件につき id の二分探索 1 回（HashMap なし）。規則がなければ空（長さ 0）で、`Observation.norms` の問い合わせはすべて定数時間で「なし」を返す。
- スイッチ `WorldConfig.nomos: bool`（既定 false、`serde(default)`）。ON で先占の記録・約束の監査・侵害と不履行のカウンタ・制度の判定・動議の受付がすべて有効になる。OFF なら `World.nomos` は空で `Agent.arrived` は `None`、ハッシュにバイトを足さない。1 つのスイッチにしたのは、EXP-002 との比較可能性（既定の世界は今日と同一）と、「追跡だけ ON（制度なし）」= H1 のベースラインを 1 条件で作るため。

### D3. 追跡する社会的事実: 先占、約束の期限、侵害

H1 の指標には「先占者」が要る。Phase 3 の状態にも台帳にもないので、`nomos` オンの世界だけが次を追跡する。

- **先占 = 到着順**。`Agent.arrived: Option<u64>` = 今のセルに立ち始めた tick。Apply Phase 4 で位置が変わるたびに `tick + 1`、出生は**親の `arrived` を継承**（家族は共同の先占者。子が親のセルで採る採集を侵害にしないため）、`SpawnAgents` は介入の tick、創設個体は `None` = 最古。`nomos` オフでは常に `None`（バイトを足さない）。
- **侵害（encroachment、先占者以外の採取）の操作的定義**: tick t に個体 A がセル c で 1 単位以上採集に成功し、S(t) で c に立つ他の生存個体のうち `arrived` が最小の B（同点は id の小さい方）が `arrived(B) < arrived(A)`（厳密。`None` < 任意の `Some`。同 tick の到着どうしは侵害にならない）を満たすとき、A は B に対して侵害した。B は**常に同じセルにいる**ので知覚半径に関係なく観測できる（`social_radius = 0` でも同セルは見える）。Apply Phase 3（採集の競合解決）の直後に、採集があったセルだけで判定する: O(競合セルの個体数)。
- 侵害は規則がなくても**事実として数える**: `Ledger.gathers`（成功した採集件数）と `Ledger.encroachments`、`TickReport` の方策別の採集と侵害（EXP-004 の「方策別の侵害率」用）、先占者の記憶に `EncroachedBy { other: A, value: 量 }`（Phase 5b）。侵害が**違反**になるのは `Encroach` の規則が施行されているときだけ（D4）。
- 「投資」（R1 の「最初に資源へ到達 / 投資した個体」）は Phase 4 では到着のみ。`Store` による先占や「直近に採集していること」を要件にする変種は Phase 5 の H2 の格子に送る（却下した案）。
- **約束の期限の監査**。`Accepted` が届いた tick に `nomos.deals` へ `OpenDeal { proposer, acceptor, offer_tick, deadline, give, want, given, repaid }` を積み、以後の `Transferred` で `given / repaid` を加算、`deadline`（= `offer_tick + by`）の tick の Apply Phase 3c で決着する: `given ≥ give ∧ repaid ≥ want` なら履行、不足側が**不履行**（3-9b の `thread.rs` と同じ定義。窓が重なる取引は同じ移転を両方で数える = ADR-0013 D5 追補のとおり）。`Ledger.promises_due / promises_broken` は規則がなくても数える。不履行が**違反**になるのは `BrokenPromise` の規則が施行されているときだけ。
- **報酬の体力項**（前提条件 2 への手当て。追跡と同じ 4-1 で入れる）: `WorldConfig.learner_health_weight: u16`（既定 0）。Learner の報酬 = Δエネルギー/100 + `weight` × Δ体力（S(t) から Upkeep 終了まで。加害・飢餓ダメージ・Rest の回復を含む）。既定 0 で Learner のゴールデン不変。値は EXP-003 で決める（D10）。

**実装（4-1、2026-10-17）と設計からの差分**:

- 置き場所は `simulation-core/src/nomos.rs`（`policy.rs` と同じく 1 ファイル。4-2 で `nomos/` に分割してよい）。入ったもの: `World.nomos: Nomos { deals }`、`Agent.arrived`、`InteractionKind::EncroachedBy`（末尾、`kind as u64` = 10）、台帳 `gathers / encroachments / promises_due / promises_broken`、`TickReport.gathers / encroachments`（方策別 `[u32; PolicyKind::COUNT]`）/ `promises_settled: Vec<OpenDeal>` / `learner_rows`、config `nomos` / `learner_health_weight`、CLI `--nomos` / `--learner-health-weight`（`run` / `sweep` / `fork`）。
- **先占と侵害の判定は設計どおり**（Apply Phase 3 の直後、採集があったセルだけ、`None < Some`、同 tick の到着どうしは侵害でない、同点は id の小さい方）。走査はセル index 順（`claims` の順）、セル内の採集者は id 順にそろえてから判定するので、4-2 の `Violated` の順序「セル順・採集者 id 順」はそのまま出る。`EncroachedBy` は 3b の記憶より前に `recent` へ積む（起きた順。採集は 3b より先）。
- **`Ledger.gathers` は `nomos` に関係なく常に数える**（成功した採集件数は物理の計数で、`state_hash` にだけ入る。`TickReport.gathers` の方策別も常時）。`encroachments / promises_*` はオフでは常に 0。
- **約束の監査の窓は受諾 tick から**: `OpenDeal` に `accepted_tick` を持ち、受諾 tick の移転は行為者順に関係なく数え（3b の後の Phase 3c で tick 全体を集計してから決着）、期限 tick（受諾が期限より後ならその受諾 tick）に決着する。3-9b の `thread.rs` は提案 tick から数えるので、**提案から受諾までの間の移転は `thread.rs` だけが見る**（カーネルは移転の履歴を持たず、取引は存在した時点から数える）。この差は `crates/cz-web/tests/nomos_audit.rs` が固定している: 台本 9 件のうち 8 件で kept / broken と不足側が一致し、「提案 tick に `give` を先渡しした 1 件」だけ `thread.rs` は履行・カーネルは提案者の不履行と読む。Random 混在世界 300 tick では期限の来た 156 件のうち 151 件が一致、5 件（提案〜受諾の間に当事者間の移転あり）を除外。先渡しで履行する方策が現れたら（Enforcer の (2) は `Promised` の後なので該当しない）、4-7 で `thread.rs` をカーネルの定義（`nomos.deals` と `promises_settled`）に寄せる。
- `promises_broken` は取引単位（不足側が 1 でも 2 でも 1 件）。不足側は `TickReport.promises_settled` の `OpenDeal::proposer_short / acceptor_short` で読む（4-2 の `Violated` の材料）。
- **記憶の競合**: `EncroachedBy` は `memory_slots`（既定 8）を他の記憶と分け合う。したがって `nomos` オンでは、`recent` を決定に使う方策（Reflex の記憶特徴と `HEARD`、Random の「最新の提案」、Learner 64 行以上の `HEARD`）の決定が変わりうる。Forager と Learner 32 行は記憶を決定に使わない（Learner が発する `Say` の記号だけが変わりうる）ので、EXP-002 の forager / learner32 の series は `nomos` オンでも全行一致（受け入れ基準どおり。`tests/nomos_facts.rs::tracking_changes_no_decision_in_forager_and_learner_worlds` は 300 tick の行動・失敗・イベント・身体・資源・`last_step` の一致）。EXP-003 / 004 の世界は全条件 `nomos` オンで走るので、この効果は条件間で共通。
- **体力項**は Upkeep 直後の `health − S(t) の health`（3b の加害、5 の飢餓ダメージと Rest の回復を含む）に `learner_health_weight` を掛けて `Δenergy/100` に足し、i16 に飽和する（`learner::reward_with_health`）。重み 0 なら 3-7 と同一の算術で、Learner のゴールデンは不変。
- `TickReport.learner_rows` は学習者が決定した tick だけ `learner_states` 個の計数を持つ（表の外の行 = 手組みの Intent は数えない）。`cz sweep` の `_qpolicy.csv` に `visits`（全区間）と `visits_window`（窓）。
- `check_invariants`: オフなら `arrived` は全員 `None` で `nomos` は空、オンなら `arrived ≤ tick`、開いている取引は過去に受諾・受諾順・期限未到来（期限 tick に決着するので）。
- スナップショット形式は **4-1 で 9**（`Agent` と `World` の形が変わった。8 を拒否）。4-2 で `Nomos` に規則・係争・動議が入れば 10 になる。
- `thread.rs` と `web/` は不変（`agent_json` に `arrived` が増えるだけ。UI は 4-7 / 4-8）。`Event` / `Failure` / `Intervention` に新変種はない（4-2 以降）。

### D4. 違反・係争・制裁の判定（カーネルの純関数、Apply の固定位置）

- **違反の 3 述語**（規範の `kind`）。すべて台帳に載るイベントと D3 の状態だけから、その tick の Apply で判定する。乱数も LLM も使わない。

| `NormKind` | 違反者 → 相手 | 判定の材料と時点 |
|---|---|---|
| `Encroach` | 採集者 A → 先占者 B | D3 の侵害。Phase 3 の直後 |
| `BrokenPromise` | 不足側 → 相手方 | D3 の監査。期限 tick の Phase 3c（移転の解決後） |
| `Harm` | 加害者 → 被害者 | `Harmed { by, target }`。Phase 3b。ただしその加害が下の定義で**制裁**であり、規範の `except_sanction` が真なら違反ではない |

- **係争中（standing）**: 違反者 A は (A, 規則) ごとに 1 つの `Standing { agent, rule, since, last, against, violations: u16, sanctions: u16 }` を持ち、新しい違反は `last` と `violations` を更新する。S(t+1) から `last + standing_ticks` の tick まで係争中（NormIndex はこれから作る）。期限が過ぎるか A が死ぬと**エピソード**が閉じ、`Ledger.episodes_closed[kind] += 1`、`sanctions > 0` なら `episodes_sanctioned[kind] += 1`。違反ごとではなくエピソードごとに 1 つにしたのは、係争の件数を `alive × rules` で有界にし（NormIndex の構築時間の上限）、慢性の違反者が制裁の分母を独占しないため。
- **制裁の識別**: tick t の `Harmed { by: S, target: V }` で V が S(t) で HARM を許す規則の係争中なら `Sanctioned { tick, by: S, target: V, rule, via: Harm }`（常時記録）、同様に `Rejected { from: S, to: V }` で REJECT を許す規則なら `via: Reject`。V が複数の規則で係争中なら最も古いエピソードに帰属（3-9b の移転の帰属と同じ）。該当する `Standing.sanctions += 1`、`Ledger.sanctions[kind] += 1`。記憶は `HarmedBy / Harmed / SawHarm` の代わりに **`SanctionedBy / Sanctioned / SawSanction`**（`SawSanction` は被害者から `social_radius` 内の目撃者。規則がなければ従来どおり）。物理は加害・拒否そのもので、何も変わらない（D1）。
- **H1 の条件付き確率**は台帳のカウンタから: `S = episodes_sanctioned / episodes_closed`（エピソード単位）と `S' = sanctions / violations`（件数単位）を両方出す。侵害率は `E = encroachments / gathers`（方策別も）。イベントの個別記録なしで計算できるので、ブラウザの率カーブと `cz sweep` は同じ数字を読む。
- **`Failure` の原則の拡張**: `Failure` は「カーネルがその行為を遂行できなかった」こと — 物理的不能に、D7 の手続き的不能（`FloorTaken` など）を加える — だけを表し、規則違反は決して `Failure` にならない。CLAUDE.md の「`Failure` は物理的不能のみ。規則違反は別概念」は、この ADR で「物理的・手続き的不能のみ」に読み替える（4-10 で CLAUDE.md に反映）。

### D5. Institution DSL の最小形: `Norm` と `Charter`、表現できることとできないこと

- **規範** `Norm { kind: NormKind, standing_ticks: u16, sanction: Sanction, except_sanction: bool }`（`Copy`、8 B）。`NormKind ∈ { Encroach, BrokenPromise, Harm }`、`Sanction` は `HARM` / `REJECT` のビット集合（空も可 = 係争はするが制裁は認めない）。`except_sanction` は `Harm` の規範でだけ意味を持つ（真なら、係争中の個体への加害は制裁として免責）。
- **規則** `Rule { id: u16（単調、再利用なし）, norm: Norm, enacted: u64 }`。同時に施行できるのは **`kind` ごとに 1 つ**（最大 3。判定の一意性）。施行済みの `kind` への `Enact` は**置換**（旧規則の `Repealed` と新規則の `Enacted`）= 改正。
- **憲章** `Charter { quorum_permille: u16（既定 100）, threshold_permille: u16（既定 667）, vote_ticks: u16（既定 32）, amendable: bool（既定 true）}`。config の `charter` が初期値で、介入か動議で変わる。
- **表現できること**: 3 種の違反の定義 × 係争期間 × 制裁の種類 × 制裁の免責、および合意の定足数・閾値・期限・改正の可否。これで H1（先占と制裁）、H2 の一部（制裁コスト・係争期間の格子）、H3 の手続き（改正）が書ける。
- **表現できないこと（R1、意図的）**: 特定の個体・セル・数量に言及する規則（人名法・区画法なし）、行為の義務（「制裁せよ」「分けよ」。制裁しないことは違反ではない = メタ規範なし）、物理の変更（コスト・ダメージ・禁止）、条件付き規則（「食料が少ないときは」。環境に応じる制度は改正で表す）、規則についての規則、複数の制度・地域制度、自由形式の述語。語彙は 3 述語 × 4 パラメータで、UI の文はテンプレートから決定的に生成する（例: `Rule #3 · first occupant: taking from a cell someone reached before you is a violation; a violator stands accused for 32 ticks and may be harmed.`）。LLM は文の生成にも使わない。
- 表現力を増やす順序は実験が決める: 先占の根拠（`Store` / 直近の採集）、地域制度（半径）、知覚制限付きの係争（D6 の `see_standing` の個体別化）は Phase 5 の候補として「却下した / 先送りした案」に置く。

### D6. 観測への規範情報と、学習方策がそれを学べる形

- **`Observation.norms: &NormIndex`** と問い合わせ: `occupied()`（自セルに自分より早く到着した他個体がいる = 採ると侵害になる）、`my_standing()`、`adjacent_violator()`（距離 1 以内で HARM 制裁を許す規則の係争中の個体を (距離, id) 順で最初の 1 体。`nearest_adjacent` と同じ走査）、`rule(kind)`、`open_motion()`、`i_voted()`。`occupied()` は `nomos` オンなら規則がなくても真になりうる（物理に近い事実）。他は規則がなければ常に偽 / `None`。
- **`Perception.see_standing: bool`（既定 true）**: 偽なら学習方策の規範特徴（下の `RULE_ACTIVE / ACCUSED / VIOLATOR_ADJACENT / SANCTIONED_RECENTLY`）が常に 0 で、Harm のアクチュエータも係争を見ない（「盲目」の帰無モデル）。`OCCUPIED` は物理に近い事実なので対象外。台本方策 `Enforcer`（D7）はこのフラグに関わらず NormIndex を直接読む（陽性対照だから）。
- **Reflex: 16 → 21 特徴**。追加は `OCCUPIED`(16) / `RULE_ACTIVE`(17、施行中の規則が 1 つでもある) / `ACCUSED`(18) / `VIOLATOR_ADJACENT`(19) / `SANCTIONED_RECENTLY`(20、`SanctionedBy` が 16 tick 以内)。`forager_weights()` は新しい 5 行を零にし、`check_invariants` は長さ 168 を検査する。突然変異は既存の 128 個を先に引き、新しい 40 個をその後に同じストリームから引く（`RngStream` はカウンタ型なので先頭 128 個の値は変わらない）。したがって `nomos` オフの Reflex 世界（EXP-002 の X1 / X1-50k）は突然変異 ON でも軌跡が変わらない（完了条件 4 の series 一致で確認）。`SAW_HARM_RECENTLY` は `SawHarm ∪ SawSanction` を数える（規則のない世界では同義）。
- **Learner: ビット順を変え、規範ビットを下位に置く**。`HUNGRY(0) / HAS_FOOD(1) / FOOD_HERE(2) / FOOD_IN_SIGHT(3) / ADJACENT(4) / OCCUPIED(5) / VIOLATOR_ADJACENT(6) / ACCUSED(7) / UPPER(8) / HEARD(9)`、`FINE_STATES` 1024（= `MAX_LEARNER_STATES`）。32 行は従来どおり下位 5 ビットで**ゴールデン不変**。64 行で `OCCUPIED`（遵守の学習に必要）、128 行で `VIOLATOR_ADJACENT`（制裁の学習に必要）、256 行で `ACCUSED` が見える。`nomos` オフ（`arrived` が全員 `None`、規則なし）なら新ビットは常に 0 で、**64〜256 行の Learner は 32 行と同一の決定をする**（行 0〜31 しか訪れない。テストで固定。512 行以上は `UPPER` が入るので別）。EXP-002 の L128（旧順序で UPPER / HEARD）は新順序では別の世界になる（ゴールデンではない。既定 32 行は ADR-0013 D6 のまま）。
- **アクチュエータ**（Reflex / Learner 共通、`reflex_act`）: `KIND_HARM` は `adjacent_violator()` があればそれを、なければ従来の `nearest_adjacent()` を対象にする。規則がなければ従来と同一。これは「殴るなら係争中の隣人を先に」という設計者の語彙だが（R1 のリスク、下記）、**殴るかどうか**（コスト 20 単位、直接の利得なし）は方策が決める。`KIND_TRANSFER` / `KIND_SIGNAL` は変えない（拒否による制裁と立法は Phase 4 では学習方策のアクチュエータに入れない。D7）。
- 性能: 新特徴は `someone_adjacent` と同じセル走査（`OCCUPIED` は自セルの `arrived` の最小、`VIOLATOR_ADJACENT` は最大 5 セルのフラグ）で、NormIndex が空なら即 0。decide の基準 ≤ 200 ns は据え置く（完了条件 2）。

### D7. 提案と合意: 動議・投票・憲章、改正、台本の立法者。学習方策は Phase 4 では立法しない

- **動議** `Message::Motion(MotionKind)`、`MotionKind ∈ { Enact(Norm), Repeal(rule_id), Charter(Charter) }`。`Action::Signal` のペイロード（行動の種類は増やさない = `KIND_COUNT` 8 のまま。Reflex の表と Learner の列を変えないため）。世界に**議場は 1 つ**（`nomos.motion`）: 開いている動議があれば `Failure::FloorTaken`、`Repeal` の id が無ければ `NoSuchRule`、憲章が `amendable: false` なら `Repeal / Charter` と施行済み `kind` への `Enact`（置換）は `NotAmendable`。コスト `motion_cost`（既定 2,000 = `harm_cost` と同水準。Random の動議の洪水を抑える）は試行時に払う。届いた動議は `Event::Motioned { tick, by, motion, kind }`（常時）。議場は世界全体で、`signal_radius` に関係なく届く（地域制度は Phase 5）。同じ tick に議場が開けば、以後の id の投票は同じ tick に数えられる（3b の行為者 id 順）。
- **投票** `Message::Vote { aye: bool }`: 開いている動議がなければ `NoMotion`、二重投票は `AlreadyVoted`。コスト `signal_cost`。`nomos.motion.ayes / nays` は id 昇順の `Vec<AgentId>`（有界: 生存数）。`Event::Voted` は `record_votes`（既定 OFF）のときだけ。`Failure::COUNT` は 11 → 16（`FloorTaken / NoMotion / AlreadyVoted / NotAmendable / NoSuchRule`）。
- **閉会**: 開いてから `vote_ticks` 後の tick の Phase 3c で集計。投票数 ≥ `quorum_permille` × 閉会時の生存数 / 1000 かつ 賛成 ≥ `threshold_permille` × 投票数 / 1000 なら可決し、`Enacted { tick, rule, norm, motion } / Repealed { tick, rule, motion } / Chartered { tick, charter, motion }` を記録して S(t+1) から有効。否決も `MotionClosed { tick, motion, ayes, nays, alive, carried }` を常時記録。死者の票は閉会時に除く。
- **既定の憲章**が定足数 10% なのは、Phase 4 で投票するのは Random / Enforcer / 台本だけで、学習方策は投票しないから。過半数の定足数では 1,300 体の世界で合意が成立しえない。H2 の格子で動かす。
- **改正（H3）**: `Repeal` / `Charter` / 置換の `Enact` の動議で可能。`amendable: false` の憲章（介入 `Charter` で設定）が改正手続きを止める = H3 の「改正手続きを無効化した同一 fork」。ただし H3 の実験には環境変化に応じて改正を提案する立法者が要り、Phase 4 の台本（下記）は適応的でない。**H3 の実験は Phase 5**。
- **`PolicyKind::Enforcer`（台本、表なし）= 陽性対照**。Forager に次を優先順で足す: (1) 距離 1 以内に HARM 制裁を許す係争中の個体がいて、そのエピソードでまだ自分が制裁しておらず（`recent` の `Sanctioned { other, tick ≥ since }` がない）、`energy > 2 × harm_cost` なら `Harm`、(2) `Promised` の相手が隣接し在庫があれば不足分（約束量 − その後の `GaveTo` の合計）を `Transfer`（履行）、(3) `enforcer_motions`（config、既定 true）で議場が空き、`Encroach` の規則がなく、16 tick 以内に `EncroachedBy` が 2 件以上あれば `Motion(Enact(Encroach { 32, HARM }))`、(4) 開いている動議に未投票なら `Vote`（`Enact` の `Encroach / BrokenPromise / Harm{except_sanction: true}` と `Charter` に賛成、`Repeal` と免責なしの `Harm` に反対）、(5) `occupied()` なら足元を採らず Forager の規則 4〜5 で他のセルへ、(6) それ以外は Forager と同一の決定。**設計者の知識そのもの**であり、EXP-004 では「制度を担う個体がいる条件」と指標の陽性対照として使う。Enforcer の挙動は何の証拠にもならない（R1 は行動語彙の制約で、方策の名前は役割の説明にすぎない）。
- **Random は 16 行動**（+ `Motion`（3 種の既定規範か、施行済みがあれば `Repeal`）、+ `Vote`（賛否一様））。Random 世界の軌跡は変わる（3-3 / 3-4 と同じ。EXP-001 / 002 の R 腕は再現しない。ゴールデンではない）。帰無モデルとして、Random の社会は偶然以上に規則を成立させないはず（閾値 2/3 に対し賛成は 1/2）。
- **学習方策は Phase 4 では立法しない**。`Motion / Vote` を Reflex / Learner のアクチュエータに入れると (1) 行動の種類が増えて表の形と Learner のゴールデンが変わり、(2) `signal_cost` 0 の投票は EXP-002 の Signal と同じく同点の雑音で選ばれ「合意」が偶然になる（P5f の教訓）。立法の学習は、投票に意味を持たせる報酬設計（制度の効果が自分の報酬に返る経路）とともに Phase 5 で扱う。

### D8. 介入と CLI: 布告による施行・廃止・憲章、fork で「規則あり / なし」を比べる

- `Intervention::Enact { norm }` / `Repeal { rule }` / `Charter { charter }`（`Copy`、乱数なし、`nomos` オフの世界には `Invalid`）。`Event::Intervened` として台帳に載り、再生で同じ tick に適用される（ADR-0012）。布告は憲章の `amendable` に縛られない（実験者の手）。布告による施行・廃止は `Intervened` だけに載り（`intervene()` は 1 イベントを返す）、エクスプローラが `Intervened { Enact / Repeal / Charter }` を時系列に混ぜる。
- 「固定 vs 学習」が fork + `SetPolicy` だったように（ADR-0013 D7）、**「規則あり vs なし」は fork + `Enact`（規則がなければ）または fork + `Repeal`（あれば）**。同一スナップショット + 同一介入で Verify = MATCH になる。EXP-004 の規則は布告で施行する（施行 tick を揃えるため）。
- CLI: `cz run / sweep / fork --nomos`、`--preset nomos`（EXP-003 で確定する config、D10）、`--enact KIND[:STANDING[:SANCTION[:EXCEPT]]] --enact-at TICK`、`--repeal ID --repeal-at TICK`、`--charter Q,TH,TICKS,AMENDABLE`、`--policy institution`（Learner 90% + Enforcer 10%、founding index の `% 10`。`nomos_preset` を暗黙に適用、cz-web の `policy_mix` も同じ）、`--learner-health-weight W`、`--record-violations`、`--record-votes`。`cz log` に新イベント。

### D9. 決定論・再現性・台帳の契約

- **整数のみ、純関数、乱数なし**: 判定・係争・集計・閉会・施行は `(World, その tick の解決結果)` の純関数で、乱数を引かない。`Stream` は増えない。並列 decide は NormIndex を読むだけ。
- **ハッシュの規約**（ADR-0013 3-3 以降と同じ「あるときだけ符号化」）: `Agent.arrived` は `Some` のときだけ、`World.nomos` は `config.nomos` が真のときだけ `dynamics_hash` にバイトを足す。新しい記憶の種類は従来の `kind as u64` の末尾に増える。config の新フィールド（`nomos / charter / motion_cost / enforcer_motions / record_violations / record_votes / learner_health_weight / perception.see_standing`）は `dynamics_hash` に入らない。台帳の新カウンタは `state_hash` にだけ入る（3-4 と同じ）。
- **ゴールデン**: Forager `0x658ee873d1866501` と Learner 32 行 `0xcb8bc0a9772c09a4` は不変（`nomos` オフ、`learner_health_weight` 0、下位 5 ビット不変、アクチュエータは規則なしで同一）。EXP-002 の F / X0 / X1 / X1-50k / L32 の `_series.csv` は 4-3 後の再実行で全行一致（回帰検査、4-3 の受け入れ基準）。Random だけが変わる（D7）。**制度世界のゴールデン**を新設: `World::generate(seed 42, nomos_preset, |i| if i % 10 == 0 { Enforcer } else { Learner })`（128 行）に tick 100 で `Enact(Encroach { 32, HARM })` を布告し tick 200 の `dynamics_hash` を native と wasm で固定（3-9c の Learner ゴールデンと同じ扱い）。
- **再生**: `multiverse::replay` は `Intervened` 以外の全イベントを照合しているので、制度のイベントが `TickReport.events` に固定順で載れば照合は自動で広がる。順序: 3b の移転・加害・交渉・発信 → `Sanctioned`（行為者 id 順）→ `Violated`（侵害はセル順・採集者 id 順、加害は行為者順、不履行は取引順）→ `Motioned` → `Voted` → `MotionClosed` → `Enacted / Repealed / Chartered` → `Acted` → 死亡 → 出生。`multiverse/tests/replay.rs` に制度世界（動議・布告・制裁を挟む）の bit 一致を足す。
- **スナップショット形式 9**（`Agent.arrived`、`World.nomos`、記憶の新種、Reflex の表 168、Learner のビット順、config の新フィールド）。形式 8 は `Format(8)` で拒否。`EventLog` の JSON Lines 形式は 1 のまま（変種の追加）。
- **記録の方針**: 常時 = `Motioned / MotionClosed / Enacted / Repealed / Chartered / Sanctioned`（いずれも件数が加害・拒否・議場で有界）。設定 = `Voted`（`record_votes`）、`Violated`（`record_violations`）。**`Violated` の既定は 4-1 のスモーク計測で決める**: forager / learner / institution の 3 世界（`nomos` オン、tick 100 で `Encroach` を布告、2,000 tick）で `Violated` が 3 世界平均 **≤ 10 件/tick**（学習世界の常時記録 ≈ 17 件/tick = 移転 2.5 + 加害 9.5 + 出生・死亡 ≈ 5 の 6 割以内。20,000 tick で +23 MB の JSON Lines）なら既定 ON、超えれば既定 OFF にしてエクスプローラは係争の窓（`standing_ticks`）と台帳のカウンタだけを見せる。侵害は同セル採集に限るが、Forager の集団が密集する既定世界では多いと予想する（リスク）。

**実装（4-1、2026-10-17）と設計からの差分**:

- ハッシュの規約はそのまま実装した: `Agent.arrived` は `Some` のときだけ（タグ 4 + tick）、`World.nomos` は `config.nomos` が真のときだけ（取引数 + 各取引）`dynamics_hash` に入り、台帳の 4 カウンタは `state_hash` にだけ入る。Forager `0x658ee873d1866501` と Learner 32 行 `0xcb8bc0a9772c09a4` は不変（native・wasm）。
- **スモーク計測（M1、2026-10-17、`cz sweep --nomos --seeds 3 --ticks 2000 --window 95`、窓 = tick 100〜2,000。規則はまだないので「`Encroach` の規則があれば `Violated` になる件数」= 侵害数、「`Harm` の規則」= 加害数）**: forager 侵害 **19.4 件/tick**（採集 159 件/tick の 12.2%）、learner 32 行 **23.2 件/tick**（161 件の 14.5%、加害 12.2 件/tick）、learner 128 行（制度世界の代用。Enforcer は 4-5）**21.2 件/tick**（110 件の 19.4%、加害 15.0 件/tick）。3 世界平均 **21.3 件/tick > 10** なので **`record_violations` の既定は OFF**（4-2 で導入時）。エクスプローラは係争の窓と台帳のカウンタを見せる（D12 の `violations_json` の第 2 経路）。`Harm` 規則の違反は 3 世界平均 9.1 件/tick（境界）、両方の規則が立てば約 30 件/tick。JSON Lines 1 件 ≈ 100 B とすると 20,000 tick で ≈ 40 MB/世界。約束は Learner / Forager の世界では発生しない（Propose を選ぶのは Random だけ）ので不履行は 0 件/tick。EXP-002 の窓（10 シード × 20,000 tick、`nomos` オン）では侵害率 forager 0.122（19.3 件/tick）、learner 32 行 0.102（21.1 件/tick）、立っている食料 6,163 / 2,758、再生 230 / 276 単位/tick — EXP-003 の E₀ の下見（予測「Forager の E₀ は Learner より高い」と整合）。

### D10. 前提条件の扱いと EXP-003（制度のない世界の格子、事前登録）

EXP-002 が示した前提条件は、制度実験のベースライン世界を**選ぶ**問題として扱う。`WorldConfig::default()` は変えない（ゴールデンと EXP-002 の再現性）。代わりに **`WorldConfig::nomos_preset()`**（`--preset nomos`、UI の `institution` ミックス）を置き、その値を EXP-003 の事前登録した決定規則で決める。

**EXP-003 候補の採否**:

| 候補（EXP-002「次の問い」、TODO） | 採否 | 理由 |
|---|---|---|
| 死の終端報酬 | **却下** | Q 表は個体の `params.table` にあり、死で個体ごと消える。子は出生時点の表を既に複写している。終端更新を受けた表は誰にも読まれない（個体学習では構造的に無効。集団共有の Q は ADR-0013 で却下済み）。行 9 の「飢えながら休む」には、死の前に効く信号 = 体力項（`learner_health_weight`、D3）を腕にする |
| ε と `harm_cost` の格子 | **採用（主格子）** | 加害死 43% が探索由来なら ε で下がるはず。制裁の指標はこの水準からの差でしか読めないので、ベースラインの暴力水準を先に決める |
| 体力項（新規、D3） | **採用（主格子）** | 前提条件 2: 加害は報酬に現れない。制裁が学習に効くには体力が報酬に要る |
| `--mutate` だけの Forager | **先送り（Phase 6）** | `hunger_permille` の低下の機構は進化の問いで、制度の前提ではない。`cz sweep --policy forager --mutate` は既に動くので Phase 6 の進化実験で腕にする |
| 立っている食料・総再生量の指標 | **採用** | `cz sweep` に `win_food_standing_mean / win_regrown_per_tick` を足す（事後計算をやめる）。制度世界の個体数差を読むのに要る。**4-1 で実装**（series にも `regrown`） |
| Q 表の行ごとの訪問回数 | **採用（計測のみ）** | `TickReport.learner_rows`（学習者がいる tick だけ、決定した行のヒストグラム）を `cz sweep` が積算し `_qpolicy.csv` に `visits` 列。状態には入れない（ゴールデン不変）。**4-1 で実装**（`visits` = 全区間、`visits_window` = 窓） |

**EXP-003 の形**（仕様は `docs/research/experiments/EXP-003-baseline-grid.md` に実行前にコミット）:
- 腕: Learner 32 行、`nomos` オン（追跡のみ、規則なし）、ε ∈ {0, 10, 50}‰ × `harm_cost` ∈ {2,000, 8,000} × `learner_health_weight` ∈ {0, 2} の 12 セル。副次腕: Forager（`nomos` オン。固定方策の侵害率 E₀）、Learner 128 行・新ビット順（既定セルのみ。`OCCUPIED` で行が割れても L32 並みに学ぶかを見る）。14 腕 × seed 0〜9 × 20,000 tick ≈ 18 分（M1、EXP-002 の実測から）。
- 指標: EXP-002 の M1〜M9 に、E₀ = `encroachments / gathers`（方策別）、立っている食料、再生量、行の訪問、行 9 の貪欲 Rest のシード数を足す。
- **preset の決定規則（事前登録）**: 絶滅 0/10、窓平均 ≥ F の 70%（918）、窓の加害死 ≤ 死因の 10% を満たすセルのうち **ε が最大**のもの。同点なら、行 9 で Rest が多数派のシードが ≤ 5/10 になる体力項を選び、それでも同点なら `harm_cost` 2,000（既定）。どのセルも満たさなければ ε 0・`harm_cost` 8,000 を採り、満たせなかったことを記録する。EXP-004 の仕様コミット前に preset × 128 行を 3 シードのスモーク（絶滅なし）で確認する。
- 予測の形（棄却可能、数値は EXP 文書で固定）: 加害死の割合は ε に単調（50‰ で 43% → 10‰ で < 10% → 0 で ≈ 0）、ε 0 でも帯域は F の 70% 以上（3-7 の単一シード 1,485 体から）、体力項 2 で行 9 の Rest 多数派が ≥ 7/10 シードで消える、Forager の E₀ は Learner より高い（Forager は最も豊かなセルへ集まる）、128 行・新順序は tick 1,000 までに 32 行の 80% 以上に回復する。
- EXP-003 の E₀ が「規則なし」の侵害率のベースラインで、EXP-004 はこの preset から始める。

### D11. H1 の事前登録の形（EXP-004: 先占規範と制裁）

仕様は `docs/research/experiments/EXP-004-first-occupant-norm.md` に実行前にコミットする。ここでは形だけを決める。

- **世界**: `nomos_preset`（EXP-003）、`nomos` オン、`enforcer_motions: false`（Enforcer が自分で立法して施行 tick をずらさないため。施行はすべて布告）、128²、1,000 体、20,000 tick、Learner は **128 行**（新順序: `OCCUPIED` と `VIOLATOR_ADJACENT` が見える）。規則 R = `Encroach { standing_ticks: 32, sanction: HARM }` を **tick 0 に布告**（腕間の対比を同じ窓で取るため。白紙の表の谷（EXP-002: L32 は tick 600、L128 は 1,600〜2,900 で回復）の後の窓を分析に使う）。分析窓: tick 2,000〜20,000 を 100 tick の窓に切り、「序盤」= 2,000〜4,000、「後半」= 10,000〜20,000、「末尾」= 15,000〜20,000。
- **腕**（各 10 シード、主対比 LE / LE0 / L0 は 30 シード）:

| 腕 | 構成 | 役割 |
|---|---|---|
| L0 | Learner、規則なし | ベースライン（E の時間変化は学習だけでも起きうる） |
| L1 | Learner + R | 学習者だけで制裁が出るか（予測: 出ない。報酬に利得がない） |
| LE0 | Learner 90% + Enforcer 10%、規則なし | Enforcer の存在そのものの効果（規則なしでは制裁しないが侵害もしない） |
| **LE** | LE0 + R | **主腕**: 制度を担う個体がいる社会で学習者の侵害率が下がるか |
| N-rand | Random + R | 帰無（方策） |
| N-blind | LE + `see_standing: false` | 帰無（規範情報の経路 (a)）: 学習者は係争を見ない。遵守が「痛み」だけで学ばれるなら LE と同じになる |
| N-nosanction | LE + R' = `Encroach { 32, 制裁なし }` | 帰無（制裁不能）: 係争はするが誰も制裁できない。「名指しされる」だけで行動が変わるか（予測: 変わらない） |
| N-nocomm | LE + `signal_radius: 0` | 帰無（通信なし）: v1 の制裁は通信を使わない（係争は NormIndex で公知）ので差がないはず。**差がないことは設計の限界の記録であって証拠ではない** |
| N-nomem | LE + `memory_slots: 0` | 帰無（記憶なし）: Enforcer の「同じエピソードに 1 回」の歯止めが消え制裁が増える。約束は成立しない |

- **指標（事前登録）**: 学習者の侵害率 E_L(w) = 窓 w の Learner の侵害 / Learner の成功採集、制裁の条件付き確率 S(w) = 閉じたエピソードのうち制裁されたもの、S'(w) = 制裁 / 違反。主指標は「後半」の E_L の平均で、腕どうしは**同じ seed の対**で比べる。時間的低下 = 「序盤」から「後半」への E_L の差、持続 = 「末尾」の E_L が「序盤」の 0.8 倍以下。個体数・死因・摂取も記録する（制度のコスト）。
- **予測の形**（棄却可能。数値は EXP 文書で固定）: (1) 後半の E_L が LE < LE0 で ≥ 8/10 シード、かつ 30 シードの対の差の 95% ブートストラップ CI が 0 を含まない。(2) L1 で S' が探索の床の 2 倍以下、L1 と L0 の E_L の差の CI は 0 を含む。(3) N-nosanction と LE0 の差の CI は 0 を含む。(4) N-blind は LE と同程度に下がる（遵守は `OCCUPIED` と痛みで足りる。下がらなければ規範特徴が効いた証拠で、どちらも記録する）。(5) N-rand は E 一定、S' ≈ 床。(6) N-nocomm は LE と差なし。(7) N-nomem は S が LE より高く、E_L は LE 以下。
- **主張の書き方**: 「条件 LE で学習者の侵害率が、同じ seed の LE0 に比べ X 下がった（CI）。N-nosanction では下がらなかった」までで、H1 の判定はこの文の成否。(1) が外れれば「この設定では先占規範の制裁は学習者の侵害率を下げなかった」と記録し、H1 は未決のまま。「創発」「世界初」「所有権が生まれた」は使わない。Enforcer は設計者の知識なので、LE の結果は「制度を担う個体がいれば学習者が従う条件がある」以上を意味しない。
- 統計: 30 シードの対の差の平均とブートストラップ CI、効果量。p 値だけの報告はしない（HYPOTHESES の規則）。

### D12. UI（制度エクスプローラ）は読み手だけで作る

- cz-web（カーネル不変）: `institution_json()`（憲章・施行中の規則・開いている動議の集計・係争中の件数）、`timeline_json(from, to)`（`Motioned / MotionClosed / Enacted / Repealed / Chartered` と `Intervened { Enact / Repeal / Charter }` を台帳順に）、`violations_json(from, to)`（`record_violations` なら台帳から、なければ係争の窓から「今係争中」を）、`sanctions_json(from, to)`（`Sanctioned` に、それが答えた係争の規則・開始 tick・違反数を付けて）、`agent_json` に `arrived` と係争、`agents()` に 9 列目 `flags`（bit0 係争中、bit1 投票済み。`AGENT_FIELDS` 9）、`Frame.institution`（規則数・動議の集計・係争数。率は `Frame.ledger` の新カウンタの差分で UI が出す）、`watch` に `standing / voted`。3-9b と同じく判定はすべて Rust 側で、UI は語彙を並べるだけ。プロトコル **v6**。
- web: 「Institution · ledger」カード（時系列）、「Violations · recent」と「Sanctions」（制裁は「answering the tick t violation」）、「Norms · rates」カード（E（方策別）・違反/tick・S の直近 100 tick）、介入パネルに Enact（`NormKind` と 4 パラメータのメニュー）/ Repeal / Charter、Branches に **「Compare: with vs without rule」**、インスペクタに「Accused under rule #k since tick t (violations n, sanctions m)」、地図に制裁のマーカー（✕ の別色）と係争中の輪郭、ヘッダの world に `institution`、ツアーに「Enact a rule」の段、TUTORIAL §10。モック worker は空の制度を返す。
- 文はテンプレート（`web/src/nomos.ts`）で、D5 の 4 パラメータから決定的に生成する。

## タスク分解

| # | タスク | 依存 | 受け入れ基準 |
|---|---|---|---|
| 4-1 ✅ | 社会的事実の追跡（D3）: `WorldConfig.nomos`、`Agent.arrived`（移動・継承・介入・出生）、侵害の判定と `EncroachedBy`、約束の監査（`nomos.deals`）、台帳 `gathers / encroachments / promises_due / promises_broken`、`TickReport` の方策別の採集・侵害と `learner_rows`、`learner_health_weight`、`cz sweep` の新指標（侵害率・立っている食料・再生量・訪問回数）、記録量のスモーク計測。**済 2026-10-17**: `src/nomos.rs`、`tests/nomos_facts.rs` 9 本 + proptest 12 ケース、`crates/cz-web/tests/nomos_audit.rs` 2 本、スナップショット形式 9、CLI `--nomos` / `--learner-health-weight`。設計からの差分は D3 / D9 の追記（監査の窓は受諾 tick から、`gathers` は常時、記憶の競合、形式 9 は 4-1 で使用） | – | `nomos` オフで Forager / Learner のゴールデン不変。`tests/nomos_facts.rs`: 手組みで先占（同 tick 到着は侵害でない、子は親と共同、死亡と移動で失効、創設個体は最古）、監査が `thread.rs` の kept / broken と一致、体力項の報酬値（加害 −20 × w、Rest +1 × w）、proptest で serial = parallel。`nomos` オンでも EXP-002 の forager / learner32 の series が全行一致（追跡は決定を変えない）。スモーク計測（3 世界 × 2,000 tick）で侵害/tick と `Violated` 相当の件数を記録し D9 の既定を決める。step の増分 ≤ 10%（`nomos` オン、規則なし）。**達成**: ゴールデン 2 本不変（native・wasm）、EXP-002 の forager / learner32 の `_series.csv` 16 列と `_summary.csv` 41 列がオフ・オンとも `cmp` で完全一致、監査は台本 8/9 件一致（残る 1 件は D3 の追記どおりの定義差）+ Random 世界 151/156 件一致（5 件は定義差で除外）、スモーク計測は D9 の追記（平均 21.3 件/tick → `record_violations` 既定 OFF）、step の増分は STATUS の計測値 |
| 4-2 | `nomos` モジュール（D2, D4, D5, D8）: `Norm / Rule / Charter / Standing / Nomos`、`NormIndex`、3 述語の判定・係争・制裁の識別・エピソードのカウンタ、イベント `Violated / Sanctioned`、記憶 `Sanctioned / SanctionedBy / SawSanction`、介入 `Enact / Repeal / Charter` と CLI、スナップショット形式 10（9 は 4-1 が使った）、`check_invariants`、再生照合 | 4-1 | `tests/nomos_rules.rs`: 3 述語が台本どおり判定され物理は不変（同じ intents を `nomos` オフ世界に与えて身体・資源・台帳の物理列が一致）、`except_sanction` の免責、制裁の帰属（最古のエピソード）、係争の期限と死亡での閉鎖、S と S' のカウンタ、置換 = `Repealed` + `Enacted`、同一 tick の `Enact` 2 回 fork でハッシュ一致、`multiverse/tests/replay.rs` で bit 一致、形式 9 の拒否、不整合の拒否。NormIndex 構築 ≤ 30 µs/tick（1.3k 体、係争 2,000 件を手組み、計測）。proptest で serial = parallel |
| 4-3 | 観測と学習方策（D6）: `Observation.norms` と問い合わせ、Reflex 21 特徴（零の初期重み、突然変異の順序、長さ 168）、Learner の新ビット順（`FINE_STATES` 1024）、Harm のアクチュエータ、`Perception.see_standing` | 4-2 | Forager / Learner 32 行のゴールデン不変。EXP-002 の X1 / X1-50k / L32 の series が全行一致。`nomos` オフで 64 / 128 / 256 行の Learner が 32 行と同一の決定（手組みテストと 1,000 tick の身体一致）。係争中の隣接個体が Harm の対象になる、`see_standing: false` で特徴が 0 と対象が従来どおり。decide ≤ 200 ns/体·tick（Reflex 進化後、Learner 128 行、規則あり、係争あり。計測） |
| 4-4 | EXP-003（D10）: 仕様コミット → 実行 → 記録、`WorldConfig::nomos_preset()` と `--preset nomos` の確定 | 4-1 | 仕様コミットが実行より前。14 腕 × 10 シード。決定規則どおりに preset を確定し、値と根拠を STATUS と EXP 文書に。終端報酬の却下理由を EXP 文書にも記す。RISKS R8 / R9 に計測値を追記 |
| 4-5 | 提案と合意（D7）: `Message::Motion / Vote`、議場・憲章・閉会・施行・置換、イベント `Motioned / Voted / MotionClosed / Enacted / Repealed / Chartered`、`Failure` 5 種、`motion_cost / enforcer_motions / record_votes`、Random の 16 行動、`PolicyKind::Enforcer`、`--policy institution` | 4-2 | `tests/nomos_motions.rs`: 台本で動議 → 投票 → 閉会 → 施行が台帳と `nomos` に並ぶ、定足数・閾値・期限の境界、議場 1 つ、死者の票の除外、改正と `amendable: false`、Enforcer の 6 規則（制裁は同一エピソードに 1 回、履行、動議、投票、不侵害、それ以外は Forager と同一決定）、Random 世界の往復、proptest で serial = parallel。`cz sweep --policy random --nomos` 10 シードで可決 ≤ 1 件（帰無が偶然以上に立法しない） |
| 4-6 | EXP-004（D11）: 仕様コミット → 実行 → 記録、`cz sweep` の方策別侵害率と S / S' | 4-3, 4-4, 4-5 | 仕様コミットが実行より前。9 腕 × 10 シード + 主対比 30 シード。帰無モデル 5 本。予測 (1)〜(7) の当否、限界、H1 の判定文を明記。禁止語を使わない |
| 4-7 ∥ | cz-web の読み手（D12）: `institution_json / timeline_json / violations_json / sanctions_json`、`agent_json` と `agents()` の拡張、`Frame.institution`、`watch` の係争、プロトコル v6、モック worker | 4-2（型が決まれば先行可） | `tests/native.rs`: 手組みの台帳と `nomos` から組んだ期待値と一致、seek 後も同じ（ビューの tick で読む）、fork 後も続く。`tests/wasm.rs` で wasm = native。`thread_json` は不変 |
| 4-8 ∥ | 制度エクスプローラ（D12）: 時系列カード、違反と制裁の一覧、率のカーブ、介入パネル（Enact / Repeal / Charter）、「Compare: with vs without rule」、インスペクタの係争、地図のマーカー、`institution` ミックス、ツアーの段、TUTORIAL §10 | 4-7、動議の表示は 4-5 | PRODUCT.md Phase 4 のデモ (a)〜(e) を公開 URL で確認。Verify = MATCH。コンソールエラーなし |
| 4-9 | 計測とゴールデン（D9、完了条件 2）: 制度世界のゴールデン（native = wasm、CI）、decide / step / NormIndex / スナップショット / 巻き戻し（R11、学習世界と制度世界）/ ブラウザ tick/s（前面、フロンティア差分で裏取り）/ wasm サイズ | 4-3, 4-5, 4-8 | 完了条件 2 のすべての数値を STATUS に機種・コミット・日付付きで。基準を超えた項目は ADR-0013 D6 の「決定」と同じ形で改定理由を書く |
| 4-10 | 文書: この ADR の完了判定、STATUS の Phase 4 表と「状態の区別」、RISKS R12〜R14（係争の公知、先占の操作化の人工物、陽性対照 = 設計者知識）、HYPOTHESES（H1 の状態、H2 / H3 の Phase 5 への引き渡し）、ARCHITECTURE（`nomos` モジュールとデータモデル、Apply の 3c、プロトコル v6）、CLAUDE.md の `Failure` の文言（D4）、TUTORIAL §10〜11、README | 全て | 新セッションが STATUS → ADR-0014 → EXP-003 / 004 で Phase 4 の状態を再構築できる |

並行可能: **4-7 と 4-8（UI）は 4-2 の型が決まり次第モック worker で先行できる**（3-9a と同じ）。4-4（EXP-003）は 4-1 だけに依存し、4-2 / 4-3 と並行できる。4-5 は 4-3 と並行できる（どちらも 4-2 の上）。4-6 は 4-3 / 4-4 / 4-5 が揃ってから。共有チェックアウトで並行する場合は、`Event` / `Failure` / `InteractionKind` / `Message` / `PolicyKind` の変種追加を先に小さくコミットして、兄弟セッションの `match` が壊れる時間を短くする（Phase 3 の教訓）。

## 却下した / 先送りした案

- **禁止行動をカーネルで実行不能にする（規則違反を `Failure` にする）**: R4 に反し、制裁の実験ができなくなる。不採用。
- **罰金・国庫・制度による移転**: 制度が物理（保存則の経路）を持つことになる。不採用。制裁は個体の行動だけ。
- **`nomos` を別クレートにする**: Decide が規範情報を読み、Apply が判定するので遷移関数の一部（ADR-0013 D1 と同じ）。読み手（cz-web / cz-cli）だけが上に載る。
- **死の終端報酬**: 表は個体と一緒に消える（D10）。体力項で代替。
- **立法（Motion / Vote）を学習方策のアクチュエータにする**: 行動の種類が増え表と Learner のゴールデンが変わる。無料の投票は同点の雑音になる（EXP-002 P5f）。Phase 5 で、投票が報酬に返る経路と一緒に。
- **違反を常に個別記録する**: 侵害の件数が未知で、`Acted` の二の舞になりうる。4-1 の計測で既定を決める（D9）。
- **メタ規範（制裁しないことを違反にする）**: 第三者執行の学習は Phase 5 以降。表現できないことに明記（D5）。
- **地域制度・複数の制度・個体ごとの知覚制限付き係争**: v1 は世界全体で 1 つ、係争は公知（NormIndex）。これは強い仮定で、R12 として記録する。半径や目撃に基づく係争（`EncroachedBy / SawSanction` の記憶だけから知る）は Phase 5 の H2 の格子に送る。
- **先占の根拠を `Store`（投資）や直近の採集にする**: 到着順が最も単純で知覚と整合（同セル）。変種は Phase 5。
- **条件付き規則（「食料が少ないときは」）**: DSL に式を入れると R1 の表現力の問題が戻る。環境に応じる制度は改正で表す（H3）。
- **制裁を自動化する（係争中の個体に対する加害を無料にする、違反者の採集を失敗させる）**: どちらも物理の変更（R4）。制裁のコストは個体が払う。
- **LLM による規則の文章化・解釈**: 文はテンプレート。LLM は Phase 4 に登場しない。
- **`--mutate` だけの Forager（EXP-003 候補）**: Phase 6 の進化実験へ。

## リスク

- **侵害の操作化の人工物**: 同セルの到着順は、密集する Forager の集団ではほぼ全員が侵害者になりうる（E₀ が高い）。H1 は「差」で読むので致命的ではないが、係争が `alive × rules` まで増えて NormIndex の構築と `Violated` の記録量を押し上げる。4-1 の計測で件数を見てから `standing_ticks` の既定と記録方針を決める（D9）。家族を共同の先占者にした選択も人工物を生む（親子が同じセルで採り続ける）。
- **係争の公知**: v1 では係争中かどうかを誰もが観測できる（通信も目撃も要らない）。これは制度の「情報の問題」を消しており、N-nocomm の帰無モデルが空振りになる理由でもある。結果の解釈に必ず付記する（R12）。
- **陽性対照は設計者の知識**: Enforcer がいる条件で侵害が減っても、それは「従わせる個体を置けば従う」以上ではない。学習者だけの条件（L1）で何も起きないことを予測しており、それが Phase 4 の正直な見込みである。制裁の学習には報酬の経路（互恵・評判）が要り、それは Phase 5 の設計問題になる。
- **報酬の体力項は Learner の目的を変える**: `learner_health_weight > 0` の世界は EXP-002 と直接比べられない。EXP-003 の格子で重み 0 のセルを必ず含め、preset の値を選んだ理由を残す。
- **Harm のアクチュエータが係争中の個体を優先する**: 「誰を殴るか」を設計者が決めている（経路 (a) の語彙）。殴るかどうかは方策が決めるが、制裁の指標 S' の床はこの語彙の帰結を含む。N-blind（係争を見ない）と N-nosanction が切り分ける。
- **性能**: 特徴が 21 個、Learner が 128 行、NormIndex と判定が step に加わる。decide 200 ns と step の +20% を門にし（完了条件 2）、超えたら ADR-0013 D6 と同じ手順で改定か最適化かを決める。
- **スナップショット**: 128 行の学習者で 1 MB を超える（3-7 の計測 ≈ 1.1 KB/体）。制度世界の門を 2 MB にし、fork の転送と巻き戻しを計測する。
- **Random の動議の洪水**: `motion_cost` と議場 1 つで抑えるが、Random 世界の絶滅は早まる（EXP-001 / 002 の R 腕は再現しない）。
- **UI の肥大**: wasm 345 KB に serde_json の直列化が 4 本増える。≤ 400 KB を門にし、超えれば TODO の「serde_json を境界から外す」を先に行う。
- **共有チェックアウトでの並行開発**: 変種の追加が兄弟セッションのビルドを壊す。小さく先にコミットする（タスク表の注記）。

## 帰結

- 遷移関数が拡張される（判定・係争・閉会・施行は Apply の一部、Phase 3c）。`nomos` オフの世界は不変で、ゴールデンは Forager / Learner 32 行とも据え置き、制度世界に第 3 のゴールデンを置く。スナップショット形式 9（4-1）→ 10（4-2）、プロトコル v6、`Failure::COUNT` 16、`InteractionKind` 14 種（4-1 で 11）、`PolicyKind` 5 種。
- ARCHITECTURE の `nomos` クレートの行は `simulation-core/src/nomos/` モジュールに置き換わり、データモデルに `Agent.arrived` / `World.nomos`、Apply に Phase 3c（判定・閉会）が加わる。CLAUDE.md の `Failure` の不変条件は「物理的・手続き的不能のみ」になる。
- Phase 5（Research Platform）は、本 ADR の H2 / H3 の材料（憲章と規範のパラメータ、`amendable`、`see_standing`、`memory_slots`、`signal_radius`）を格子にし、地域制度・知覚制限付きの係争・立法の学習を候補に持つ。EXP-004 の結果が「学習者は従うが制裁しない」なら、制裁の報酬経路（互恵・評判）が Phase 5 の最初の設計問題になる。
- EXP-002 の前提条件は EXP-003 で「選んだベースライン」になる。`WorldConfig::default()` は変えないので、EXP-001 / 002 はそのまま再現できる。
