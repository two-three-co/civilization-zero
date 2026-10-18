# ADR-0015: Phase 5 — Research Platform（実験ダッシュボードと H2 / H3 / H4 の実験）の設計

- 状態: 提案 → 採用 (2026-10-18)。**実装中**: 5-1（`cz-lab` クレート、D2）は 2026-10-18 に実装・検査・計測済み（タスク表の ✅、D2 の「実装と設計からの差分」、STATUS の Phase 5 表）、5-2〜5-14 は未着手。各タスクの着手・完了は STATUS.md の表に記録し、設計からの差分は ADR-0013 / 0014 と同じく各 D に「実装と設計からの差分」として追記する。完了判定は末尾の「完了判定」に書く（Phase 4 と同じ表）
- 前提: docs/PRODUCT.md（Phase 5 のデモ = 実験ダッシュボード: 複数シード・条件格子の結果分布と介入効果の比較）、docs/STATUS.md（Phase 4 完了判定、完了条件 2 の計測表、「観察された挙動（製品の制度世界）」）、docs/research/HYPOTHESES.md（H2 / H3 / H4 と「Phase 5 への引き渡し」）、docs/research/RISKS.md（R1, R3, R8, R9, R11, R12〜R14）、docs/research/experiments/EXP-003（格子と決定規則の形）/ EXP-004（腕・対・帰無モデル・`summarize.py` の形と「次の問い」）、ADR-0012（再生・fork）、ADR-0013（D6 学習方策、D7 fork + `SetPolicy`、D9 事前登録）、ADR-0014（D6 観測と学習方策、D7 議場と `Enforcer`、D9 決定論の契約、D11 EXP-004 の形、D12 読み手だけの UI）、docs/TODO.md の Phase 5 ブロック
- 用語: **Research Platform** = 本 ADR が置く 4 つの部品の総称。(1) **`cz-lab`**（native のライブラリクレート: 実験の**設計** `Design` の型、1 世界を走らせて窓ごとに集計する純関数、`cz sweep` と同じ列の CSV、対比の統計）、(2) **`cz-lab-web`**（同じ `cz-lab` を wasm-bindgen で包んだ第 2 の wasm パッケージ。ブラウザの Worker プールが読む）、(3) **実験ダッシュボード**（`web/src/lab/`。設計を選び、プールで回し、分布と対の差を描き、個々の世界を Toy 1 で開く）、(4) **設計ファイル** `Design`（JSON）と**結果** `Result`（`cz sweep` と同じ CSV + マニフェスト）。**格子** = 条件（セル）× シード、**腕** = 1 セルの別名（EXP-004 の用語）、**対比** = 同じ seed の 2 腕の差、**窓** = tick 区間の合算、**コホート** = 窓の開始時に生きていた学習者の集合。LLM はどこにも登場しない

## 文脈

Phase 4 は「人工社会は、自らを統治する実行可能なルールを発明できるか」の前半 — ルールが世界の中の社会的事実として存在し、個体がそれを守り・破り・罰することができ、効果を事前登録した指標で測れる — を終えた（ADR-0014 の完了判定）。EXP-004 の結果は H1 の判定文を成立させたが、機構は**違反者の除去**であって学習ではなく、盲目の帰無モデルも同程度に下がり、制裁を担う `Enforcer` は設計者の知識そのものだった（ADR-0014 の帰結、RISKS R12〜R14）。HYPOTHESES は Phase 5 に 2 つの前提問題 — **信用割当**（学習者が従うことを学べる経路）と**個体単位の追跡**（除去と学習の分離）— と、H2 の格子の軸（Harm のアクチュエータの語彙、`standing_ticks`、制裁コスト、密度、係争の知覚、記憶、通信、憲章、先占の根拠）、H3 に足りないもの（環境変化に応じて改正を提案する立法者）を引き渡した。

製品側では、Phase 2〜4 のデモはすべて**1 つの世界**（と、その fork 1 本）の観察・巻き戻し・介入・分岐比較だった。実験（EXP-001〜004）はすべて CLI の `cz sweep`（Apple M1 で 1 実験 17〜20 分、単一スレッド）と Python の `summarize.py` で行い、ブラウザからは見えない。PRODUCT.md Phase 5 のデモ「複数シード・条件格子の結果分布を見て、介入効果を比較できる」は、この**実験の道具そのものを製品の機能にする**こと — ユーザーが公開 URL で条件格子 × シードを自分のブラウザで回し、分布と介入効果（同じ seed の対の差と CI）を見て、気になる世界を Toy 1 で開く — である。同時に、研究側の「1 世界の Verify = MATCH」を「実験の再現 = 同じ CSV が出る」に広げる。

Phase 4 の持ち越し（TODO の Phase 5 ブロック）: UI の主スレッドが律速（制度世界の 2 世界同期 435 tick/s・29 fps、目標 900）、曲線の履歴の長さ（fixed vs learning が tick 60,000 で 593 tick/s・40 fps）、wasm の門 ≤ 400 KB の残り 0.7%（次に境界を広げるときは serde_json を境界から外す）。

## 完了条件

1. **ブラウザのデモ（PRODUCT.md Phase 5 = 実験ダッシュボード）**: 公開 URL で、(a) 事前登録した実験の設計（条件格子 × シード × tick 数 × 窓）を選び、**ブラウザ内の Worker プール**で回せる（進捗と中断が見える。設計の一部 — セルとシードの範囲 — をその場で絞れる）、(b) セルごとに指標（生存数、E / E_L、S / S′、加害死の割合、摂取/体·tick、D4 のコホート指標）の**シードごとの分布**（点と中央値・四分位）が並ぶ、(c) 対比（同じ seed の 2 腕の差: 介入あり / なし）の**対の差と 95% ブートストラップ CI** が描かれる、(d) 分布の点（セル, seed）を選ぶとその世界を **Toy 1 で開ける**（同じ config・seed・介入の予定から決定論的に建て直し、巻き戻し・介入・fork・Verify がそのまま使える。ヘッダに「実験の run と同じ tick で hash が一致」の印）、(e) 結果を **`cz sweep` と同じ CSV**（summary / series / qpolicy）とマニフェストでダウンロードでき、(f) **Replicate**（同じセル・seed をもう 1 度回して hash と CSV 行が一致）が MATCH になる。
2. **計測**（Apple M1、機種・コミット・日付付きで STATUS に。詳細は D11）: **1 世界の門は Phase 3 / 4 のまま**（native ≥ 1,800 tick/s、ブラウザ ≥ 60 tick/s、decide ≤ 200 / 250 ns、Nomos ≤ step の 20%、スナップショット ≤ 1 / 2 MB、巻き戻し ≤ 200 ms、cz-web の wasm ≤ 400 KB）。**プールの門**: 7 Worker の合計 tick/s が 1 Worker の **≥ 4 倍**（制度世界）、参照設計 D-ref（D8 / D11: 既定の forager 世界と学習世界、ScaleFood 4 水準（偽薬込み）× 10 シード × 2,000 tick = 80 世界）が **≤ 15 s**、制度世界 20 世界 × 3,000 tick が **≤ 15 s**、実行中の主スレッド ≥ 30 fps。**ブラウザと CLI の一致**: 同じ設計の同じ（セル, seed）で `dynamics_hash` と CSV 行（`ticks_per_s` を除く全列）が一致。`cz-lab-web` の wasm ≤ 400 KB。**Phase 4 の未達分**: 制度世界の「Compare: with vs without rule」15 tick/フレームで **≥ 60 fps（900 tick/s）**、fixed vs learning は tick 60,000 でも **≥ 55 fps**。カーネルの追加分: 個体追跡 step ≤ +3%、適格度トレース（K = 8）step ≤ +20%、目撃に基づく係争の decide ≤ 250。
3. **事前登録実験**: EXP-005（H4、計測器の較正）、EXP-006（H1 再訪: 除去と学習の分離、信用割当）、EXP-007（H2 の格子）、EXP-008（H3 の改正あり / なし）を「仕様コミット → 実行 → 記録」の順で、各腕 ≥ 10 シード、主対比 30 シード、帰無モデル付き（D8）。H2 / H3 の判定文の成否がどちらでも記録する。結果の記述に「創発」「世界初」「所有権が生まれた」を使わず、Enforcer と台本の立法者を含む条件は「陽性対照」と書く。
4. **決定論**: ゴールデン 3 本（Forager `0x658ee873d1866501`、Learner 32 行 `0xcb8bc0a9772c09a4`、制度世界 `0xfedc281503180c8e`）は 5-1〜5-13 を通して不変。新しい状態（記憶の種類、トレース、投資の tick）はすべて config で有効化し、**「あるときだけ符号化」**（既定 OFF では 1 バイトも足さない）。ゴールデン 3 本は `cz lab`（native）・`cz-lab-web`（wasm）・`cz-web` の `from_config`（wasm）の 3 経路で同じ値に達することを CI が照合する。EXP-006 で選んだ学習者の変種を preset にするなら**第 4 のゴールデン**を置く。再生（スナップショット + 台帳）は新しい介入の予定（schedule）を挟んでも bit 一致。
5. **台帳と CSV の契約**: `cz sweep` の既存の列は名前・順序・意味とも不動で、新しい列は末尾（`ticks_per_s` の前）に足す。EXP-003 / EXP-004 の CSV は `cz-lab` に移した後も再実行で `ticks_per_s` 以外の全列が一致（P0 の回帰検査）。個体単位の追跡は `TickReport` の材料（状態でも台帳でもない）から走行時に集計し、イベントの種類は増やさない。
6. **原則**: 走行・集計・対比の統計・文の生成は純関数で LLM は使わない（結果の要約もテンプレート）。ダッシュボードの見出しは指標名（`E`, `S′`, `alive` …）だけで「所有権」の語を使わない。制度名の行動を作らない（R1）。

## 決定（設計）

### D1. 問いの順序: 計測器を先に検証し、除去と学習を分けてから、格子と改正を問う

Phase 5 の実験は 4 本で、番号と順序を固定する（TODO の「EXP-005 の候補」(1)〜(7) の採否は下の表）:

| 実験 | 問い | 依存 | 主な場所 |
|---|---|---|---|
| **EXP-005**（H4） | fork + 介入 + 同じ seed の対で**推定が既知の効果を回復する**か: 偽薬（介入なしの fork）が厳密に 0、用量反応（`ScaleFood`）が単調、n シードの CI の被覆率、分散の 1/n 収束、対にしない推定との幅の比 | 5-1（`cz-lab`） | CLI（200 シード）+ ブラウザ（部分集合、CSV の一致が完了条件 1 (e)） |
| **EXP-006**（H1 再訪） | 信用割当（D5）を入れた学習者は**従うことを学ぶ**か。個体単位の追跡（D4）で除去と学習を分ける | 5-2, 5-3 | CLI（主対比 30 シード） |
| **EXP-007**（H2） | 制度の**成立確率と安定性（半減期）**のパラメータ依存。格子 + 1 軸ずつの腕 + 帰無 | 5-4, 5-7（スモーク格子のデモは 5-9） | CLI（格子）+ ブラウザ（スモーク格子をデモに） |
| **EXP-008**（H3） | 改正できる社会は固定制度の社会より**環境ショック**から速く回復するか（手続きの実験、D7） | 5-5（A / F のデモは 5-9） | CLI + ブラウザ（A / F の対比をデモに） |

理由: (1) ダッシュボードが出す「対の差と CI」を信じてよいかは、答えが分かっている合成実験でしか確かめられない。EXP-005 はそれを事前登録し、同時にブラウザと CLI の CSV の一致（完了条件 1 (e) / 2）を検査する最初の設計になる。(2) H2 の「制度成立」を EXP-004 のまま E_L の差で定義すると、成立確率は「Enforcer が十分殺したか」の確率になる（ADR-0014 の帰結）。EXP-006 で除去と学習を分ける指標を先に作り、学習が出る変種があればそれを H2 の基底セルにし、無ければ H2 は「除去の動態の格子」として記録する。(3) H3 は改正の手続きそのものを問う実験で、立法者が台本（D7）である限り「発明」は問えない。それを先に書いておく。

TODO の候補の採否: (1) 議会で成立した規則で EXP-004 と同じ指標 → **EXP-007 の軸「施行の経路」**（布告 tick 0 / 布告 tick 2,000 / 議会）、(2) 個体単位の追跡 → **D4 / EXP-006**、(3) 信用割当を入れた学習者で H1 → **D5 / EXP-006**、(4) H2 の格子 → **D6 / EXP-007**、(5) 布告の tick → EXP-007 の軸、(6) 憲章の最小票数 → EXP-008 の憲章の軸に入れず **先送り**（D7 の決定どおり「生存 ≥ 50 の議会」の読みで足りている）、(7) 行 9 の γ / `UPPER` / 失敗行動のコスト → γ だけ **D5 の config**（EXP-006 の腕）、`UPPER` と失敗行動のコストは先送り（却下した案）。

### D2. 実験の実行基盤（native）: `cz-lab` クレート、`cz sweep` の委譲、seed 単位の並列

- **`crates/cz-lab`**（ライブラリ、`simulation-core` と `multiverse` にだけ依存。cz-cli と cz-lab-web が読む）に置くもの:
  - **`Design`**（serde、JSON）: `format: 1`、`name`、`base`（`policy` ミックス名、`preset`、`width / height / agents / patches`、`config` の上書き = `WorldConfig` の部分集合）、`ticks`、`window_pct`、`sample`、`seeds { start, count }`、`cells: [{ name, config の上書き, policy, schedule: [{ tick, intervention }] }]`（セルは基底 + 上書き + **介入の予定**。EXP-004 の `--enact … --enact-at` も、EXP-005 の fork + `ScaleFood` も、EXP-007 の tick 10,000 の `Repeal` も、同じ seed の走行に予定を挟むだけで表す）、`contrasts: [{ name, treat, control, metric, window }]`（同じ seed の対の差を取る腕の組）。設計は人が書く JSON で、EXP の仕様コミットに `design.json` として入る。
  - **走行** `run_cell(design, cell, seed, sink)`: `World` を建て（`cz run` の `build_world` と同じ規則を `cz-lab` に移す）、`ticks` 進め、予定の tick に `intervene` し、`sample` ごとに series 行を、`window_pct` の窓で summary 行を、学習者がいれば qpolicy 行を `sink` に書く。**`Tally` と列の定義は cz-cli から移す**（列は不動。完了条件 5）。同じ seed の「fork」は予定の違う走行で表し、スナップショットは持たない（決定論で等価。`Multiverse` で接頭辞を共有する最適化は要るまでしない）。
  - **対比の統計** `paired_stats(rows, contrast)`: `summarize.py` の対の平均・95% ブートストラップ CI（10,000 回、乱数はカーネルの `RngStream` と同じ SplitMix 系のカウンタ型で seed 20261018 を固定）・d_z・比・向きのシード数を Rust で。native と wasm で bit 一致し、ダッシュボードと `cz lab summarize` が同じ数字を出す。EXP-004 の `summarize.py` との照合は平均・d_z は一致、CI の端点は乱数列が違うのでモンテカルロ誤差（±1e-4 程度）の範囲で一致（受け入れ基準に書く）。
  - **コホートの集計**（D4）と **結果のマニフェスト** `RUN.json`（`lab_result: 1`、設計、カーネルのコミット、機種、開始 / 終了、世界ごとの `(cell, seed, ticks, dynamics_hash, ms)`）。
- **`cz sweep` は `cz-lab` の薄い呼び出し**になる（フラグ → `Design` の 1 セル）。`cz lab run DESIGN.json --out DIR [--jobs N] [--cells a,b] [--seeds S..E]` と `cz lab summarize DIR --design DESIGN.json`（セルごとの分布、対比の統計、コホート指標を JSON と Markdown で）を新設。**EXP ごとの予測の判定表（P0〜Pn）は従来どおり各 EXP の `summarize.py`（stdlib）**が担い、対比の数字は `cz lab summarize` と一致することを検査する（2 実装の相互検査）。
- **並列は seed 単位**（`--jobs N`、`rayon`、`parallel` feature）: 各 seed は独立した `World` なので結果はスレッド数に依らず、CSV は seed 順に並べ替えて書く。EXP-004 の 19 分 38 秒は M1 の 4 性能コアで 6〜8 分の見込み（計測して記録）。`RUN.log` 相当の記録（コミット、バイナリの sha256、load、壁時計）は `RUN.json` と `cz lab run` の標準出力に残す。
- **回帰検査**: (a) `tests/` に「EXP-004 `le` seed 0 の最初の 2,000 tick の series 行が `data/EXP-004/le_series.csv` の先頭 21 行と一致」「同 `l0`」「ゴールデン 3 本を `Design` で建てて 200 tick の `dynamics_hash` が一致」を置く（CI、数秒）。(b) 1 回限りの手動検査として EXP-003 の `forager`、EXP-004 の `l1` と `n_rand`（合計 ≈ 2 分）を再実行し `ticks_per_s` 以外の全列を `cmp` で照合して STATUS に記録。

**実装と設計からの差分（5-1、2026-10-18）**:

- **依存**: `cz-lab` は `simulation-core`（と serde / serde_json）にだけ依存する。`multiverse` は `run_cell` がスナップショットも fork も持たない（同じ seed の「fork」= 予定の違う走行、上の決定どおり）ので足していない。要るときに足す。
- **並列は `rayon` ではなく `std::thread::scope`**（cz-cli の `lab.rs`: 共有の原子カウンタから (セル, seed) を取る N 本のスコープ付きスレッド。feature 不要、依存なし）。各世界は独立なので結果はスレッド数に依らず、CSV は (セル, seed) 順に並べて書く。`rayon` はカーネルの `parallel` feature（decide の並列）のまま。`cz sweep` も `--jobs N` を受ける（既定 1 = 従来どおりの順でコンソールに出る）。
- **`Design` の形**（`crates/cz-lab/src/design.rs` のモジュール文書が正）: 上の決定に加えて、セルに **`preset` / `policy` / `seeds` / `sample` の上書き**（EXP-004 の 10 シードの腕、`--preset nomos` の L0 / L1 / N-rand、N-rand の `--sample 10` を 1 つの設計で記録するため）、設計に `notes`（自由文）。`config` の上書きは **`WorldConfig` の JSON をキーごとに重ねる**（`perception` / `charter` などの入れ子はフィールド単位、`null` で `Option` を消す、未知のキーと `seed` は拒否、結果は `WorldConfig::validate`）: 型付きの部分構造体ではないので、5-3〜5-5 で `WorldConfig` に足すフィールドはそのまま上書きできる。優先順位は preset → 方策ミックスの含意（`institution` は行数と share、`reflex` / `mixed-learning` は突然変異）→ `base.width / height / agents / patches` → `base.config` → `cell.config` → seed（= `cz sweep` の「明示したフラグだけが上書き」の規則）。対比の `window` は `(from, to]`（`summarize.py` の `span_rows`）、`metric` は `ratio`（series の Σ/Σ、分母は複数列の和も可）/ `mean` / `per_tick` / `summary`（summary の列）/ `summary_ratio` の 5 種。
- **`run_cell(design, cell, seed, options, sink)`**: `RunOptions { check }`（`cz sweep --check`）が加わり、戻り値 `CellRun` がコンソール行の材料と `dynamics_hash / state_hash / ms` を運ぶ。`Sink` は trait（`Rows` が集める実装。ブラウザの `rows()` は同じ型）。wasm32 では壁時計を持たないので `ticks_per_s` / `ms` は 0（5-8 で Worker が測る）。
- **`paired_stats` の乱数**は `BootRng`（`mix64(key ^ mix64(counter))` = `RngStream` と同じ算術、鍵は `mix64(20261018)`）。`summarize.py` との照合（`tests/stats.rs`、10 対比）: 平均・d_z・比・向きのシード数は一致（相対 1e-9 以内）、CI の端点は LE − LE0（E_L 後半）で 1.2e-6 / 6.5e-6、LE − LE0（E_L 序盤）で 2.0e-5 / 5.7e-5、10 シードの対比では最大 3.4e-4（区間幅の 4% を許容 = 分位点のモンテカルロ誤差の約 5σ）、Learner 数の対比では 0.7 / 2.4 体（幅 97 体）。受け入れ基準の「±1e-4」は E_L 尺度の主対比で成立し、他は区間幅に比例する。
- **`goldens.json`** は `crates/cz-lab/goldens.json`（3 セル: forager / learner / institution、seed 42、200 tick。`institution` は tick 100 の布告を予定に持つ）。期待値はテスト側（`tests/goldens.rs`）に置き、5-8 の `cz-lab-web` は同じファイルを `include_str!` する。
- **回帰検査**: (a) CI は `le` / `l0` seed 0 の 2,000 tick の series 21 行（debug で 8.6 s）とゴールデン 3 本、列名の文字列（summary / series / qpolicy と、EXP-001〜004 の登録 CSV のヘッダが今日の列の接頭辞であること）、`cz sweep` ≡ `cz lab run`（`--jobs 1` ≡ `--jobs 2`、cz-cli の `tests/lab_cli.rs`）、`cz run --policy institution … --ticks 200` の `state_hash` `0x4b945b8bade819ee`（4-9）。(b) 手動は EXP-003 `forager`（`--cells forager`）、EXP-004 の**全 11 腕**を `--jobs 4` で 1 回（l1 / n_rand を含む。登録 CSV は単一スレッドの `cz sweep` の出力なので `--jobs 4` ≡ `--jobs 1` の検査を兼ねる）、l1 の `--jobs 1`。結果は STATUS の Phase 5 表。
- **`RUN.json`**（`Manifest`）: `lab_result`、`design`、`selection { cells, seeds }`、`environment { commit, worktree_modified, machine, os, cores, memory_gb, rustc（build.rs が記録）, binary_sha256, load_at_start / end }`（すべて best effort の `Option`）、`jobs`、`check`、`started / finished`（ISO 8601 UTC、依存なしの civil-from-days）、`wall_ms`、`worlds[] { cell, seed, ticks, dynamics_hash（"0x…" の文字列）, state_hash, ms, final_alive, extinct_at }`。
- **`cz lab summarize`** はセルごとに `final_alive / win_mean / win_consumed_per_agent_tick / win_encroach_rate(_learner) / win_s / win_s_prime / 加害死の割合 / win_harms_per_tick / win_alive_learner_mean / win_alive_enforcer_mean` と対比の指標の分布（n・平均・sd・最小・四分位・最大、生存シードのみ）、対比ごとの `paired_stats` を `SUMMARY.json` / `SUMMARY.md` に書く。EXP ごとの予測の判定表は従来どおり `summarize.py`。
- **コホート（D4）の枠**は `cz_lab::cohort`（`CohortTally`: id を添字にした `Vec`、出生・死亡・制裁を `TickReport.events` から読む。採集・侵害は `gatherers / encroachers`（5-2）待ち）。summary の列は 5-2 で一度に足すので `run_cell` はまだ駆動しない。

