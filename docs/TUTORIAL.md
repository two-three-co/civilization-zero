# Toy 1 — 5 分チュートリアル

Civilization Zero の最初の遊べるマイルストーン。ブラウザだけで、文明を観察し、巻き戻し、介入し、分岐した歴史を並べて比較する。
画面に出る数字はすべてこのタブの中でカーネル（Rust → wasm）が計算したもので、サーバーには何も送らない。

## 開き方

- 配信 URL: **https://two-three-co.github.io/civilization-zero/**（`main` の最新ビルド。フッタのコミットで版が分かる）。
- ローカル: リポジトリで `cd web && npm install && npm run build:wasm && npm run dev` → `http://127.0.0.1:5173/`。
- 配布された `dist/` フォルダ: 任意の静的サーバーで配信する（例: `python3 -m http.server 8766` を `dist` の親で実行し `/dist/` を開く）。`file://` では Worker が動かないブラウザがある。

画面右上のバッジが **kernel · wasm** なら本物。`mock worker` と出ていたら `?mock=1` 付きで開いている（見た目確認用の偽物）。
右列の「Five-minute tour」カードが、下の 5 ステップ（と Phase 4 の 6 段目「Enact a rule」、10 節）を自動で進捗判定する。

## 1. 観察する（Play）

**Play** を押す。オレンジが採集者（forager）、青がランダム歩行者、黄が進化する反射（reflex）、ピンクが学習者（learner）、緑が食料。個体の明るさはエネルギー。
最初の 100〜300 tick で、食料から遠い個体が餓死し、豊かな場所の個体がエネルギーを分けて子を産み、
人口は約 1,300 の帯域に落ち着く（docs/research/experiments/EXP-001）。

見る場所: 右列「World A」の **alive** と **food on map**、スパークライン（人口 = オレンジ、食料 = 緑）、
ヘッダの実測値（tick/s・fps・worker ms/tick）。`30 tick/frame` にすると約 1,800 tick/s。

## 2. 巻き戻す（タイムライン / Go）

タイムラインを左へドラッグするか、`tick` 欄に数字を入れて **Go**。
世界は最寄りのスナップショット（250 tick ごと）から再実行されて復元され、その過程で出生・死亡が台帳と照合される。
「last seek」に復元時間と再実行 tick 数が出る（最悪で 50 ms 程度）。

**state hash** はその tick の状態の指紋。同じ tick に戻れば同じ値になる。
同じ世界をコマンドライン（`cz run --ticks N`）で動かしても同じ値になる — それが決定論の意味。

## 3. 介入する（Apply）

frontier（到達した最大 tick）にいるとき、「Intervene」カードで介入を選び **Apply**。

- Drought: 再生量と間隔を書き換える（`0` にすると食料が戻らない。劇的）
- Scale food: 全セルの食料を permille/1000 倍（500 で半分、2000 で倍）
- Spawn agents: 1 セルに n 体追加
- Remove agent: id 指定で 1 体除去（地図クリックで id を拾える）

介入は台帳に記録され、「Ledger · interventions」に **カーネルから読み戻した**履歴が出る（ページの記憶ではない）。
巻き戻し中（frontier より前）は Apply が無効になる。過去を変えたいなら次の Fork。

## 4. 分岐して比べる（Fork here）

任意の tick に戻り、介入を選んで **Fork here**。
その tick の開始状態から 2 つ目の世界 B が作られ、介入が適用される。以後、Play も Go も A・B 両方に効く（B は分岐点より前には戻らない）。

「Branches」カードに A / B の tick・人口・食料・ハッシュと差分、スパークラインに B が破線で重なる。
干ばつを入れた B の人口が落ちていく様子と、何もしない A を並べて見られる。

## 5. 本物か確かめる（Verify: fork again）

