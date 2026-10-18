# EXP-004: 先占規範と制裁 — H1 の事前登録実験（LE / LE0 / L0 / L1 + 帰無モデル 5 本 + 副次腕 2 本）

- 仕様の登録日: 2026-10-18（実行前にこの文書と `data/EXP-004/run.sh` / `summarize.py` だけをコミットする。結果は実行後に「結果」節へ追記し、仕様の節は書き換えない）
- カーネル: `simulation-core` 0.1.0（ADR-0014 4-5 適用後の HEAD 7774adb の直後に 4-6 の準備コミット **afb3e26**（`WorldConfig::enforcer_share_permille`、`TickReport` の方策別の出生と制裁、`cz --enforcer-share / --blind / --signal-radius / --memory-slots`、`cz sweep` の 4-6 の列、スナップショット形式 13）を足したもの。Forager のゴールデン `0x658ee873d1866501`、Learner 32 行のゴールデン `0xcb8bc0a9772c09a4` は不変）。実行に使うバイナリのコミットは結果節に記す
- 機種: Apple M1（8 コア、16 GB）, rustc 1.99.1, release build
- 種別: **研究仮説 H1 の検証**（ADR-0014 D11、完了条件 3、HYPOTHESES.md の H1）。規則 R = `Encroach { standing_ticks: 32, sanction: HARM }` を布告で施行した世界で、学習者の侵害率が、制度を担う個体（台本の `Enforcer`）のいる条件で下がるかを、同じ seed の対照と帰無モデル 5 本と比べて測る。**`Enforcer` は設計者の知識そのもの**（ADR-0014 D7）で、LE の結果は「制度を担う個体がいれば学習者が従う条件がある」以上を意味しない。規則は布告で与える（発明ではない）。「創発」「世界初」「所有権が生まれた」は使わない

## 問い

H1（HYPOTHESES.md）: 「所有権を事前定義しなくても、競合・記憶・通信・制裁から所有権相当の規範が成立する条件がある」。主要指標は「先占者以外の採取」率とそれに続く制裁の条件付き確率、両者の時間的低下と持続。帰無モデルはランダム方策 / 通信なし / 記憶なし / 制裁不能。

Phase 4 で答えられるのはその最初の半分、「ルールが世界の中の社会的事実として存在し、個体がそれを守り・破り・罰することができ、その効果を事前登録した指標で測れる」か（ADR-0014 の文脈）である。本実験の問い:

1. **学習者の侵害率は、制度を担う個体と規則のある社会で下がるか。** 主腕 LE（Learner 128 行 90% + Enforcer 10%、規則 R）と、同じ seed の LE0（同じ構成、規則なし）の、後半の窓の学習者の侵害率 E_L の対の差。
2. **学習者だけで制裁は出るか。** L1（Learner のみ + R）で制裁の条件付き確率 S′ が探索の床を超えるか（予測: 超えない。制裁に報酬の利得がない）。
3. **何が効いているか（帰無モデル）。** 制裁できない規則（N-nosanction）、規範情報を見ない学習者（N-blind）、通信なし（N-nocomm）、記憶なし（N-nomem）、ランダム方策（N-rand）で、LE の差がどこまで残るか。
4. **制度のコスト。** LE の個体数・死因・摂取は LE0 からどう動くか。
5. **（副次、探索的）制裁が「痛い」と報酬が言えば何が変わるか。** 体力項 `learner_health_weight` 2 の世界（W0 / W1）で、規則の効果の大きさと死因が変わるか。H1 の判定には使わない。

## 腕

世界はすべて `WorldConfig::nomos_preset()`（EXP-003: `nomos` オン・ε 10‰・`harm_cost` 2,000・体力項 0）× Learner **128 行**（新ビット順、ADR-0014 D6）、128²、1,000 体、20,000 tick、`enforcer_motions: false`（Enforcer は動議しない。施行はすべて布告）。規則 R = `Encroach { 32, HARM }` を **tick 0 に布告**（`--enact encroach`。CLI の既定がこの規範）。