### D3. 実験の実行基盤（ブラウザ）: 第 2 の wasm `cz-lab-web`、Worker プール、進捗と中断、結果の保存、CLI との分担

- **第 2 の wasm パッケージ `crates/cz-lab-web`**（wasm-bindgen。`cz-lab` + カーネル。公開 API は `LabRun::new(design_json, cell, seed)`、`advance(ticks) -> done`、`rows() -> { summary, series, qpolicy, cohort }`、`dynamics_hash_hex()`、`config_bytes()`（そのセル・seed の `WorldConfig` を postcard で）、`paired_stats(rows_json, contrast_json)`）。Toy 1 の `cz-web`（397,333 B、門 ≤ 400 KB の残り 0.7%）には走行・集計・設計の構文解析を**足さない**。理由: 1 パッケージにすると `Design` / `WorldConfig` の serde_json と `Tally` の分で門を超え、「serde_json を境界から外す」（≈ −150 KB の見込み、プロトコル v8 と読み手 3 本の書き直し）を先にやる必要が出て、Phase 5 の UI 予算（D9）を食う。ダッシュボードの Worker だけが `cz-lab-web` を読み、Toy 1 のページの読み込みは不変。両パッケージは同じクレート・同じプロファイル（ADR-0014 D12: カーネルだけ `opt-level 3`、残り `z`、`wasm-opt -Oz`）で建て、`npm run build:wasm` が 2 回 wasm-pack を呼ぶ。門は**それぞれ ≤ 400 KB**。
- **Toy 1 側に足すのは `WasmWorld::from_config(config_bytes, policy, snapshot_every)` だけ**（postcard の `WorldConfig`: スナップショットの経路に既にある serde impl を使い、serde_json は増えない）。ダッシュボードから「この世界を開く」は、`cz-lab-web` の `config_bytes()` と設計の予定（schedule）を Toy 1 の kernel worker に渡し（プロトコル v8: `WorldInit.config` と `WorldInit.schedule`。worker は予定の tick で `step` を区切って `intervene` する）、同じ世界を建て直す。
- **Worker プール**（`web/src/lab/pool.ts`）: Worker 数 = `min(navigator.hardwareConcurrency − 1, 8)`（最低 1）。ジョブ = (セル, seed) をセル順・seed 昇順に積み、空いた Worker が取る。完了順は任意だが結果は (セル, seed) で引くので出力は順序に依らない。**進捗**は Worker が `advance(chunk)`（chunk = 500 tick）ごとに返す tick 数の合計 / 総 tick 数。**中断**は Worker の `terminate()` と作り直し（wasm の instantiate は数十 ms）。途中の世界は捨てる（再実行で等価。部分結果は保存しない）。
- **結果** `Result`（`web/src/lab/result.ts`）: `RUN.json` と同じマニフェスト + CSV 行の配列（文字列のまま。ブラウザと CLI で同じ行）。メモリに持ち、**IndexedDB に直近 5 件**（設計名 + 日時で）保存して再読み込みで消えないようにし、ダウンロードは `cz sweep` と同じファイル名の CSV と `RUN.json` を zip に（≤ 50 MB の見込み: EXP-004 の `le_series.csv` が 30 シードで 1.07 MB）。ブラウザのマニフェストの機種は `hardwareConcurrency` と `userAgent`、コミットはフッタの `built from commit`。
- **ブラウザと CLI の分担**（役割で分ける。速さではない）: ブラウザ = 探索（軸と水準を絞って回す）、**登録済みセルの再現**（誰でも公開 URL で EXP-005 / 007 / 008 のセルを回して CSV と hash が記録と一致することを確かめられる = 実験の Verify）、デモ。CLI = **記録となる実行**（事前登録した設計の全セル・全シード・20,000 tick を `RUN.json` 付きで、`data/EXP-00n/` にコミット）。理由: 20 分級の走行はタブの背景化（rAF 2 Hz・Worker の背景 QoS、STATUS 4-9 の記録）・熱・メモリの影響を受け、時間の記録として再現しない（内容は決定論で同じ）。ダッシュボードは走らせる前に見込み時間（D11 の ms/tick の表から）を出し、**既定の予算 = 2,000,000 世界·tick**（≈ 100 世界 × 20,000 tick。M1 で 2〜4 分の見込み）を超える設計は確認を求める。Worker 数の上限は 8。
- **一致の契約**: 同じ設計の同じ（セル, seed）について、`cz lab run`・`cz-lab-web`・Toy 1 の `from_config` の 3 経路で `dynamics_hash` が一致し、CSV 行は `ticks_per_s`（壁時計）以外の全列が文字列として一致する。浮動小数点の列は Rust の `Display` の最短往復表記で、native と wasm で同じ。走行時間は列ではなくマニフェストに置く。

### D4. 個体単位の追跡（除去と学習の分離）

- **カーネル**: `TickReport` に `gatherers: Vec<AgentId>`（成功した採集 1 件につき採集者。Apply Phase 3 のセル順・採集者 id 順）と `encroachers: Vec<(AgentId, AgentId)>`（侵害 1 件につき（採集者, 先占者）。`Violated` と同じ順）を足す。状態でもハッシュでもイベントでもなく、記録の設定も要らない（`Violated` の常時記録は 4-1 で却下済み: 21 件/tick。こちらは tick ごとに消費される材料で、1 tick あたり u32 を ≈ 160 + 20 個積むだけ。step の増分 ≤ 3% を門にして計測）。制裁の対象と死因は常時記録の `Sanctioned { by, target }` と `Died { id, cause }` から読める。
- **走行時の集計**（`cz-lab`、カーネルの外）: 個体 id は単調で再利用しないので `Vec<AgentTally>`（id で添字、`next_agent_id` の長さ）に、個体ごとの `policy / born / died(tick, cause) / gathers / encroachments / sanctions_received / sanctions_given` を窓ごとに積む（HashMap を使わない。走査は id 順で決定論的）。**コホート指標**（summary の末尾、`ticks_per_s` の前に追加。すべて窓 W について）:
  - `win_cohort_learners`: W の開始時に生きていた学習者数。`win_cohort_survived`: うち W の終了時も生きていた数。`win_cohort_died_violence`: うち W の中で加害死した数。
  - **`win_e_learner_survivors`** = 生存コホートの Σ 侵害 / Σ 採集（**学習の指標**: 生き残った個体の侵害率）。**`win_e_learner_removed`** = W の中で加害死した学習者の、死ぬまでの Σ 侵害 / Σ 採集。**`win_encroach_share_removed`** = W の学習者の侵害のうち、W の中で加害死した個体によるものの割合（**除去の指標**）。
  - `win_e_survivors_first_half` / `win_e_survivors_second_half`: 生存コホートの W の前半 / 後半の侵害率（**同じ個体の中の変化** = 学習の時間構造。個体ごとの対の差の平均も `win_e_survivors_delta_mean`）。
  - `win_sanctioned_learners` / `win_sanctioned_then_died`: W で制裁を受けた学習者数と、うち最後の制裁から `standing_ticks` 以内に加害死した数（制裁 → 死の経路の強さ）。
- `cz lab run --agents` で短い走行（≤ 5,000 tick）の個体表 `<腕>_agents.csv`（1 個体 1 行: 上の個体ごとの値）を書く。20,000 tick × 30 シードでは 1 腕 ≈ 60 MB になるので登録データにはコホート列だけを入れ、個体表はダッシュボードの「この世界を開く」と EXP-006 の短い走行で読む。
- **読み方**（EXP-006 / 007 の事前登録に使う形）: 規則の効果 Δ = E_L(R) − E_L(R0)（同じ seed）を、生存コホートの差 Δ_surv = E_surv(R) − E_surv(R0) と、除去の寄与（`win_encroach_share_removed` と `win_cohort_died_violence` の差）に分けて読む。「学習」の判定文は「Δ_surv が SESOI（0.01）以上に負で、CI が 0 を含まず、かつ生存コホートの前半 → 後半の変化が負」、「除去」は「Δ_surv が SESOI 未満で `win_encroach_share_removed` が R で R0 より大きい」。EXP-004 の LE はコホート列を持たないので、EXP-006 の C0 腕（LE の再実行）がその値を初めて出す。

### D5. 信用割当: `ENCROACHED_RECENTLY` ビットと行マスク、適格度トレース、γ