**Verify: fork again** を押す。保存しておいた同じスナップショットから、同じ介入で 3 つ目の世界を作り、同じ tick まで進めて B とハッシュを比べる。
**MATCH** なら「同じ入力 → 同じ歴史」。これはこのプロジェクトの核になる契約（ADR-0012）で、主張ではなく画面内で検証できる。

## 6. 社会マーカーと記憶を見る（Phase 3）

採集者だけの世界（既定）では誰も他者に働きかけない。ヘッダの **world** で **mixed**（偶数 id が採集者、奇数 id がランダム歩行者。`cz run --policy mixed` と同じ割当なので state hash も一致する）か **random** を選ぶと世界が作り直され、ランダム歩行者が隣の個体に食料を渡し、殴り、記号を発し、取引を持ちかける。
採集者だけの世界でも「Spawn agents」で policy を Random にして追加すれば同じものが見える。

地図のマーカーは**直前に実行された tick** の相互作用で、台帳から読む（ページが覚えているのではない）:

- 水色の線と点 = 食料の移転（点のある側が受け取った個体）
- 赤い線と ✕ = 加害（✕ が被害者）
- 紫の輪 = 記号の発信（輪の中心が発信者。中身は `say 0〜7`）

マーカーは 0.6 秒で消える。`1 tick/frame` にすると毎 tick 全部見える。`30 tick/frame` では 30 tick に 1 tick 分しか描かれないが、右列「Ledger · interactions」の累計と「per tick (1 s)」（直近 1 秒の tick あたりの移転・加害・発信数）は落ちない。タイムラインで戻れば、その tick の直前の相互作用が出る。

個体をクリックすると「Agent」カードに 2 つの記録が並ぶ:

- **Memory · recent**: その個体自身の記憶。カーネルが書く最大 8 件（古いものから消える）で、`tick 41 · #267 proposed: give 1, want 0 …`、`heard #12 say 3`、`harmed by #5 (−20 health)`、`saw #5 harm #9` のように tick 付き。方策はこれを読むだけで、書くのはカーネル。
- **Ledger · ticks a–b**: 台帳のうちその個体を名指しする直近 100 tick の出来事。記憶と違って上限がなく、個体が死んでも残る（`died of violence at age …`）。

ランダム歩行者は加害のコストが高いので tick 300 前後までに絶滅する。`Go` で 100 あたりに戻って青い個体（または紫の輪の中心）を選ぶと、記憶が埋まっているのが見える。

## 7. 交渉のスレッドを読む（Phase 3、3-9b）

ランダム歩行者は取引も持ちかける: 「今 give 渡すから、期限までに want 返せ」（`Propose`）に相手が `Accept` / `Reject` で答え、受諾されれば両者の記憶に「約束」が書かれる。**約束の履行は強制されない**。渡すも返すも普通の `Transfer` で、守られたかどうかは台帳を読んで初めて分かる（ADR-0013 D5）。それを読むのがスレッドである。

1. **https://two-three-co.github.io/civilization-zero/?policy=mixed&seed=23** を開く（`?seed=` は世界のシード。既定の 42 には提案 → 受諾 → 移転 → 返済の 4 手順がそろった取引が歩行者の絶滅までに 1 つもないので、ここでは 23 を使う。`cz run --seed 23 --policy mixed` と同じ世界で、state hash も一致する）。
2. `tick` 欄に 40 と入れて **Go**。右列の「Deals · ledger」で **Find deals** を押す。直近 2,000 tick の受諾された提案がすべて出て、それぞれ **KEPT**（両側が期限内に渡した）/ **OPEN**（期限前で未完）/ **BROKEN**（期限を過ぎて不足）と判定されている。判定はページではなくカーネル側（wasm の Rust）が台帳から行う。
3. いちばん上の `#909 → #363 · 2 for 1 by tick 33 · KEPT` の **read** を押す。「Agent」カードの下の **Thread · with #363** に、2 個体の間で起きたことが「tick · 誰が → 誰に · 何を」で古い順に並ぶ:

   ```
   tick 17  #909 → #363  proposed: give 2 now, want 1 back by tick 33 · accepted · deal 4
   tick 18  #909 → #363  gave 1 food · repayment · deal 3
   tick 19  #363 → #909  accepted the offer of tick 17: #909 gives 2 now, #363 returns 1 by tick 33 · deal 4
   tick 19  #909 → #363  gave 1 food · the give · deal 4
   tick 20  #363 → #909  gave 1 food · repayment · deal 4
   ```

   その下の **deal 4** が判定: `#909 gives 2 (given 2), #363 returns 1 (repaid 1) · kept at tick 20`。同じ 2 個体の tick 1〜14 の取引 1〜3 は BROKEN で、誰が何を渡さなかったか（`broken by #363: deadline tick 13 passed`）まで読める。tick 22 の `rejected the offer of tick 17` は、受諾した相手が同じ提案を後から拒否したもの（カーネルは何も検査しない）。