| 腕 | ファイル名 | シード | `cz sweep` の指定（共通部分は下の計測コマンド） | 役割 |
|---|---|---|---|---|
| **L0** | `l0` | 0〜29 | `--policy learner --preset nomos --learner-states 128` | ベースライン（E の時間変化は学習だけでも起きる。EXP-003 の L128 と同じ世界の ε 10 版） |
| **L1** | `l1` | 0〜9 | L0 + `--enact encroach` | 学習者だけで制裁が出るか |
| **LE0** | `le0` | 0〜29 | `--policy institution --no-enforcer-motions --enforcer-share 100` | Enforcer の存在そのものの効果（規則なしでは制裁しないが侵害もしない） |
| **LE** | `le` | 0〜29 | LE0 + `--enact encroach` | **主腕**: 制度を担う個体がいる社会で学習者の侵害率が下がるか |
| **N-rand** | `n_rand` | 0〜9 | `--policy random --preset nomos --enact encroach --sample 10` | 帰無（方策）。tick 2,000 までに絶滅するので（4-5 の計測）、系列は 10 tick ごとに取る |
| **N-blind** | `n_blind` | 0〜9 | LE + `--blind`（`perception.see_standing: false`） | 帰無（規範情報の経路 (a)）: 学習者は係争を見ず、Harm のアクチュエータも係争者を狙わない |
| **N-nosanction** | `n_nosanction` | 0〜9 | LE0 + `--enact encroach:32:none` | 帰無（制裁不能）: 違反と係争は記録されるが誰も制裁できない |
| **N-nocomm** | `n_nocomm` | 0〜9 | LE + `--signal-radius 0` | 帰無（通信なし）。v1 の制裁は通信を使わない（係争は NormIndex で公知）ので差がないはず。**差がないことは設計の限界の記録であって証拠ではない** |
| **N-nomem** | `n_nomem` | 0〜9 | LE + `--memory-slots 0` | 帰無（記憶なし）: Enforcer の「同じエピソードに 1 回」の歯止めが消える |
| W0（副次） | `w0` | 0〜9 | LE0 + `--learner-health-weight 2` | 探索的: 体力項のある世界の対照 |
| W1（副次） | `w1` | 0〜9 | LE + `--learner-health-weight 2` | 探索的: 制裁の痛み（−20 体力 × 2）が報酬に入る世界で規則の効果はどう変わるか |

`--policy institution` は Learner 128 行の創設集団の 10 体に 1 体（`i % 10 == 0`）を `Enforcer` にし、`nomos_preset` と 128 行を暗黙に適用する（4-5）。

**構成の維持（4-5 からの含意への答え）**: 4-5 の計測では、Forager 等価の Enforcer が白紙の 128 行 Learner の谷を埋め、`--policy institution` の世界は tick 300 で生存の 97% が Enforcer になった（ADR-0014 D7 / D11 の追記）。本実験の制度世界（LE0 / LE と帰無 4 本、W0 / W1）は **`--enforcer-share 100`** = `WorldConfig::enforcer_share_permille = Some(100)` で構成を保つ: 出生のたびに、親が Learner か Enforcer なら、子は「Enforcer の数 × 1000 < 100 × (生存数 + 1)」のとき Enforcer、そうでなければ Learner に生まれる（親の種類に依らない。親と種類が違う子は種類の白紙のパラメータ = Enforcer の子の Learner は白紙の Q 表、Learner の子の Enforcer は表なし。ADR-0014 D11 の追記、`tests/enforcer_share.rs`）。これは**実験者の介入**で、Enforcer の割合は構成上 10% に保たれる。帰無モデルと W の腕も同じ規則で走る。L0 / L1 には Enforcer がいないので規則は何もしない（`--enforcer-share` を付けない）。

## 固定する設定