EXP-004 の読み 3: 制裁の痛み（−40 報酬単位）は侵害の採集から数 tick〜数十 tick 遅れて別の行の別の行動に帰属し、1 段の TD と γ = 29/32（地平線 ≈ 10 tick）では侵害の採集まで戻らない。3 つの経路をすべて **config で有効化する整数の機構**として入れ、EXP-006 が腕にする（どれも既定 OFF で、ゴールデン 3 本と EXP-002〜004 の軌跡は不変）:

- **(a) 自分が侵害したことの記憶と状態ビット**。`WorldConfig.remember_encroaching: bool`（既定 false）が真なら、侵害の判定（Apply Phase 3 直後）で**採集者**の `recent` に `InteractionKind::Encroached { other: 先占者, value: 量 }`（`InteractionKind` の末尾、`kind as u64` = 14。先占者の `EncroachedBy` と対）を書く。既定 false なので制度世界のゴールデン（`nomos` オン、`recent` は非空ならハッシュに入る）は不変。Learner の細かい状態に **`ENCROACHED_RECENTLY`（ビット 10**: 16 tick 以内に `Encroached` がある。`SANCTIONED_RECENTLY` と同じ地平線）を足し、`FINE_STATES` を 2048 にする。**既存のビット 0〜9 の位置は動かさず**、既定の折り畳み（下位 `log2(rows)` ビット）では `MAX_LEARNER_STATES` 1024 の範囲に入らないので、既存の世界の行は変わらない。Reflex には特徴 21 `ENCROACHED_RECENTLY`（表 168 → 176、初期重み零、突然変異は既存の 168 個が先。`nomos` オフでは常に 0 なので X1 / X1-50k の series は不変。表全体がハッシュに入るので Reflex 世界のハッシュは変わる = 4-3 と同じ扱い、ゴールデンではない）。
- **行マスク** `WorldConfig.learner_row_mask: u16`（既定 0 = 従来の折り畳み）。非零なら、行 = 細かい状態からマスクのビットを**詰めた値**（pext）`% learner_states`。tick の開始時に 2048 要素の `Vec<u16>`（派生、保存もハッシュもしない）を 1 回作って引く（decide に加わるのは 1 読み）。EXP-006 の「ビットあり」の腕は 128 行のまま `{HUNGRY, HAS_FOOD, FOOD_HERE, FOOD_IN_SIGHT, ADJACENT, OCCUPIED, ENCROACHED_RECENTLY}` を選び、**`VIOLATOR_ADJACENT` を外す**（EXP-004 の読み 2: 行 64〜127 は訓練の薄い同点の行で、学習者の Harm を 1.45 倍にした人工物。従う学習に要るのは `OCCUPIED` と自分の侵害の記憶で、係争者の隣接ではない）。256 行にして両方を持つ腕は Q 表 4 KB/学習者で制度世界のスナップショットが門（2 MB）に当たるので主腕にしない（副次腕として 10 シードまで）。`_qpolicy.csv` の行のビット列はマスクに従って出す（既存の CSV は不変）。
- **(b) 適格度トレース**。`WorldConfig.learner_trace_len: u8`（既定 0 = OFF、上限 16）と `learner_trace_decay_32nds: u8`（既定 28 = 0.875）。`Agent.trace: Vec<(state: u16, action: u8)>`（カーネル所有、`last_step` の兄弟、oldest-first、**非空のときだけ符号化**、`SetPolicy` と出生で空、継承しない）に直近 K 件の (行, 行動) を持ち、Apply 5a の更新で TD 誤差 δ を (s, a) に α で入れた後、古い順に k = 1..K の (s_k, a_k) にも `δ · decay^k` を同じ α で足す（**置換トレース**: 同じ (s, a) が複数あれば最新の 1 回だけ。i64 で計算し i16 に飽和。decay^k は 1/32 単位の累乗を逐次掛ける）。コスト: 学習者 1 体につき K 回の読み書き（K = 8、1,000 体で ≈ +30〜50 µs/tick の見込み = step の +10〜20%）。**門は K = 8 で step ≤ +20%**（計測して超えれば K の既定を下げるか、|δ| が閾値以上のときだけ遡る変種にする — 変種の採用は EXP-006 の仕様コミット前の計測で決め、ADR に追記する）。
- **(c) γ** を config に: `WorldConfig.learner_gamma_32nds: u8`（既定 29 = 今日の `29 · max >> 5` と同一の算術。ゴールデン不変）。EXP-006 の腕は 31（地平線 ≈ 32 tick）。
- スナップショット形式は **14**（config のフィールド追加と `Agent.trace`。13 を拒否）。`check_invariants`: `trace` は Learner だけ・長さ ≤ K・行と行動の範囲、マスクのビットは `FINE_STATES` の範囲、`learner_row_mask` のビット数 ≥ `log2(learner_states)` でなければ `Invalid`。
- **期待される読み**（EXP-006 で棄却可能な形に）: (a) は 1 段の TD でも「侵害した後の状態」を別の行に置くので、制裁の痛みがその行群の価値を下げ、`OCCUPIED` の行の Gather の価値が max Q′ 経由で下がる。(b) は痛みを直接 K tick 遡らせる。(c) は地平線を伸ばすが空腹帯（≈ 100 tick）には届かない。どれも「従う」方向に動かなければ、Phase 5 の H2 は除去の格子として記録する（D1）。

### D6. H2 の格子の軸をカーネルに: Harm の語彙、目撃に基づく係争、先占の根拠

HYPOTHESES の引き渡しのうち「要実装」の 3 軸。いずれも既定値が今日の挙動で、既定ではバイトを足さない:

- **Harm のアクチュエータの語彙** `WorldConfig.harm_target: HarmTarget { StandingFirst（既定）, Nearest, Random }`。`StandingFirst` = 4-3 のとおり係争中の隣人を先に、`Nearest` = 3-6 の最寄り（規則を読まない語彙）、`Random` = 隣接個体から一様に 1 体（`Decide` ストリームで 1 回引く。選ばれたときだけ乱数を消費）。Reflex / Learner のアクチュエータだけが読む（Enforcer の制裁（規則 1）は語彙ではなく台本なので不変）。R1 の「語彙が指標を動かす」を格子の軸にする（EXP-004 の読み 2 / 8）。
- **目撃に基づく係争** `Perception.standing: StandingKnowledge { Public（既定）, Witnessed }`。`Witnessed` なら、規範の問い合わせ（`my_standing / accused / adjacent_violator / violator_adjacent / rule`）は NormIndex のフラグに加えて**観測者自身の記憶**を要件にする: 個体 X が係争中だと「知る」のは、観測者の `recent` に X を名指しする `EncroachedBy { other: X }`、`SawSanction`（value = 被制裁者 X）、`SanctionedBy { other: X }` のいずれかが 16 tick 以内にあるときだけ（自分の係争は `SanctionedBy` か `Encroached` があるときだけ）。規則の有無（`rule(kind)`）は公知のまま（布告・可決は世界の出来事）。**`see_standing` と違い、台本の `Enforcer` にも効く**（R12 の「情報の問題」を Enforcer の制裁にも課さなければ軸にならない）。`violator_adjacent()` の件数の近道（4-9a）は `Witnessed` では使えず走査に戻るので、decide の門 ≤ 250 ns を制度世界で計測する。NormIndex の構築は不変。
- **先占の根拠** `WorldConfig.occupancy: Occupancy { Arrival（既定）, Invested }`。`Invested` なら `Agent.invested: Option<u64>`（今のセルで最初に採集または貯蔵に成功した tick。移動で `None`、出生は親を継承 = 家族は共同、`Some` のときだけ符号化）を持ち、先占者 = 同セルの生存個体のうち `invested` が `Some` で最小（同点は `arrived`、次に id）。誰も投資していないセルの採集は侵害ではない。`occupied()` も同じ定義で読む。R13 の「投資を含まない到着順」の変種。
- CLI: `--harm-target standing|nearest|random`、`--standing public|witnessed`、`--occupancy arrival|invested`。スナップショット形式 **15**。テスト: 既定で 3 つとも 4-x の挙動と bit 一致（ゴールデン 3 本、`nomos` オンの random 32² のゴールデン `0x1e2a77fc40cf7156` も不変）、`Nearest` は 3-6 の `nearest_adjacent` と同一決定、`Witnessed` で記憶のない観測者は規則下でも盲目と同一決定、`Invested` の真偽表（投資前 / 後 / 移動 / 家族）、proptest で serial = parallel = 再実行・往復。

### D7. 台本の適応的立法者（H3 の手続きの実験）。学習方策の立法は先送り

H3 の実験に足りないのは「環境変化に応じて改正を提案する立法者」（HYPOTHESES）。Phase 5 では**台本**で置く（= 陽性対照。R14 の続き）:

- `WorldConfig.enforcer_amends: bool`（既定 false）と `legislator_permille: u16`（既定 32）。真なら `Enforcer` の台本（ADR-0014 D7 の規則 (1)〜(6)）に規則 (3′) と (4′) を足す:
  - **(3′) 改正の動議**: 議場が空いていて `Encroach` の規則 R が施行中、自分が `hungry` かつ視界に食料がない（Forager の走査の結果 = **局所の欠乏**）とき、R の `sanction` が HARM なら `Motion(Enact(Encroach { R.standing_ticks, REJECT }))`（**緩和**: 制裁を加害から拒否に置換 = 欠乏下で制裁が集団を削るのを止める）、R の `sanction` が REJECT で自分が `hungry` でなく視界に食料があれば `Motion(Enact(Encroach { R.standing_ticks, HARM }))`（**復元**）。`FloorTaken` の試行コスト（`motion_cost` は失敗でも払う、D7）を抑えるため、条件を満たした tick に `legislator_permille / 1000` の確率で試行する（`Decide` ストリームで 1 回引く。既定 32‰ で 100 体の Enforcer から 1 tick ≈ 3 試行）。規則 (3)（規則がないときの先占規範の動議）はそのまま。
  - **(4′) 投票**: 開いている動議が `Enact(Encroach { _, sanction })` の置換なら、**自分が今 (3′) で提案するであろう sanction と同じなら賛成、違えば反対**（欠乏を見ている Enforcer が閾値を超えたときだけ緩和が通る = 議会が環境を集計する）。他の動議は規則 (4) のとおり。
  - 読むのは `Observation`（視界の走査結果、自分の空腹、`rule(kind)`、`open_motion`）だけで、世界全体の食料は読まない（方策の純関数性と知覚制限を保つ）。乱数は `Decide` ストリーム（(3′) の試行の抽選）。既定 false では分岐 1 つで決定も乱数消費も不変（制度世界のゴールデン不変。`tests/nomos_motions.rs` の Enforcer = Forager の 600 tick 一致はそのまま）。