4. 相手を替えるには、Thread の見出しの下に並ぶ `#id` ボタン（選択個体の記憶と台帳に出てくる相手。交渉相手が先）か、id を入れて **read**。地図で個体をクリックしても同じ。
5. タイムラインで tick 19 まで戻すと deal 4 は **OPEN**（期限 33 まで待っている）に、tick 14 まで戻すとまだ存在しない。スレッドは「その tick のビューで読んだ台帳」で、ページは何も覚えていない。再生すると取引が増えていき、歩行者が死んだ後もスレッドは残る（台帳は個体より長生きする）。

読み方の規則（ADR-0013 D5 の 3-9b での確定）: 取引の窓は提案の tick から期限（提案 tick + by）まで。窓の中で提案者が渡した合計が give 以上、受諾者が返した合計が want 以上になれば kept。期限を過ぎてどちらかが不足なら broken。移転と約束の対応はカーネルが記録しないので、窓が重なる取引は同じ移転を両方で数える。

## 8. 固定方策と学習方策を比べる（Phase 3、3-9c）

学習方策（ADR-0013 D6）は 2 種類ある。**Learner** は表形式 Q 学習（整数、生涯内で学ぶ、子は表を継承）、**Reflex** は 16 特徴の線形反射で世代間の突然変異だけで変わる。どちらも固定方策と同じ `Observation → Intent` の純関数で、学習（Q の更新）はカーネルが Apply で行う。「固定 vs 学習」は fork と `SetPolicy` 介入（D7）の組み合わせで、Toy 1 の分岐比較がそのまま実験になる。

