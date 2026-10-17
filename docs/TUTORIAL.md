# Toy 1 — 5 分チュートリアル

Civilization Zero の最初の遊べるマイルストーン。ブラウザだけで、文明を観察し、巻き戻し、介入し、分岐した歴史を並べて比較する。
画面に出る数字はすべてこのタブの中でカーネル（Rust → wasm）が計算したもので、サーバーには何も送らない。

## 開き方

- 配信 URL: **https://two-three-co.github.io/civilization-zero/**（`main` の最新ビルド。フッタのコミットで版が分かる）。
- ローカル: リポジトリで `cd web && npm install && npm run build:wasm && npm run dev` → `http://127.0.0.1:5173/`。
- 配布された `dist/` フォルダ: 任意の静的サーバーで配信する（例: `python3 -m http.server 8766` を `dist` の親で実行し `/dist/` を開く）。`file://` では Worker が動かないブラウザがある。

画面右上のバッジが **kernel · wasm** なら本物。`mock worker` と出ていたら `?mock=1` 付きで開いている（見た目確認用の偽物）。
右列の「Five-minute tour」カードが、下の 5 ステップを自動で進捗判定する。

## 1. 観察する（Play）

**Play** を押す。オレンジが採集者（forager）、青がランダム歩行者、緑が食料。個体の明るさはエネルギー。
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

## 分かっている制限

- 分岐は A と B の 1 組だけ。B をさらに分岐することはまだできない。
- B の介入履歴と個体インスペクタは未対応（B の地図にもマーカーは出る）。
- 世界の設定は seed（`?seed=`、既定 42）と方策構成（forager / mixed / random）だけ変えられる。128×128、1,000 体は固定。
- 制度・学習はまだ存在しない（Phase 3〜4）。スレッドは台帳を読むだけで、不履行に対して世界の中で何かが起きるわけではない（制裁は Phase 4）。学習方策との比較は 3-9c。