- **固定制度の社会** = 同じ fork に `Intervention::Charter { amendable: false }`（4-5 実装済み。布告は憲章に縛られないので実験者は後から直せる）。**帰無の立法者** = `Random` を 10% 混ぜた議会（動議の種類は一様、D7）と、(3′) の条件を無視して一様に緩和 / 復元を動議する `legislator_random`（config のフラグ 1 つ: 台本の中の分岐）。
- **学習方策の立法**（`Motion / Vote` を Reflex / Learner のアクチュエータに入れる、投票が報酬に返る経路）は **Phase 6 へ先送り**（却下した案）: 行動の種類が 8 → 10 になり表の形と Learner のゴールデンが変わること、制度の効果が自分の報酬に返るまで数百 tick の遅れがあり D5 の信用割当がそのまま前提になること、無料の投票が同点の雑音になること（EXP-002 P5f）の 3 つが Phase 5 の予算に収まらない。Phase 5 の H3 が答えるのは「改正の**手続き**（憲章の定足数・閾値・期限・改正の可否）があると、設計された改正が環境変化に追随し、固定制度より速く回復する条件があるか」まで。
- スナップショット形式 **16**（config のフィールド追加）。CLI: `--enforcer-amends`、`--legislator PERMILLE`、`--legislator-random`。

### D8. 事前登録実験の形（数値は各 EXP 文書で固定。ここでは形だけ）

共通の規則（EXP-004 と同じ）: 仕様コミット → 実行 → 記録。腕どうしは同じ seed の対、95% ブートストラップ CI、d_z、向きのシード数、SESOI（E_L は 0.01）。10 シードの対比は検査、主対比は 30 シード。絶滅したシードは対から除いて数を報告。禁止語なし。Enforcer・台本の立法者を含む条件は「陽性対照」。各実験は `design.json` を持ち、`cz lab run` で走り、`summarize.py`（stdlib）が予測の判定表を出す。

**EXP-005（H4、計測器の較正。5-6）**

- 世界: 既定の forager 世界と `?policy=learner` の学習世界（`WorldConfig::default()`。制度はなし）。seed 0〜199、2,000 tick。tick 1,000 に `ScaleFood { permille }` ∈ {500, 750, **1,000（偽薬）**, 1,250} を予定（= tick 1,000 の fork + 介入）。腕は世界 2 × 水準 4。
- 指標: 窓 (1,500, 2,000] の `win_mean`（生存数）、`win_consumed_per_agent_tick`、`win_food_standing_mean`。対比 = 各水準 − 偽薬（同じ seed）。
- 予測の形: (1) **偽薬の差は全 seed で厳密に 0** で `dynamics_hash` が介入なしの走行と一致（決定論の検査。1 seed でも違えば基盤の欠陥）。(2) 用量反応: 500 < 750 < 1,000 < 1,250 の順に `win_mean` の対の平均が単調（200 シードの CI が重ならない）。(3) **被覆率**: 200 シードの対の平均を「真値」とし、n ∈ {5, 10, 30} の部分集合を 1,000 回（固定 seed）引いて、95% ブートストラップ CI が真値を含む割合が **≥ 0.90**。(4) 対の差の分散が 1/n で縮む（log-log の傾きが −1 ± 0.2）。(5) **対にしない推定**（seed を入れ替えて組む）の CI の幅が対の **≥ 2 倍**（同じ seed の対の利得の記録）。(6) 擬似介入（`nomos` オンの世界で `Charter` の布告 = 物理に触れない）の差は厳密に 0。
- ブラウザ: 水準 4 × seed 0〜9 × 2 世界の 80 世界を**ダッシュボードで回し、CLI の 200 シードのうち同じ seed の行と CSV が一致**（完了条件 1 (e)、D3 の契約）。これが Phase 5 のデモの既定の設計（D-ref）。

**EXP-006（H1 再訪: 除去と学習の分離、信用割当。5-7）**

- 基底: EXP-004 の LE / LE0（`institution_preset`、`enforcer_motions` false、`enforcer_share` 100、R = `Encroach { 32, HARM }` を tick 0 に布告）。
- 腕（各腕に規則あり R と規則なし R0 の対。変種は規則がなくても学習を変えるので対照は変種ごと）: **C0** = LE / LE0（EXP-004 の再実行 = P0 の回帰検査とコホート列の初値）、**C1** = C0 + `remember_encroaching` + 行マスク（128 行、`VIOLATOR_ADJACENT` を外し `ENCROACHED_RECENTLY` を入れる）、**C2** = C0 + トレース（K = 8、decay 28/32）、**C3** = C1 + C2、**C4** = C0 + γ 31/32、副次 **C5** = C1 の 256 行（両ビット、10 シード）。主対比 C1 / C3 とその対照は 30 シード、他は 10。帰無: N-blind（C3 + `--blind`: 規範ビットを見ない学習者でも (a)(b) の経路が効くか）、N-nosanction（C3 + 制裁なし: 痛みなしでは何も変わらない）。
- 指標: EXP-004 の M1〜M12 に D4 のコホート指標を足す。主指標は **Δ_surv**（生存コホートの侵害率の対の差）と `win_encroach_share_removed`、副次に行 9 の貪欲 Rest のシード数（γ の腕）。
- 予測の形: C0 は EXP-004 と同値（Δ −0.067、Δ_surv は SESOI 未満 = 除去）。C1 / C3 で Δ_surv ≤ −0.01（CI が 0 を含まない）かつ生存コホートの前半 → 後半が負なら「従うことを学ぶ条件がある」、そうでなければ「この 3 経路では学ばない」。C2 単独は K = 8 では届かない（痛みは数十 tick 遅れる）、C4 は行 9 を動かさない（地平線 32 < 空腹帯 100）。コスト（加害死、谷、Learner 数）は腕ごとに記録。
- **決定規則（事前登録）**: Δ_surv の対の平均が最も負で CI が 0 を含まず、絶滅 0/30 で、step の増分が門の内側の腕の学習者設定を **`WorldConfig::compliance_preset()`**（名前は仮）として H2 の基底にし、第 4 のゴールデンを置く。該当がなければ H2 の基底は C0（LE）のままとし、H2 の「成立」は除去の動態として記録する。

**EXP-007（H2 の格子。5-11）**

- 基底セル: EXP-006 の決定規則の学習者設定 × `institution_preset`、R を布告 tick 0。
- **成立の定義**（seed ごと）: 後半の窓で Δ = E_L(R) − E_L(R0) ≤ −0.01（SESOI）。**成立確率** = セル内で成立した seed の割合（Wilson 区間）。連続版として Δ の対の平均と CI も必ず併記（10 シードの二値は粗い）。**除去 / 学習の分解**を D4 のコホート列で全セルに。
- **安定性（半減期）**: 基底セルと `standing_ticks` の軸で、tick 10,000 に `Repeal` を予定した第 3 の腕 R-rep を同じ seed に加え、廃止後 100 tick の窓ごとの E_L が R0 の同じ窓の値との差の半分を回復するまでの tick 数（打ち切りは走行の終端 10,000 tick）。従うことが学ばれていれば長く、除去なら個体数の回復（EXP-004: 1,000 体まで 1,500〜17,900 tick）と同じ時間尺度になる。
- **主格子**（全組み合わせ、10 シード、各セルに R / R0）: `standing_ticks` ∈ {8, 32, 128} × `harm_cost` ∈ {2,000, 8,000} × 密度（tick 0 の `ScaleFood` ∈ {500‰, 1,000‰}）= 12 セル。**1 軸ずつ**（基底セルから、10 シード、R / R0）: Harm の語彙 {Nearest, Random}、係争の知覚 {Witnessed}、`memory_slots` {0, 32}、`signal_radius` {0, 4}、先占の根拠 {Invested}、施行の経路 {布告 tick 2,000, 議会（`enforcer_motions` true、布告なし）}、憲章 {定足数 300‰}（議会の経路でだけ意味がある）。帰無: N-rand と N-blind を基底で。合計 ≈ 12 × 2 + 10 × 2 + 2 + 半減期の腕 ≈ 480 走行 × 20,000 tick（M1 単一スレッドで ≈ 70 分、`--jobs 4` で ≈ 20 分の見込み）。
- 予測の形（棄却可能。向きだけ）: 成立確率と |Δ| は `standing_ticks` に単調増（HARMABLE の時間が長いほど除去が進む）、`harm_cost` に単調減（学習者の制裁が減る）、密度 500‰ で縮む（E は共在の率）。`Nearest / Random` では学習者の制裁が消えて N-blind と同じ水準（語彙の帰結の確認）。`Witnessed` では Enforcer の制裁が半分以下になり |Δ| が縮む（R12: 情報の問題を課すと制度は弱くなる）。`memory_slots` 0 は S′ を上げ |Δ| を少し広げる（EXP-004 P7）。`signal_radius` は SESOI 未満（v1 の制裁は通信を使わない = R12 の記録、EXP-004 P6）。`Invested` は E₀ 自体を下げる（先占者が減る）。議会の経路は布告 tick 51 相当で後半の Δ は布告 tick 0 と同じ、布告 tick 2,000 は谷が浅く後半の Δ は同じ。半減期は基底で個体数の回復の時間尺度（除去）か、それより長い（学習、EXP-006 の決定規則で学習者を選べたときだけ）。
- H2 の判定文: 「成立確率（または Δ）が軸 X に対して傾き ≠ 0（傾きの CI が 0 を含まない）である軸がある / ない」。全軸で傾きの CI が 0 を含めば棄却条件（HYPOTHESES）に該当すると記録する。

**EXP-008（H3、改正あり / なし。5-12）**