1. **https://two-three-co.github.io/civilization-zero/** を開き（既定の採集者だけの世界）、`tick` 欄に 300 と入れて **Go**。
2. 右列「Branches」の **Compare: fixed vs learning** を押す（隣の選択は B の方策。既定は Learner）。tick 300 の開始状態から世界 B が作られ、B の全員が学習者に切り替わる。身体・位置・記憶はそのままで、方策と白紙の Q 表だけが変わる。Branches の表の **policies** が A「1442 forager」/ B「1442 learner」になり、state hash が分かれる。
3. `15 tick/frame` にして **Play**。地図 B の個体がピンク（learner）になり、移転の線・加害の ✕・発信の輪が B にだけ出る。右列「Compare · A solid, B dashed」の 5 本のカーブで比べる: **alive**（B は tick 300〜450 で 1,442 → 約 500 体まで落ちてから回復し、tick 1,000 以降は A より多い）、**intake / agent / tick**（1 体 1 tick あたりの摂取量。A ≈ 0.17、B は落ち込みの後 0.16 前後）、**transfers / harms / signals / tick**（A は 0、B は学習者が社会行動を試すので非零）。率は直近 50 tick の台帳差分なので、速度を変えても読みは同じ。Branches の表には食料消費の累計と、各世界のワーカーの ms/tick も並ぶ（学習世界は採集者の世界の 2〜2.5 倍かかる）。
4. **Verify: fork again** を押す。同じスナップショットに同じ `set_policy` を適用して同じ tick まで進めた 3 つ目の世界が B とハッシュ一致 = **MATCH**。学習の Q 更新が整数演算だから、学習世界でも決定論の契約（ADR-0012）はそのまま成り立つ。コマンドラインでも同じ歴史になる: `cz run --seed 42 --ticks 600 --set-policy learner --set-policy-at 300` の hash `0xdf5ff028506d418a` は、tick 600 に戻した B の state hash と一致する（A は `cz run --seed 42 --ticks 600` の `0xf7f61e25359e29dd`）。
5. 学習者の中を見るには、ヘッダの **world** で **learner**（全員 Learner）を選ぶ。`?policy=learner` の世界は `cz run --seed 42 --policy learner` と同じで、tick 1000 の hash は `0x83e036b522fecf63`。地図の個体をクリックすると「Agent」カードに **last step**（前 tick に決めた Q 表の行・行動の種類・報酬）と **table**（その行の Q 値 8 個。最大に `*`）が出る。零の初期 Q では未試行の種類が同点で最大になるので、各状態で全種類を 1 回ずつ試してから好みがつく様子が読める。**reflex**（進化する反射。突然変異 ON）と **mixed-learning**（forager / reflex / learner を founding index で輪番）も同様に `cz run --policy …` と hash が一致する。
6. 「Intervene」の **Set policy** は同じ介入の一般形: 対象（all / fraction = permille で各個体を seed 付きで抽選 / one agent）、kind、params（default = 白紙の表、keep = 学習者は学んだ Q を保つ）を選んで **Apply**（head のみ）か **Fork here**。台帳の履歴に `set policy → reflex for each agent with probability 500‰ (keep params)` のように記録される。

注意: 採集者の世界の fork で B を **Reflex** にすると、B の身体の軌跡は A と同一になる（hash だけ違う）。Reflex の初期重みは Forager 等価で、進化は世界の設定 `mutation_enabled` に依るところ、介入は設定を変えないから。進化する Reflex を見るには `reflex` / `mixed-learning` の世界を選ぶ。

## 9. Phase 3 の見どころ（EXP-002 の結果から）

6〜8 節の操作で何を見ると面白いか。数値は 10 シードの事前登録実験（`docs/research/experiments/EXP-002-learning-vs-fixed.md`）のもので、画面の 1 世界は「事例」にすぎない。

- **学習者の谷と回復**: 「Compare: fixed vs learning」で B を Learner にすると、tick 300〜450 で約 1,440 体から 500 体前後まで落ち、その後 A（採集者）より多い個体数に回復する。白紙の Q 表を埋めるまでの死と、埋めてからの回転の速さ（出生 2.4/tick、寿命 ≈ 660 tick）。
- **加害死の多さ**: Branches の表の harms と、個体の台帳の `died of violence`。Learner の死因の 43% が加害死で、源は ε = 5% の探索（殴ることを学んだのではない）。制度（Phase 4）が無い世界のベースライン。
- **個体数が多いのに立っている食料は少ない**: Compare の food on map（A 7,428 vs B 9,997 は tick 600、長く回すと B が少なくなる）。学習者の集団はセルを満杯まで放置しないので世界の再生量が増える。1 体あたりの摂取はほぼ同じ（intake / agent / tick ≈ 0.17）。個体数が多い = 賢い、ではない。
- **「飢えながら休む」**: `?policy=learner` で空腹・在庫なし・食料なし・隣接なしの個体を選ぶと、last step が `rest` で報酬が −5 の列になっていることがある。死に終端報酬が無い設計（ADR-0013 D6）の帰結の候補。
- **移転や発信は学習ではない**: B にだけ出る移転の線と発信の輪は、無料の行動が同点で選ばれているだけ（EXP-002 P5f）。記号の意味も分かち合いも学んでいない。