- 128×128、初期 1,000 体、資源パッチ 23（CLI の `side²/700`）、seed は腕の表のとおり（LE / LE0 / L0 は 0〜29、他は 0〜9。登録前のスモークは seed 100〜102 で行い、登録データには含めない）。
- `nomos_preset`: `nomos` オン、`learner_epsilon_permille` 10、`harm_cost` 2,000、`learner_health_weight` 0（W0 / W1 だけ 2）。他は `WorldConfig::default()`（`harm_damage` 20 / `transfer_cost` 100 / `signal_cost` 0 / `memory_slots` 8（N-nomem は 0）/ `social_radius` = `signal_radius` = 2（N-nocomm は `signal_radius` 0）/ `learner_inherit_q` true / `mutation_enabled` false / `birth_threshold` 80,000 / `child_energy` 40,000 / `max_age` 3,000 / `metabolism_cost` 500 / `move_cost` 300 / `rest_heal` 1 / `starvation_damage` 5 / 憲章 100‰ / 667‰ / 32 tick / amendable / `motion_cost` 2,000 / `record_violations` off / `record_votes` off / `perception.see_standing` true（N-blind は false））。
- `learner_states` 128、`enforcer_motions` false、`enforcer_share_permille` 100（制度世界）。
- 規則 R = `Encroach { standing_ticks: 32, sanction: HARM, except_sanction: false }`、tick 0 の布告。N-nosanction は `Encroach { 32, NONE }`。規則は 1 つだけ。布告以外の介入はしない。N-rand では Random が動議・投票するので（4-5）、R が死にかけの議会で廃止・置換されうる。それは台帳の事実として `motions_carried` に残る。
- **20,000 tick**、窓 = 後半 50%（tick 10,001〜20,000、`--window 50`）、系列のサンプル間隔 100 tick（`--sample 100`。N-rand だけ 10）。
- 計測コマンド（`out/` は gitignore なので実行後に `data/EXP-004/` へコピーする。`data/EXP-004/run.sh` が同じループで、各腕の標準出力と壁時計時間を `RUN.log` に残す。**zsh ではなく `sh` で実行する**: zsh は `"$EXTRA"` のような変数を単語に分割しないので、`--enact encroach` が 1 語になって規則なしで走る。登録前のスモークの 1 回目がそれで無効になった）:

```bash
cargo build --release -p cz-cli
C="--ticks 20000 --window 50 --sample 100"
I="--policy institution --no-enforcer-motions --enforcer-share 100"
L="--policy learner --preset nomos --learner-states 128"
./target/release/cz sweep $C --seeds 30 $I --out out/EXP-004/le0
./target/release/cz sweep $C --seeds 30 $I --enact encroach --out out/EXP-004/le
./target/release/cz sweep $C --seeds 30 $L --out out/EXP-004/l0
./target/release/cz sweep $C --seeds 10 $L --enact encroach --out out/EXP-004/l1
./target/release/cz sweep $C --seeds 10 --policy random --preset nomos --enact encroach --sample 10 --out out/EXP-004/n_rand
./target/release/cz sweep $C --seeds 10 $I --enact encroach --blind --out out/EXP-004/n_blind
./target/release/cz sweep $C --seeds 10 $I --enact encroach:32:none --out out/EXP-004/n_nosanction
./target/release/cz sweep $C --seeds 10 $I --enact encroach --signal-radius 0 --out out/EXP-004/n_nocomm
./target/release/cz sweep $C --seeds 10 $I --enact encroach --memory-slots 0 --out out/EXP-004/n_nomem
./target/release/cz sweep $C --seeds 10 $I --learner-health-weight 2 --out out/EXP-004/w0
./target/release/cz sweep $C --seeds 10 $I --enact encroach --learner-health-weight 2 --out out/EXP-004/w1
```

- 見込みの実行時間: スモーク（M1）の 2,000〜3,100 tick/s から 1 シード 7〜10 s、30 × 3 + 10 × 8 = 170 シードで **約 25 分**（他の重い処理と同時に走らせない。他の Claude Code セッションは idle であることを確認する）。
- `cz sweep` の出力列は EXP-003 の列（名前・順序・意味とも不変）に 4-5 の列と、本仕様の直前のコミット（afb3e26）が末尾に足した 4-6 の列（series: `births_* / births_reassigned / sanctions_* / alive_*`、summary: `enforcer_share … win_births_reassigned_per_tick`）を足したもの。