- 世界: `institution_preset` + 議会（`enforcer_motions` true）+ `enforcer_amends` true、布告なし（規則は tick ≈ 51 に可決。製品の制度世界と同じ）。tick 5,000（谷の後、定常帯域）に**環境ショック** S = `Drought { amount, interval }`（再生量を既定の 1/4、以後ずっと）を予定。seed 0〜29。
- 腕（同じ seed の同じ接頭辞から。すべて tick 5,000 の予定で分岐）: **A**（改正可、既定の憲章）、**F**（tick 5,000 に `Charter { amendable: false }`）、**F\***（固定だが緩和後の規則 `Encroach { 32, REJECT }` を tick 5,000 に布告してから固定 = 設計された改正の上界参照）、**N**（制度のない世界: `enforcer_motions` false で規則なし。接頭辞が違うので対にしない参照）、帰無 **A-rand**（`legislator_random`）と **A-R**（Random 10% の議会）、手続きの軸 **A-q300**（定足数 300‰）、**A-v128**（`vote_ticks` 128）（10 シード）。
- 指標: **回復時間** T_rec = ショック後、100 tick の窓の生存数がショック前 1,000 tick の平均の 90% に初めて戻るまでの tick 数（打ち切り = 終端 15,000 tick）、ショック後の最小生存数、ショック後 2,000 tick の加害死の割合、`win_consumed_per_agent_tick`、可決した改正の tick と内容（timeline）。対比 A − F（30 シード）、F\* − A、A-rand − F、A-q300 − A、A-v128 − A。
- 予測の形: T_rec(A) < T_rec(F)（CI が 0 を含まない）**かつ** A で緩和の動議が最小生存数の tick より前に可決したシードでだけ差が出る（可決の有無で層別）。F\* ≤ A（手続きの遅れの分）。A-rand と A-R は F と同程度かそれより悪い（偶然の改正は役に立たない）。定足数 300‰ と `vote_ticks` 128 は可決を遅らせ T_rec(A) を F に近づける（手続きの費用）。
- H3 の判定文: 「ショック S の下で、改正できる社会 A は固定制度の社会 F より回復時間が X 短い（30 シード、CI）。差は設計された緩和が可決した seed に限られ、立法者は台本（陽性対照）である」。差の CI が 0 を含めば「この設計では改正の手続きは回復を速めなかった」と記録。

### D9. UI: 実験ダッシュボードと Toy 1 への導線、Phase 4 の UI の課題

- **ダッシュボードは別のページ状態**（`?lab=1`、`web/src/lab/Lab.tsx` 以下。`main.tsx` が URL で `App`（Toy 1）と `Lab` を切り替える）。Toy 1 の `App.tsx` には導線（下）以外を足さない。部品: **Design**（同梱の設計 = EXP-005 の D-ref、EXP-007 のスモーク格子（基底セルと `standing_ticks` の軸 × 10 シード × 3,000 tick）、EXP-008 の A / F（10 シード × ショック後 3,000 tick）。セル・シード範囲・tick 数をその場で絞れる。見込み時間と予算の表示）、**Run**（開始 / 中断、Worker ごとの進捗、合計 tick/s）、**Cells**（指標を 1 つ選び、セル × seed の点と中央値・四分位のストリップ。クリックで Toy 1 へ）、**Contrasts**（設計の対比ごとに seed の対の差の点、平均と 95% CI の帯。`cz-lab-web` の `paired_stats` の値そのまま）、**Export**（zip）、**Replicate**（1 セル 1 seed を回し直して hash と行を照合、MATCH / MISMATCH）。文はテンプレート（`web/src/lab/text.ts`: 「cell c · seed s · hash … · alive … · E …」）。判定と統計は Rust、ページは描くだけ（3-9b / 4-7 と同じ原則）。
- **Toy 1 への導線**: `?lab=<result>&cell=<name>&seed=<n>` で開かれた Toy 1 は、`WorldInit.config`（postcard）と `schedule` で世界を建て、予定の介入を tick どおりに適用しながら進め、ヘッダに「lab · cell c · seed s · hash @tick T = run ✓」（結果の `dynamics_hash` と一致したら ✓）を出す。以後は普通の Toy 1（巻き戻し・介入・fork・Verify）。
- **Phase 4 の UI の課題**（Toy 1 側、5-10）: (1) **制度カード 4 枚の再描画**: 読み（timeline / violations / sanctions の 3 要求）を 500 ms ごとから **A の tick が 100 進むごと**（と seek 完了・布告・停止の直後）に変え、要求が返ったときだけ `memo` で再描画する。率のカーブは `Frame.ledger` の差分だけから描くので要求を増やさない。(2) **曲線の履歴**: 1 フレーム 1 点の配列を、固定長（1,200 点）の**ビン**（各ビンは区間の min / max / last）に累積し、見た tick 数に依らず O(1,200) で描く（Compare の 5 カーブ、Norms の 6 カーブ、スパークライン共通）。(3) 計測は `web/scripts/rates-sampler.js` の有効な秒で（STATUS 4-9 の手順）。目標は完了条件 2 のとおり（制度世界の 2 世界同期 15 tick/フレームで ≥ 60 fps = 900 tick/s、fixed vs learning は tick 60,000 で ≥ 55 fps）。届かなければ Performance タブの切り分けを STATUS に残し、未達として記録する。
- **serde_json を境界から外す**案は **Phase 5 では実施しない**（D3 の理由: 第 2 パッケージで Toy 1 の門に触れない）。発動条件を残す: `cz-web` に次に読み手を足して公開ビルドが 400 KB を超えたとき、その読み手を足す前に 3-9a / 3-9b / 4-7 の直列化を構造体と typed array に置き換える（プロトコル v9）。
- プロトコル **v8**（5-8 で定義、5-9 / 5-10 は消費するだけ）: `WorldInit.config?: Uint8Array`（postcard の `WorldConfig`。`policy` はミックス名のまま）、`WorldInit.schedule?: { tick, intervention }[]`、`Frame.lab?: { cell, seed, expectedHash?: string }`。kernel worker は `step(n)` を予定の tick で区切り、`intervene` を挟む（台帳に `Intervened` が載るので再生・Verify はそのまま）。モック worker は `config` を無視し `schedule` を空で受ける。

### D10. 決定論・再現性の契約と形式の版

- **ハッシュの規約**（ADR-0014 D9 の続き）: `Agent.trace` は非空のときだけ、`Agent.invested` は `Some` のときだけ、`InteractionKind::Encroached` は末尾（`kind as u64` = 14）で `remember_encroaching` の世界でだけ書かれる。config の新フィールド（`remember_encroaching / learner_row_mask / learner_trace_len / learner_trace_decay_32nds / learner_gamma_32nds / harm_target / perception.standing / occupancy / enforcer_amends / legislator_permille / legislator_random`）は `dynamics_hash` に入らない。`TickReport.gatherers / encroachers` は状態ではない。行マスクの派生表と NormIndex は保存もハッシュもしない。
- **ゴールデン**: 3 本不変（完了条件 4）。EXP-006 の決定規則で `compliance_preset()` を置くなら、`World::generate(seed 42, compliance_preset + institution の割当)` を 100 tick → 布告 → tick 200 の `dynamics_hash` を第 4 のゴールデンとして `tests/determinism.rs`・`cz-web/tests/{native,wasm}.rs`・`cz-lab-web/tests/wasm.rs` に固定する。ゴールデン 3〜4 本は `cz lab` の設計 `goldens.json` としても走り、3 経路（native / cz-lab-web / cz-web の `from_config`）で同じ値（CI）。
- **形式の版**: スナップショット 13 → 14（5-3）→ 15（5-4）→ 16（5-5）。config を足すタスクごとに 1 回上げ、旧版は `Format(n)` で拒否（ブラウザに保存した旧形式のバイナリは読めない = Phase 4 と同じ扱い）。台帳の JSON Lines は形式 1 のまま（イベントの種類は増えない）。`Design` は `format: 1`、`Result` のマニフェストは `lab_result: 1`。CSV の列は完了条件 5 の契約（既存は不動、追加は末尾）。
- **再生**: 予定の介入は `World::intervene()` を tick の合間に呼ぶだけで、台帳の `Intervened` から再生される（ADR-0012 のまま）。`multiverse/tests/replay.rs` に「予定（`ScaleFood` → `Charter { amendable: false }` → `Repeal`）を挟む 600 tick の bit 一致」を足す。
- **実験の再現**: `Result` は設計・コミット・機種・世界ごとの hash を持ち、別の機械・別のブラウザで同じ設計を回せば hash と CSV 行（`ticks_per_s` 以外）が一致する。一致しなければ計測器の欠陥（EXP-005 (1) が最初の検査）。

### D11. 計測の門

| 項目 | 門 | 根拠・測り方 |
|---|---|---|
| 1 世界（native / ブラウザ / decide / Nomos / NormIndex / スナップショット / 巻き戻し / cz-web の wasm） | Phase 3 / 4 のまま（ADR-0014 完了条件 2） | 既存のベンチと `rates-sampler.js` |
| 個体追跡（5-2） | step ≤ +3%（制度世界、`step/institution/1000`） | `TickReport` の 2 本の `Vec` |
| 適格度トレース（5-3） | K = 8 で step ≤ +20%（`step/institution_trace/1000` を新設）、decide 不変 | 超えれば D5 (b) の変種 |
| 目撃に基づく係争（5-4） | decide ≤ 250 ns/体·tick（`decide/institution_witnessed/3000` を新設） | 走査が戻る分 |
| 適応的立法者（5-5） | decide ≤ 250（`decide/institution`）、既定 false で不変 | 分岐 1 つ |
| `cz lab run --jobs 4`（5-1） | EXP-004 の 11 腕 170 シードが ≤ 10 分（M1）、CSV は `--jobs 1` と同一 | 記録 |
| `cz-lab-web` の wasm（5-8） | ≤ 400 KB（`cz-web` も ≤ 400 KB のまま） | D12 のプロファイル |
| プール（5-8 / 5-13） | 7 Worker の合計 ≥ 4 × 1 Worker の tick/s（制度世界 3,000 tick）。D-ref（forager + 学習世界、80 世界 × 2,000 tick）≤ 15 s、制度世界 20 世界 × 3,000 tick ≤ 15 s。実行中の主スレッド ≥ 30 fps | 1 Worker のブラウザ実測は forager 0.27 / 学習 0.59 / 制度 0.42〜0.47 ms/tick（STATUS 4-9）。M1 は 4 性能 + 4 効率コアなので 7 Worker で 4〜6 倍が見込み。背景化すると落ちるので前面で測る |
| ダッシュボード（5-9） | Replicate = MATCH、EXP-005 の D-ref が CLI の同じ seed の行と `cmp` 一致、コンソールエラーなし | 完了条件 1 |
| Toy 1 の課題（5-10） | 制度世界の 2 世界同期 15 tick/フレーム ≥ 60 fps（900 tick/s）、fixed vs learning tick 60,000 で ≥ 55 fps、巻き戻し ≤ 200 ms のまま | 完了条件 2 |
| 結果の保存 | zip ≤ 50 MB（予算の上限の設計で）、IndexedDB は直近 5 件 | 計測して記録 |

門を超えたら ADR-0013 D6 / ADR-0014 D6 と同じ手順（同じ run の対照が静かなことを確かめ、同じ target で再計測し、製品の門の内側なら改定か最適化かをこの ADR の「決定」として追記）。

## タスク分解