## 10. 規則を布告し、違反と制裁を読み、規則の有無で比べる（Phase 4、4-8）

Phase 4（ADR-0014）では、世界の中に**規則**（先占の侵害・約束の不履行・加害のどれを違反とし、違反者を何 tick 係争中にし、どの応答を制裁と数えるか）を置ける。規則は物理を変えない: 違反は禁止されず、制裁は普通の加害や拒否で、「係争中の個体に届いた」ときにカーネルが制裁と**読む**だけである（D1 / D4）。画面はカーネルと cz-web の読み手が書いた事実を並べるだけで、ページは何も判定しない（D12）。

1. **https://two-three-co.github.io/civilization-zero/?nomos=1&policy=learner&seed=3** を開く（`?nomos=1` で世界は `WorldConfig::nomos_preset()`（EXP-003 の決定: 制度オン・ε 10‰・加害コスト 2,000）で建てられ、`cz run --preset nomos --seed 3 --policy learner` と同じ hash になる。ヘッダの **institutions** チェックでも同じ。既定の URL では制度が無く、布告はカーネルに拒否される）。ヘッダに `institution · 0 rules · 0 accused` のバッジが出る。
2. `tick` 欄に 300 と入れて **Go**。「Intervene」で **Enact** を選ぶ: norm = encroachment（先占の侵害）、standing ticks = 32、sanction: harm（CLI `--enact encroach` の既定）。**Apply** を押すと「Ledger · interventions」に `tick 300 enact encroachment · accused 32 ticks after each violation · sanction: harm`、その下の「Institution · ledger」の **Timeline · decrees** に `tick 300 rule #0 enacted by decree: …` が出る。規則 id `#0` はカーネルが付けたもので、読み手が台帳の布告を原点から辿って復元している（4-7）。同じカードの上段は状態そのもの: 施行中の規則、係争中の件数（種別）、憲章（動議は 4-5 まで無い）。
3. `30 tick/frame` で **Play**。バッジが `institution · 1 rule · 500 accused` 前後になり、地図の係争中の個体に輪郭が付く（琥珀 = 係争中、紫 = 係争中でいま殴ると制裁になる）。赤い ✕ の加害のうちカーネルが制裁と読んだものには紫の大きな ✕ が重なる。右列:
   - **Norms · rates**: 直近 100 tick の台帳差分。E（侵害/採集）≈ 0.14、違反 ≈ 23/tick、制裁 ≈ 3〜4/tick、S′（制裁/違反）≈ 0.15、S（制裁された係争/閉じた係争）≈ 0.2、係争中 ≈ 520。`cz sweep` の窓の列と同じ定義で、速度を変えても読みは同じ。
   - **Violations · recent**: 既定では違反は台帳に記録されない（約 21 件/tick、D9）ので、いま開いている係争が `#6324 → #7027 accused under rule #0 (encroachment) since tick 2567 (violations 4, sanctions 1) · until tick 2599` のように並ぶ。**record violations**（`&violations=1`）で開き直すと、直近 100 tick の違反が「誰が・どの規則を・誰に・いつ・根拠（`gathered 2 on cell (40, 119) where the other arrived first`）」で並ぶ（tick 2,580 で約 2,350 件）。
   - **Sanctions**: 直近 100 tick の制裁が「`#7286 → #5139 sanctioned by a harm under rule #0 (encroachment) · answering the episode open since tick 2560 (violations 3, sanctions 1, still open)`」のように、**答えた係争**付きで並ぶ。係争が閉じた後は、違反が記録されていれば `closed · rebuilt from the recorded violations`、無ければ `episode unknown`。
   - 一覧の `#id` を押すか地図をクリックすると「Agent」カードに **arrived**（先占の到着 tick）、**standing**（`Accused under rule #0 (encroachment) since tick 56 (violations 1, sanctions 1), until tick 88 · against #527`）、**voted**（動議が無いので `no motion open`）が出て、記憶と台帳に `sanctioned by #527 (−20 health)` / `sanctioned by #527 (harm, rule #0)` が読める。