## 分析窓と指標

系列（`<腕>_series.csv`、100 tick ごと）の行 t は区間 (t − 100, t] の計数を持つ。区間 (a, b] の値は **a < t ≤ b の行を合算**して比にする（率の平均ではなく合算比。採集の少ない窓で率が暴れないため）。

- **窓**: tick 2,000〜20,000 を 100 tick の窓 w_k = (2,000 + 100k, 2,100 + 100k]（k = 0〜179）に切る。**序盤** = (2,000, 4,000]、**後半** = (10,000, 20,000]（= `cz sweep` の `--window 50` の窓: summary の `win_*` 列と同じ区間）、**末尾** = (15,000, 20,000]。tick 0〜2,000 は白紙の表の谷（EXP-003: 128 行の谷は tick 200、回復は tick 1,000 まで）として分析に使わない（ただし記述する）。
- N-rand だけは絶滅までの「生涯」を 10 tick ごとの行で読み、生涯の前半 / 後半（絶滅 tick の半分で分ける）を窓にする。

| # | 指標 | 定義（系列の列） | 読む腕 |
|---|---|---|---|
| **M1** | 絶滅 | `extinct_at`（summary）。腕ごとの絶滅シード数と tick | 全腕 |
| **M2** | **学習者の侵害率 E_L(窓)** | Σ `encroachments_learner` / Σ `gathers_learner`（先占者以外の採取 / 成功した採集、ADR-0014 D3 の操作的定義）。**主指標は後半の E_L**。N-rand は `_random` の列で E_R | 全腕 |
| **M3** | 制裁の条件付き確率 | **S(窓)** = Σ `episodes_sanctioned` / Σ `episodes_closed`（エピソード単位）、**S′(窓)** = Σ `sanctions` / Σ `violations`（件数単位。複数の Enforcer が同じ違反者を 1 回ずつ殴るので 1 を超えうる）。方策別: **S′_E** = Σ `sanctions_enforcer` / Σ `violations`、**S′_L** = Σ `sanctions_learner` / Σ `violations` | 規則のある腕 |
| **M4** | 時間的低下と持続 | ΔE_L = E_L(後半) − E_L(序盤)。持続 = E_L(末尾) ≤ 0.8 × E_L(序盤)（D11 の定義。満たす / 満たさないを腕ごとに数える） | 全腕 |
| **M5** | 個体数と構成 | 窓の各サンプル tick の `alive` / `alive_learner` / `alive_enforcer` の平均、Enforcer の割合（‰）の最小と最大（tick ≥ 2,000 の全サンプル）、Learner の最小 | 全腕 |
| **M6** | 死因 | 窓の Σ `deaths_violence` / Σ(全死因)（加害死の割合）、死亡/tick | 全腕 |
| **M7** | 摂取 | `win_consumed_per_agent_tick`（summary、後半の窓） | 全腕 |
| **M8** | 学習者の Harm | `win_act_harm`（summary、決定のうち Harm の割合、Enforcer 込み）、`win_sanctions_learner_per_tick`、`<腕>_qpolicy.csv` の **行 64〜127（`violator_adjacent` = 1）** で終了時の生存学習者の貪欲行動の多数派が `harm` の行の数（訪問済みの行だけ） | 学習者のいる腕 |
| **M9** | 構成の維持（前提条件） | tick ≥ 2,000 の全サンプル tick で Enforcer の割合が **80〜120‰** かつ Learner ≥ **300 体** | 制度世界 |
| **M10** | 白紙の学習者の流入 | `births_reassigned` / tick（Enforcer の子として白紙で生まれた Learner + Learner の子として生まれた Enforcer）と `births` / tick | 制度世界 |
| **M11** | 議会 | `motions / motions_carried / motions_defeated`（summary、全区間） | N-rand（他は 0 のはず） |
| **M12** | 実行時間 | `ticks_per_s`、`RUN.log` | 参考値 |