各タスクは別セッションで 1 つずつ進める。**ファイルの重なり**: カーネル 3 本（5-3 → 5-4 → 5-5）は `policy.rs` / `config.rs` / `world.rs` / `tests/` を共有するので**この順に逐次**（並行させない）。5-1 と 5-2 は `cz-cli/main.rs` と新クレートを共有するので 5-1 → 5-2 の順。UI（∥）は 5-8 がプロトコル v8 と `web/src/lab/` の骨格を置いた後、5-9（`web/src/lab/`）と 5-10（`App.tsx` / `Institution.tsx` / `render.ts` / `kernel.worker.ts`）は触るファイルが分かれるので並行できる（5-10 の導線は `App.tsx` だけ）。実験 4 本は依存が揃い次第。共有チェックアウトでは、変種・フィールドの追加（`InteractionKind::Encroached`、`TickReport` の 2 本、`WorldConfig` の各フィールド、`Perception.standing`）を先に小さくコミットして兄弟の `match` と構造体リテラルが壊れる時間を短くする（Phase 3 / 4 の教訓）。

| # | タスク | 依存 | 受け入れ基準 |
|---|---|---|---|
| 5-1 ✅ | **`cz-lab` クレート（D2）**: `Design`（JSON）、`build_world` と `Tally` と CSV の列の移管、`run_cell`、`paired_stats`（ブートストラップ、SplitMix 系の固定 seed）、`RUN.json`、`cz sweep` の委譲、`cz lab run --jobs / --cells / --seeds`、`cz lab summarize`、EXP-003 / 004 の `design.json`（記録用。`run.sh` は残す） | – | `cz sweep` の出力列が不動（テストで列名の文字列を固定）。CI: `le` / `l0` seed 0 の最初の 2,000 tick の series 行が `data/EXP-004/*_series.csv` の先頭行と一致、ゴールデン 3 本を `goldens.json` で再現。手動: EXP-003 `forager`、EXP-004 `l1` / `n_rand` を再実行し `ticks_per_s` 以外の全列が `cmp` 一致（STATUS に記録）。`--jobs 4` の CSV が `--jobs 1` と同一、EXP-004 の 11 腕が ≤ 10 分（計測）。`paired_stats` が EXP-004 の対比（LE − LE0 など）で `summarize.py` と平均・d_z 一致、CI 端点 ±1e-4。ゴールデン不変 |
| 5-2 | **個体単位の追跡（D4）**: `TickReport.gatherers / encroachers`、`cz-lab` のコホート集計と summary の新列、`cz lab run --agents` の個体表 | 5-1 | ゴールデン 3 本不変（状態不変）。手組みの世界でコホート列の値（生存 / 加害死 / 前半後半）が定義どおり、`Σ gatherers.len() == gathers`、`encroachers` が `Violated` と同順。`step/institution` の増分 ≤ 3%（計測）。EXP-004 の LE seed 0 を 2,000 tick 走らせた個体表が series の合計と一致 |
| 5-3 | **信用割当（D5）**: `remember_encroaching` + `InteractionKind::Encroached`、Learner ビット 10 と `learner_row_mask`（派生表）、Reflex 特徴 21、`learner_trace_len / learner_trace_decay_32nds` + `Agent.trace` + 5a の置換トレース、`learner_gamma_32nds`、CLI フラグ、スナップショット形式 14、ベンチ `step/institution_trace/1000` | 5-2（順序のみ） | ゴールデン 3 本不変（native・wasm）。既定 config で `nomos` オン / オフとも 32 / 128 / 256 行の Learner が 4-x と 1,000 tick 同一決定（マスク 0、K 0、γ 29）。EXP-002 の X1 / X1-50k の series が全行一致。単体: トレースの更新値（δ · decay^k の列、置換、飽和、`SetPolicy` で空）、マスクの詰め方（既定と等価な明示マスク、ビット数不足の拒否）、γ 31 の更新値。`remember_encroaching` の世界で採集者に `Encroached` が書かれ先占者の `EncroachedBy` と対、ビット 10 が 16 tick 立つ。proptest で serial = parallel = 再実行・往復。step の増分（K = 8）≤ +20%、decide 不変（計測） |
| 5-4 | **H2 の軸（D6）**: `harm_target`、`Perception.standing`（Witnessed）、`occupancy`（Invested）+ `Agent.invested`、CLI、スナップショット形式 15、ベンチ `decide/institution_witnessed/3000` | 5-3（順序のみ） | ゴールデン 3 本と random 32² の `0x1e2a77fc40cf7156` 不変。`Nearest` は 3-6 の `nearest_adjacent` と同一決定（手組み + 300 tick）、`Random` は隣接個体に一様（proptest）。`Witnessed`: 記憶のない観測者は規則下で盲目と同一決定、`SawSanction` の後だけ `adjacent_violator` が返る、Enforcer にも効く（記憶なしの Enforcer は制裁しない）。`Invested` の真偽表（投資前は侵害でない、投資後、移動で失効、家族の継承、`SpawnAgents`）。proptest で serial = parallel = 再実行・往復。decide（Witnessed、制度世界）≤ 250（計測） |
| 5-5 | **台本の適応的立法者（D7）**: `enforcer_amends`、`legislator_permille`、`legislator_random`、規則 (3′) / (4′)、CLI、スナップショット形式 16、`multiverse/tests/replay.rs` の予定を挟む bit 一致 | 5-4（順序のみ） | 既定 false で制度世界のゴールデン不変、Enforcer = Forager の 600 tick 一致そのまま。手組み: 欠乏（`hungry` ∧ 視界に食料なし）で緩和の動議、豊かで復元の動議、議場が開いていれば試行しない、試行確率の一様性（proptest）、投票は自分の提案と同じ sanction に賛成・違えば反対、`legislator_random` は一様。台本の世界で `Drought` の後に緩和が可決し、`Charter { amendable: false }` の fork では `NotAmendable` で可決しない（EXP-008 の A / F の形）。再生の bit 一致。decide ≤ 250 |
| 5-6 | **EXP-005（H4、D8）**: 仕様コミット（`design.json` + `summarize.py`）→ `cz lab run`（2 世界 × 4 水準 × 200 シード × 2,000 tick）→ 記録 | 5-1 | 仕様が実行より前。偽薬の差 0 と hash 一致（全 seed）、用量反応、被覆率、1/n、対の利得、擬似介入の 6 項目の当否と限界。`cz lab summarize` と `summarize.py` の対比が一致。禁止語なし。ブラウザでの D-ref の一致は 5-9 の受け入れ基準 |
| 5-7 | **EXP-006（H1 再訪、D8）**: 仕様コミット → 実行（C0〜C5 × R / R0、主対比 30 シード、帰無 2 本）→ 記録 → 決定規則の適用（`compliance_preset()` と第 4 のゴールデンを置く、または置かないことを記録） | 5-2, 5-3 | 仕様が実行より前。C0 が EXP-004 の LE / LE0 と `ticks_per_s` 以外の既存列で全行一致（P0）。Δ と Δ_surv と `win_encroach_share_removed` の表、予測の当否、限界、H1 再訪の判定文（学ぶ条件がある / ない）。決定規則の適用と preset の値（置くなら `config.rs` と `tests/preset.rs`、第 4 のゴールデンを 3 経路に）。禁止語なし |
| 5-8 ∥ | **`cz-lab-web` と Worker プール（D3）**: `crates/cz-lab-web`（`LabRun`、`paired_stats`、`config_bytes`）、`npm run build:wasm` の 2 パッケージ化、`WasmWorld::from_config`、プロトコル v8（`WorldInit.config / schedule`、`Frame.lab`）、kernel worker の予定の適用、`web/src/lab/pool.ts / result.ts`（進捗・中断・IndexedDB・zip）、モック worker | 5-1（`Design` の型） | `wasm-pack test --node` で `cz-lab-web` がゴールデン 3 本を `goldens.json` から再現し、`cz-web` の `from_config` も同じ値（CI の 3 経路）。`tests/wasm.rs`: `LabRun` の series 行が native の `run_cell` と文字列一致。wasm サイズ `cz-web` ≤ 400 KB / `cz-lab-web` ≤ 400 KB（計測）。プールの単体（ジョブの割当と結果の順序非依存、中断で Worker が作り直される）。`npm run build` OK |
| 5-9 ∥ | **実験ダッシュボード（D9）**: `web/src/lab/Lab.tsx` 以下（Design / Run / Cells / Contrasts / Export / Replicate）、同梱の設計 3 つ、文のテンプレート、TUTORIAL §12 | 5-8 | PRODUCT Phase 5 のデモ (a)〜(f) をローカルで確認。**EXP-005 の D-ref（80 世界）をダッシュボードで回し、CLI の `data/EXP-005/` の同じ seed の行と `ticks_per_s` 以外が `cmp` 一致**、Replicate = MATCH、中断 → 再開で同じ結果、zip の CSV が `cz sweep` と同じファイル名・列。コンソールエラーなし。公開 URL での確認は 5-13 |
| 5-10 ∥ | **Toy 1 の課題と導線（D9）**: 制度カードの tick 駆動の読みと memo 化、曲線の履歴のビン化（Compare / Norms / スパークライン）、`?lab=…&cell=…&seed=…` の起動と hash の印 | 5-8（導線のみ。性能の 2 件は依存なし） | `rates-sampler.js` の有効な秒で: 制度世界の 2 世界同期 15 tick/フレーム ≥ 60 fps（900 tick/s）、fixed vs learning tick 60,000 で ≥ 55 fps（届かなければ切り分けを STATUS に残し未達と記録）。カードの表示内容は 4-8 と同一（seek 後・布告後の即時更新を含む）。導線で開いた世界の hash が結果と一致し、Verify = MATCH。Forager / 学習 / 制度世界の単独の tick/s が 4-9 の値から落ちない |
| 5-11 | **EXP-007（H2、D8）**: 仕様コミット → `cz lab run --jobs 4`（主格子 12 セル + 1 軸 10 腕 + 帰無 2 + 半減期の腕）→ 記録。ダッシュボードのスモーク格子の設計を同梱 | 5-4, 5-7, 5-9 | 仕様が実行より前、各腕 10 シード。成立確率（Wilson）と Δ の CI、コホートの分解、半減期（打ち切り数）、軸ごとの傾きの CI、予測の当否、限界、H2 の判定文。ダッシュボードでスモーク格子（基底 + `standing_ticks` 軸 × 10 シード × 3,000 tick）を回して CLI の同じ seed の 3,000 tick までの series 行と一致。禁止語なし、Enforcer は「陽性対照」 |
| 5-12 | **EXP-008（H3、D8）**: 仕様コミット → 実行（A / F 30 シード、他 10）→ 記録。ダッシュボードの A / F の設計を同梱 | 5-5, 5-9 | 仕様が実行より前。T_rec の対の差と CI、可決の有無での層別、F\* / 帰無 / 手続きの軸、予測の当否、限界、H3 の判定文（「手続き」の実験で「発明」ではないことを明記）。ダッシュボードで A / F（10 シード）を回して CLI と一致 |
| 5-13 | **計測とゴールデン（D10 / D11）**: プールの合計 tick/s（1 / 4 / 7 Worker）、D-ref と制度世界 20 世界の時間、実行中の fps、メモリ、zip のサイズ、`cz-lab-web` のサイズ、Toy 1 の課題の最終計測、第 4 のゴールデン（5-7 で置いたなら）の CI、公開 URL での (a)〜(f) の確認 | 5-8, 5-9, 5-10（5-7） | D11 の表のすべての数値を STATUS に機種・コミット・日付付きで。超えた項目は D11 の手順で改定理由を書く。公開 URL（push 後）でダッシュボードの D-ref を回し、CLI の行と一致、Replicate = MATCH、導線の hash の印 |
| 5-14 | **文書**: この ADR の完了判定、STATUS の Phase 5 表・「状態の区別」・「次のタスク」、HYPOTHESES（H2 / H3 / H4 の状態、H1 再訪）、RISKS（R15 ブラウザ計測の変動と背景化、R16 台本の立法者 = 設計者知識）、ARCHITECTURE（`cz-lab` / `cz-lab-web`、プロトコル v8、形式 16、データモデルの `trace / invested`、Apply 5a のトレース）、CLAUDE.md（ゴールデンの本数）、TUTORIAL §12、README / PRODUCT / TODO | 全て | 新セッションが STATUS → ADR-0015 → EXP-005〜008 で Phase 5 の状態を再構築できる |