4. 「Branches」の **Compare: with vs without rule** を押す（隣の選択は規則の種類。既定は encroachment）。A にその種類の規則が有れば B はそれを **repeal** し、無ければ B に CLI 既定の norm を **enact** する。ここでは B が `repeal rule #0` の世界になる。`15 tick/frame` で **Play** すると、Norms · rates の表に `A · B` の 2 列が出て B は違反・制裁とも 0/tick（規則が無いので何も違反にならない）、E は A も B も ≈ 0.14（侵害という**事実**は規則が無くても数える）。比較表には規則数・係争・侵害・違反・制裁の行が加わる（tick 2,715 で違反 55,712 / 52,582、制裁 15,642 / 15,183、alive +3）。
5. **Verify: fork again** → **MATCH**。布告も介入なので、2b-6 / 3-9c と同じ契約（同じスナップショット + 同じ介入 = 同じ歴史）がそのまま成り立つ。
6. 比較の読み方の注意: **Random の世界では規則の有無で物理が変わらない**（`?nomos=1&policy=random&seed=3` で tick 40 に布告し tick 80 で比べると、A / B の alive・食料・加害・発信は同一で、違反 23 / 18・制裁 13 / 5 だけが違う）。ランダム歩行者は規範を読まないので、制裁は「たまたま係争中の個体に届いた加害」である。学習者（Reflex / Learner）は観測の規範情報（`ACCUSED` / `VIOLATOR_ADJACENT` など、4-3）を読み、Harm のアクチュエータが係争中の隣人を優先するので、規則の有無で決定が分かれる。どちらの世界でも「制度が何を変えたか」は事前登録した指標（EXP-004、4-6）でしか言えない。
7. **Repeal** は施行中の規則を id で選んで外す（開いている係争は閉じ、台帳の `episodes_closed` に数えられる）。**Charter** は定足数・閾値・期限・改正可否の 4 欄で、動議（4-5）が読む。どちらも台帳に `Intervened` として載り、Timeline · decrees に `rule #0 repealed by decree (…)` / `charter replaced by decree: …` と出る。

## 分かっている制限

- 分岐は A と B の 1 組だけ。B をさらに分岐することはまだできない。
- B の介入履歴と個体インスペクタは未対応（B の地図にもマーカーは出る）。
- 世界の設定は seed（`?seed=`、既定 42）、方策構成（forager / mixed / random / reflex / learner / mixed-learning）、制度の有無（`?nomos=1`。オンなら `nomos_preset` = ε 10‰・加害コスト 2,000）と違反の記録（`?violations=1`）だけ変えられる。128×128、1,000 体、学習者の表の行数（32）は固定。
- 制度は布告（Enact / Repeal / Charter）でしか変わらない。動議・投票・`Enforcer`（議場で規則を提案し守らせる台本の方策）と `institution` の方策構成は 4-5 で入る。規則は物理を変えず、制裁は普通の加害・拒否をカーネルが「制裁」と読んだもの（ADR-0014 D1 / D4）。率のカーブの E は方策別ではなく世界全体（方策別は `cz sweep`）。学習者の報酬は自分のエネルギーだけなので、分かち合いは出なくて当然（ADR-0013 D6）。
- 発信（`Say`）は既定では台帳に載らない（`record_signals` で ON）。地図の輪は各個体の「最後の発信」から描いているので、巻き戻した tick では消える。移転・加害・提案・受諾・拒否は常に台帳にある。
- 学習者の ε と加害のコストは固定（既定世界 5% / 制度の世界 10‰、加害コストは `nomos_preset` で 2,000）。既定世界の暴力水準はこの 2 つの帰結で、学習の結果ではない（docs/research/RISKS.md R8）。