**対の比較**は同じ seed どうし（同じ創設集団の位置・年齢。LE と LE0 は布告の有無だけが違う。L0 と LE0 は創設集団の 10% の方策が違う）。

## 統計（事前に固定）

- 腕どうしの対比はすべて**同じ seed の対の差** d_i = x_i(腕 A) − x_i(腕 B)。報告するのは: 対の平均、**95% ブートストラップ信頼区間**（対の差を 10,000 回復元抽出した平均の 2.5 / 97.5 パーセンタイル。乱数は `random.Random(20261018)`）、**効果量 d_z** = 平均 / 差の標本標準偏差（ddof 1）、比 = 平均(A) / 平均(B)、予測の向きに差が出たシード数。p 値だけの報告はしない（HYPOTHESES の規則）。
- 主対比（LE − LE0、LE0 − L0、LE − L0）は **30 シード**。帰無モデルと W は 10 シードで、LE / LE0 との対は seed 0〜9 で取る（CI は広い: 10 シードの対比は帰無モデルの**検査**であって H1 の主張ではない）。
- **最小関心効果（SESOI）**: E_L の対の差 **0.01**（絶対値。規則なしの E₀ ≈ 0.08（EXP-003 の 128 行）の 1/8）。CI が 0 を含まず、かつ平均の絶対値 ≥ 0.01 のとき「下がった / 上がった」と書く。CI が [−0.01, +0.01] と重なる差は「SESOI 未満」。
- 絶滅したシード（M1）は対から除く（除いた数を報告する）。LE / LE0 の対が **3 対以上**失われたら H1 の判定は「この設計では読めない（制度のコストが支配）」と記録し、判定文を書かない。
- 単一シードの値は事例としてのみ扱う。

## 判定規則と予測（実行前に固定）

数値の根拠: EXP-003（128 行の規則なしの E₀ 0.078、ε 10 の Harm の決定割合 ≈ 0.0019 = 探索の床 0.0012 + 貪欲の残り 0.0007、preset × 128 行の加害死 12.9%）、4-5 の計測（`--policy institution --enact encroach` 3,000 tick で Learner の E 0.048、S′ 2.05）、ADR-0014 D11 の予測の形。**登録前のスモーク（seed 100〜102、`data/EXP-004/SMOKE.log`）は構成の維持と絶滅の確認のために LE0 / LE を 20,000 tick 走らせたもので、その E_L（LE0 0.077〜0.087、LE 0.011〜0.015、後半）・加害死・個体数は予測を固定する前に見ている**。D11 の予測の向きは 2026-10-17 に（実行前に）固定されており、SESOI 0.01 は設計の値だが、(1b)・(1c)・(7) の数値はスモークを見たうえで書いた。登録シード 0〜29 はスモークと重ならない。