並行可能: 5-8 は 5-1 の `Design` が決まり次第、5-9 は 5-8 の `pool.ts` のモックで先行できる。5-10 の性能 2 件は依存がなく最初から並行できる（`App.tsx` を触るので 5-9 とは分けるが、5-9 は `web/src/lab/` と `main.tsx` だけ）。5-6（EXP-005）は 5-1 の直後に CLI で走らせられる。カーネル 3 本（5-3 / 5-4 / 5-5）は逐次。5-7 → 5-11 → 5-12 は実験の順。

## 却下した / 先送りした案

- **サーバー側で実験を回す**（API、ジョブキュー、結果の共有 URL）: PRODUCT.md「サーバーは共有・保存が必要になるまで置かない」と静的配信（GitHub Pages）に反し、「誰でも自分のブラウザで同じ CSV を出せる」という再現の形が消える。不採用。共有は zip のダウンロードと `data/EXP-00n/` のコミット。
- **LLM による結果の要約・解釈**: PRODUCT.md 原則 2（LLM は歴史の語りに限定）と完了条件 6。結果の文はテンプレート。年代記としての語りは Phase 7 以降の別の ADR で、数値を唯一の情報源にする条件付き。Phase 5 には登場しない。
- **学習方策の立法**（`Motion / Vote` をアクチュエータに、投票が報酬に返る経路）: D7 の理由で Phase 6 へ。Phase 5 の立法者は台本。
- **1 つの wasm に実験の走行を入れる**: 門 ≤ 400 KB を超え、serde_json を境界から外す作業（プロトコル v9、読み手 3 本）を先に要する。第 2 パッケージ（D3）。serde_json の除去は次に `cz-web` が門に当たるとき（D9）。
- **wasm スレッド（SharedArrayBuffer + rayon）**: COOP / COEP のヘッダが要り GitHub Pages では付けられない。Worker プールで足りる。
- **部分結果の保存（中断時）**: 再実行で等価（決定論）。保存すると結果の形が増える。中断は捨てる。
- **`Acted` 相当の全採集の台帳記録で個体追跡**: 220 MB / 2,000 tick（3-2）。`TickReport` の材料を走行時に集計する（D4）。
- **全要因の H2 格子**: 9 軸の全組み合わせは不可能。主格子 3 軸 + 1 軸ずつ（EXP-003 の格子と EXP-004 の腕の形）。
- **「安定性」を規則そのものの寿命（議会での廃止までの時間）で測る**: Random の投票は雑音で、Enforcer は廃止を動議しない。廃止後の E_L の半減期（D8）にする。
- **H3 のショックを config の変更で表す**（`regrow_interval` を介入で書き換える）: 介入は config を変えない（ADR-0010 2a-1、3-9c の決定）。`Drought` 介入で足りる。
- **256 行の学習者を EXP-006 の主腕にする**: 制度世界のスナップショットが 2 MB の門に当たる。行マスクで 128 行のまま必要なビットを選ぶ（D5）。副次腕で 10 シードまで。
- **`UPPER`（512 行）と失敗行動のコストで行 9 を消す**（R9）: 失敗行動のコストは物理の変更でゴールデンを変える。Phase 6 の学習方策の設計へ。γ だけ D5 で config にする。
- **憲章の最小票数**（D7 の小議会の可決）: ADR-0014 の決定（生存 ≥ 50 の議会の読み）で足りている。EXP-008 で小議会が問題になれば追記。
- **ダッシュボードの統計を TypeScript で実装**: 2 実装（CLI と UI）がずれる。Rust（`cz-lab`）1 本を両方から呼ぶ（D2 / D3）。Python の `summarize.py` は EXP ごとの判定表と相互検査のために残す。
- **実験の結果をリポジトリに自動コミット**: 記録はこれまでどおり人が `data/EXP-00n/` に置き、`RUN.json` をコミットに含める。

## リスク

- **ブラウザの計測の変動**: Worker プールの tick/s は背景化（rAF 2 Hz、背景 QoS）・熱・他タブで動き、時間の記録としては再現しない（内容は決定論で同じ）。記録となる実行は CLI（D3）。ブラウザの門（D11）は前面・他の負荷なしの条件で `rates-sampler.js` の手順で測り、条件を STATUS に書く。RISKS に R15 として記録（5-14）。
- **メモリ**: Worker 8 本 × （wasm のインスタンス + 制度世界 ≈ 1 MB の状態 + `Tally`）。IndexedDB の保存は直近 5 件に限る。`performance.memory` が読める環境で測り、読めなければ「未計測」と書く。
- **2 つの wasm パッケージの乖離**: 同じクレートから建てるが、プロファイルや feature の違いで速度や hash が違いうる。CI の 3 経路のゴールデン照合（完了条件 4）が検出する。
- **`cz-web` の門**: `from_config`（postcard）は小さいはずだが、397,333 B は残り 0.7%。5-8 で計測し、超えたら D9 の発動条件（serde_json を境界から外す）に従う。
- **適格度トレースのコスト**（step +10〜20% の見込み）と**目撃に基づく係争の decide**（走査が戻る）: 門（D11）を超えたら変種（|δ| の閾値、走査の打ち切り）を ADR に追記して決める。
- **信用割当は学習者の行動を広く変える**（Rest、採集の場所）: 変種ごとに規則なしの対照を置く（EXP-006）。学習が出ても「所有権」ではなく「侵害の後に痛みが来る世界で侵害を避けることを学んだ」と書く。
- **台本の立法者は設計者の知識**（R14 の続き）: EXP-008 の差は「設計された改正が手続きを通れば回復が速い」以上を意味しない。帰無の立法者（A-rand / A-R）と F\*（上界）を必ず並べ、RISKS に R16 として記録。
- **半減期と回復時間の打ち切り**: 走行の終端で回復しない seed は打ち切りとして数を報告し、平均には入れない（CI は打ち切りのない seed だけ）。
- **格子の規模と時間**: EXP-007 は ≈ 480 走行 × 20,000 tick。`--jobs 4` で ≈ 20 分の見込みだが、M1 の熱で伸びる。壁時計は `RUN.json` に記録し、tick/s は参考値。
- **CSV の列の契約**: `cz sweep` の列を 1 つでも動かすと EXP-003 / 004 の `summarize.py` が壊れる。列名の文字列をテストで固定（5-1）。
- **禁止語**: ダッシュボードの見出し・ツールチップ・テンプレートに「所有権」「創発」を入れない（5-9 の受け入れ基準に `grep`）。
- **共有チェックアウトでの並行開発**: カーネル 3 本は逐次、UI 2 本はファイルを分ける（タスク表の注記）。

## 帰結

- ワークスペースに **`cz-lab`**（native ライブラリ）と **`cz-lab-web`**（第 2 の wasm）が加わり、`cz sweep` は `cz-lab` の薄い呼び出しになる。`cz lab run / summarize` が実験の入口で、EXP-005 以降は `design.json` を持つ。公開ビルドは wasm 2 本（Toy 1 用と実験用）で、Toy 1 の読み込みは不変。
- カーネルの遷移関数は 4 か所で拡張される（D4 の `TickReport`、D5 の記憶の種類・ビット 10・行マスク・トレース・γ、D6 の 3 軸、D7 の規則 (3′) / (4′)）が、すべて config で有効化し既定ではバイトを足さない。スナップショット形式 13 → 14 → 15 → 16、`InteractionKind` 15 種、`FINE_STATES` 2048、Reflex の表 176。ゴールデン 3 本は不変で、EXP-006 の決定規則で第 4 のゴールデンが置かれうる。プロトコル v7 → v8。
- ARCHITECTURE の層構造に `cz-lab` / `cz-lab-web` と `web/src/lab/` が加わり、データモデルに `Agent.trace / invested`、Apply 5a にトレースの更新、Decide に行マスクの派生表が加わる（5-14 で反映）。
- 研究側: H4 は EXP-005 で計測器の較正として答え、H1 は EXP-006 で「従うことを学ぶ条件がある / ない」を初めて問え、H2 / H3 は EXP-007 / 008 で判定文の成否がどちらでも記録される。Phase 5 が終わっても、文脈の問いの後半（立法の学習 = 発明）は手付かずで、Phase 6（学習方策の立法、認知構造の進化）へ渡す。
- 製品側: 「文明を観察する → 巻き戻す → 介入する → 分岐を比較する」に「**条件格子 × シードで回し、分布と介入効果を見て、気になる世界に飛ぶ**」が加わる。同じ seed・同じ設計からは同じ CSV が出る（実験の Verify）。

## 完了判定

（5-14 で、ADR-0014 と同じ表 — 完了条件 1〜6 × 状態 / 根拠 — を書く。未達は未達として記録する。）