| # | 予測（支持される条件） | 反証される条件 | 根拠 |
|---|---|---|---|
| **P0a** 構成 | LE0 / LE の 30 シード、帰無 4 本と W の 10 シードすべてで M9 を満たす（tick ≥ 2,000 の全サンプルで Enforcer 80〜120‰、Learner ≥ 300） | 1 シードでも外れる | 出生の規則は構成上 100‰ を保つ（スモーク: 98〜102‰、Learner ≥ 708） |
| **P0b** 生存 | L0 / LE0 は絶滅 0/30、LE は ≤ 1/30、帰無 4 本と W は ≤ 1/10 | 超える | スモーク 0/3。ただし LE の谷は深い（tick 400 に 84〜133 体）ので 1 件は許す |
| **P1a** 主対比（H1） | 後半の E_L が **LE < LE0** で、seed 0〜9 のうち ≥ 8/10、かつ 30 シードの対の差の 95% CI が 0 を含まず、平均の絶対値 ≥ 0.01 | 8/10 未満、CI が 0 を含む、または \|平均\| < 0.01 | D11 の (1)。機構は下の P1c（体力項 0 では制裁は学習者の報酬に現れない: 侵害者の除去と継承 Q の選択） |
| **P1b** コスト | 後半の Learner の個体数が LE < LE0 で ≥ 8/10（30 シードの CI が 0 を含まない）、加害死の割合が LE > LE0 で ≥ 8/10 | いずれかが外れる | 制裁は加害（−20 体力）で違反者を削る。スモーク: 加害死 0.5〜0.7 vs 0.12〜0.14、Learner 1,080〜1,160 vs 1,510〜1,580 |
| **P1c** 時間構造 | (i) 序盤の窓で既に E_L が LE < LE0（≥ 8/10、CI が 0 を含まない）。(ii) LE の E_L(後半) / E_L(序盤) ≥ 0.5（後半でさらに半減しない）。(iii) D11 の「持続」（末尾 ≤ 0.8 × 序盤）は **LE でも LE0 でも L0 でも満たすシードが ≤ 5/10**（序盤から末尾への低下は規則の有無に依らず 10〜20% の学習の漂移） | (i) が外れる、(ii) < 0.5、または (iii) で LE だけが ≥ 7/10 | 体力項 0 では制裁は報酬に現れず、効果は布告直後から効く「除去」で、累積する学習ではない。(iii) が LE だけで満たされれば累積する機構（継承 Q の選択）の証拠で、どちらも記録する |
| **P2a** L1 の制裁 | L1 の後半の S′ ≤ **2 × 床**。床 = 0.0012 × N_L / V（N_L = `win_alive_learner_mean`、V = `win_violations_per_tick`。探索で決めた Harm が全部係争中の隣人に当たったときの S′）で、≥ 9/10 シード | 超えるシードが 2 つ以上 | D11 の (2): 制裁に報酬の利得がないので学習者は制裁を学ばない。Harm の決定は床 0.0012 + 貪欲 0.0007（EXP-003）。2 倍は貪欲の残りの余裕 |
| **P2b** 制裁の学習 | L1 / LE の `_qpolicy.csv` で、行 64〜127（係争者が隣接）のうち訪問済みの行の貪欲多数派が `harm` の行は、**どのシードでも ≤ 3 行**（スモークの LE では 0〜2 行 / 26〜31 行）。`win_act_harm` は L1 ≤ 1.5 × L0（seed 0〜9 の対で ≥ 9/10） | 4 行以上のシードがある、または Harm の決定が 1.5 倍を超える | 同上 |
| **P2c** L1 と L0 | 後半の E_L の対の差（L1 − L0、10 シード）の 95% CI が [−0.01, +0.01] と重なる | CI が帯の外 | D11 の (2) 後半。学習者どうしの制裁（床 ≈ 1.5 件/tick）は除去として弱い |
| **P3** N-nosanction | **LE0 と物理が同一**: seed 0〜9 で series の物理列（`alive, food, gathered, consumed, births, deaths_*, gathers, encroachments, harms, gathers_learner, encroachments_learner, alive_learner, alive_enforcer`）が全行一致、`violations` > 0 かつ `sanctions` = 0。したがって E_L の差は 0（D11 の (3) は構造的に成立） | 1 行でも違う | 128 行の学習者が読む規範ビットは HARMABLE から派生し、制裁なしの規則では誰も HARMABLE にならない。Enforcer の規則 1 も発火しない（ADR-0014 D11 の追記、seed 100 で 3,000 tick 一致）。**これは実装の検査で行動の証拠ではない**。情報のある対比は N-nosanction（= LE0）vs LE: 差はすべて制裁の経路に帰属する |
| **P4** N-blind | 後半の E_L が N-blind < LE0 で ≥ 8/10（盲目でも下がる）。N-blind − LE の対の平均は [−0.01, +0.01] の中、ただし **N-blind ≥ LE が ≥ 7/10**（学習者の制裁が消えて除去が弱まる）。`win_sanctions_learner_per_tick` は N-blind < 0.2 × LE で 10/10 | N-blind が下がらない、差が SESOI を超える、または学習者の制裁が残る | D11 の (4)。学習者の「制裁」は Harm のアクチュエータが係争中の隣人を優先する語彙の帰結（R1）で、学習ではない。盲目ではそれが消える（スモーク: 227 → 18 件 / 100 tick） |
| **P5** N-rand | (a) 10/10 シードが tick 2,000 までに絶滅。(b) E_R（生涯後半）≥ 0.5 × E_R（生涯前半）が ≥ 8/10（低下しない）。(c) `motions_carried` ≤ 1 / シード | (a) 生き残る、(b) 8/10 未満、(c) 2 件以上 | D11 の (5)、4-5 の計測（tick 187〜263 で絶滅、可決は死にかけの議会だけ）。S′ は記述し、床とは比べない（Random の Harm は一様で、係争者の割合という偶然の積） |
| **P6** N-nocomm | 後半の E_L の対の差（N-nocomm − LE）の平均が [−0.01, +0.01] の中で、CI が 0 を含む | 差が SESOI を超える | D11 の (6): v1 の制裁は通信を使わない（係争は公知）。物理は LE と同一ではない（`Heard` の記憶が Enforcer の `recent` の枠を争う）が、差は雑音のはず |
| **P7** N-nomem | (a) 後半の S′ が N-nomem > LE で ≥ 8/10。(b) S は N-nomem ≥ LE で ≥ 7/10（LE の S はスモークで 0.95〜0.97: 天井に近い）。(c) E_L は N-nomem ≤ LE で ≥ 7/10。(d) Learner の個体数は N-nomem < LE で ≥ 7/10 | いずれかが外れる | D11 の (7): 記憶なしの Enforcer は同じ係争者を毎 tick 殴る（エネルギーの門まで）。除去が強まる |
| **P8** W（副次） | (a) 加害死の割合が W1 < LE で ≥ 7/10。(b) 後半の E_L が W1 ≤ LE で ≥ 7/10。(c) 規則の対の差（W1 − W0）の符号が（LE − LE0）と同じで ≥ 8/10。W0 の Rest の決定割合は LE0 より高い（EXP-003 の体力項の副作用の再現） | 外れる | 体力項 2 では制裁（−20 体力）が −40 報酬単位として Q に入り、係争中の隣人の近くの採集（+80）に遅れて効く。学習者が先占者のいるセルを避ければ制裁も除去も減る。**H1 の判定には使わない** |

集計規則: 平均・sd（標本、ddof 1）は腕のシード全部で取る。腕どうしの比較は対の差の統計（上の「統計」）。10 シードの対比は検査。`summarize.py` が表と P0〜P8 の二値判定を機械的に出す。

## 主張の書き方（H1 の判定文）

結果節には次の形の文だけを書く: 「**条件 LE で学習者の侵害率（後半の窓）が、同じ seed の LE0 に比べ X 下がった（30 シード、対の平均 −X、95% CI [a, b]、d_z = …、比 …）。N-nosanction では下がらなかった（LE0 と同一）。**」H1 の判定はこの文の成否（P1a と P3）: P1a が外れれば「この設定では先占規範の制裁は学習者の侵害率を下げなかった」と記録し、H1 は未決のまま。

判定文の後に、機構の読みを必ず付ける: 体力項 0 の世界では制裁は学習者の報酬に現れない（EXP-003 / ADR-0014 の前提条件 2）ので、LE の低下は (a) 制裁による侵害者の除去（加害死）、(b) 継承される Q 表への選択、(c) Enforcer が先占者のいるセルを避ける語彙、の合成であり、**学習者が規範を学んだ証拠ではない**。P1b / P1c / P4 / P8 がどれに重みがあるかを示す。「創発」「世界初」「所有権が生まれた」は書かない。

## 既知の限界（事前）

- **Enforcer は設計者の知識**（ADR-0014 D7、R1）。LE で侵害が減っても「従わせる個体を置けば従う」以上ではない。学習方策は立法しない。規則は布告で与え、発明は問うていない。
- **体力項 0 では制裁が学習者の報酬に入らない**（前提条件 2）。学習者が規範に「従うことを学ぶ」経路は本実験には無く、ある差はすべて除去・選択・Enforcer の行動の帰結。W1 / W0 はその限界を探索的に見るだけで、体力項 2 は Learner の目的を変える（EXP-003: Rest が倍増）。
- **構成の維持は実験者の介入**: Enforcer の割合は出生時の割当で 10% に固定され、Enforcer の子として生まれる Learner は白紙の表を持つ（スモーク: 0.2 件/tick ≈ 出生の 10%）。L0 / L1 にはこの流入がない。LE0 − L0 の差には Enforcer の存在とこの流入の両方が入る。
- **N-nosanction は LE0 と物理が同一**（P3）、**N-nocomm は LE とほぼ同一**。帰無モデル 4 本のうち 2 本は設計の帰結を記録するだけで（係争は公知 = R12、v1 の制裁は通信を使わない）、「通信がなくても制度が成り立つ」証拠にはならない。
- **布告が tick 0** なので、白紙の表の谷に制裁が重なり LE の谷は深い（スモーク: tick 400 に 84〜133 体）。絶滅の扱いは「統計」のとおり。tick 2,000 に布告する設計（谷の後）は比較の窓の都合で採らなかった（D11）。
- **侵害の操作的定義は到着順**（D3）で、家族は共同の先占者。密集では誰もが侵害者になりうる。E_L は「所有」の何かを測っていない。
- **S′ は 1 を超えうる**（複数の Enforcer が同じ違反者を 1 回ずつ殴る）。S はエピソード単位で天井が近い。
- **学習者の「制裁」は語彙の帰結**: Harm のアクチュエータが係争中の隣人を先に狙う（D6）ので、探索の Harm が制裁として数えられる。N-blind がこれを切り分ける。
- **10 シードの帰無モデルの CI は広い**。10 シードの対比は検査。
- **スモークを見てから数値を書いた項目がある**（上の「判定規則と予測」の冒頭）。登録シードはスモークと重ならない。
- 窓の率は合算比で、窓の途中の分布（侵害者の個体差）は見ない。採集・侵害は方策別に数えるが死因・摂取は方策別に無い（Learner の加害死 ≠ 全体の加害死。制度世界では Enforcer も殴られうる）。
- 実行時間は `cz sweep` の総時間で参考値。

## 登録前のスモーク（開示）

`data/EXP-004/SMOKE.log`、`smoke_le0_*.csv`、`smoke_le_*.csv`（seed 100〜102、20,000 tick、本仕様の直前のコミット afb3e26 の作業ツリーのバイナリ）。目的は `--enforcer-share 100` が構成を保つか（M9）と、LE が絶滅しないか。見えたもの: LE0 は Enforcer 99〜101‰・Learner ≥ 1,423 体・絶滅 0/3、E_L（後半）0.077〜0.087。LE は 98〜102‰・Learner ≥ 708 体・絶滅 0/3、谷は tick 400 に 84〜133 体、E_L（後半）0.011〜0.015、S 0.95〜0.97、S′ 3.1〜3.6（Enforcer 4.1〜5.5 件/tick + Learner 1.2〜1.7 件/tick）、加害死の割合 0.53〜0.70（LE0 は 0.13〜0.14）、Learner 1,080〜1,160 体（LE0 1,510〜1,580）。seed 100 の 3,000 tick で N-nosanction ≡ LE0、N-nocomm / N-blind / N-nomem は LE から分岐（ADR-0014 D11 の追記）。

## データ

`data/EXP-004/<腕>_summary.csv`、`<腕>_series.csv`、学習者のいる腕は `<腕>_qpolicy.csv`、`RUN.log`（各腕の標準出力と壁時計時間、コミット、バイナリの sha256）、`run.sh`（計測ループ。`sh` で実行）、`summarize.py`（CSV から本文の表を再生成し、対の統計と P0〜P8 の判定を機械的に出す: `python3 -I summarize.py [data_dir]`）、`SMOKE.log` と `smoke_*.csv`（登録前のスモーク）。
