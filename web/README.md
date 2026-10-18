# cz-web — Toy 1 の UI（ADR-0010 2b）

ブラウザ内で文明を観察 → 巻き戻し → 介入 → 分岐比較するための UI。
世界は **本物のカーネル**（`crates/cz-web` を wasm-bindgen で wasm 化、`src/worker/kernel.worker.ts`）が動かす。
`?mock=1` を付けると `src/worker/mock.worker.ts` のスタンドイン世界に切り替わる（wasm なしで UI を触るため。事実ではない）。

## 動かす

```bash
cd web && npm install && npm run build:wasm && npm run dev
```

`http://127.0.0.1:5173/` を開く。`npm run build` は wasm ビルド → 型検査（`tsc --noEmit`）→ Vite ビルド。
`build:wasm` には `wasm-pack`（`curl -sSf https://rustwasm.github.io/wasm-pack/installer/init.sh | sh`）と
`rustup target add wasm32-unknown-unknown` が要る。生成物 `src/wasm/` はコミットしない。

## 構成

| ファイル | 役割 |
|---|---|
| `src/protocol.ts` | UI ↔ ワーカーのメッセージ型（v6）。`Frame` は `source`（kernel / mock）、`hash`、食料グリッド（`Uint8Array`）、個体配列（`Int32Array`、1 体 9 フィールド = カーネルの trace の 8 + `flags`（bit0 係争中 / bit1 投票済み / bit2 HARMABLE / bit3 REJECTABLE、カーネルの NormIndex）。policy 列は `POLICY_KINDS` の添字 0..=3）、社会マーカー `social`（`Int32Array`、1 件 7 値: kind / tick / x / y / x2 / y2 / value）を Transferable で渡し、台帳の累計 `ledger` を載せる。要求に `watch { id, window }` を添えると `Frame.watched`（その個体の `Agent`、学習者なら Q 表と `last_step` 込み、と台帳の出来事）が付き、`watch.partner` を添えると 2 者間の交渉スレッド `Watched.thread`（`Thread` / `ThreadEntry` / `Deal`）も付く。`deals` 要求は窓内の全取引。介入には `set_policy { who, kind, params }`。`WorldInit.policy` = forager / mixed / random / reflex / learner / mixed-learning。制度（ADR-0014 D12、4-7）: `Frame.institution`（憲章・施行中の規則・動議・係争中の件数）、`Ledger` の 4-1 / 4-2 の 8 カウンタ、`AgentDetail.arrived / standing / voted`、介入 `enact / repeal / charter`、`WorldInit.nomos / recordViolations`、要求 `institution / timeline / violations / sanctions`。4-8 は `MARKER_SANCTION`（kernel worker が `events_at` の `sanctioned` から足す制裁マーカー）を v6 内に追加 |
| `src/policies.ts` | フレームの方策構成の集計（policy 列を数える）と `set_policy` の言葉、「Compare」が作る介入 |
| `src/nomos.ts` | 制度の言葉（ADR-0014 D12、4-8）: 規範の 4 パラメータ・規則・憲章・布告・時系列・違反（根拠）・制裁（答えた係争）・係争の文を決定的に生成する。「Compare: with vs without rule」が作る介入（規則が無ければ CLI 既定の `enact`、有れば `repeal`） |
| `src/Institution.tsx` | 制度エクスプローラの 4 カード（4-8）: **Institution · ledger**（状態の憲章・規則・係争中の件数 + 台帳から読み戻した布告の時系列）、**Violations · recent**（直近 100 tick の違反 = 記録 ON なら根拠付き、OFF なら係争中の一覧）、**Sanctions**（各制裁とそれが答えた係争）、**Norms · rates**（直近 100 tick の E・違反/tick・制裁/tick・S′・S・係争中のカーブと表、A 実線 / B 破線）。判定はすべて wasm 側の読み手 |
| `src/client.ts` | `WorldClient`: 1 インスタンス = 1 世界。`init / step / seek / frame` を Promise で返す。`kernel()` / `mock()` |
| `src/worker/kernel.worker.ts` | `WasmWorld`（`crates/cz-web`）を駆動。要求は到着順に直列処理。4-8: 制度のある世界では `events_at(tick − 1)` の `sanctioned` を `MARKER_SANCTION` として `Frame.social` に足す |
| `src/worker/mock.worker.ts` | スタンドイン世界（台帳も決定論の契約もハッシュもない） |
| `src/render.ts` | Canvas2D 描画（食料 = ImageData を拡大、個体 = 方策ごとの色の矩形、透明度 = エネルギー、社会マーカー = 移転の線と点 / 加害の線と ✕ / 発信の輪。`MarkerTrail` が直近 0.6 s のマーカーを薄く残す）、人口スパークライン、`History`（tick 順の標本）と `drawCurves`（A/B の人口・摂取量/体/tick・移転/tick・加害/tick・発信/tick。率は直近 `RATE_WINDOW` = 50 tick の台帳差分、1,200 点超は間引き描画）。4-8: `flags` の係争中の輪郭（HARMABLE は制裁色）、制裁の ✕（拒否は棒付きの輪）、`CurveSet` で曲線集合を一般化した `drawCurves`（`NORM_CURVES` = 100 tick 窓の E・違反/tick・制裁/tick・S′・S・係争中）、`valuesAt` |
| `src/App.tsx` | 画面: キャンバス、再生/速度/タイムライン/`Go`、ヘッダの世界選択（`?policy=` の 6 構成、変更で再読み込み。`?seed=` で世界のシード、既定 42）、統計（state hash、方策構成、実測 tick/s・fps・Worker ms/tick、最後の巻き戻し時間、台帳の移転・加害・発信の累計と直近 1 s の tick あたり件数）、人口 + 食料カーブ、「Compare · A solid, B dashed」の 5 カーブ、「Deals · ledger」（直近 2,000 tick の受諾された提案を kept / open / broken の判定付きで一覧、`read` でそのスレッドへ）、個体インスペクタ（本体、最後の発信、カーネルが書いた記憶 `recent` を tick 付きで、台帳の直近 100 tick の出来事、学習者なら **last step**（Q 表の行・行動・報酬）と **table**（その行の Q 値 8 個）、**Thread · with #id**: 相手を選ぶと 2 者間の台帳が「tick · 誰が → 誰に · 何を」で古い順に並び、各取引の判定が付く）。4-8: `?nomos=1` / `?violations=1`（ヘッダの **institutions** / **record violations** チェック、バッジ `institution · n rules · m accused`）、制度カードの読み（再生中 500 ms ごと・seek 後・布告後・停止時に `timeline / violations / sanctions` の 3 要求）、インスペクタの arrived / standing / voted、地図の凡例 |
| `src/Intervene.tsx` | 介入パネル（干ばつ・資源量スケール・個体追加・個体除去・**Set policy**: all / fraction / one agent × kind × default / keep）と、台帳から読み戻した介入履歴。head 以外では Apply 無効。「Fork here」で任意 tick から分岐。4-8: **Enact**（NormKind・standing ticks・sanction の harm / rejection・except_sanction）/ **Repeal**（施行中の規則から選ぶ）/ **Charter**（4 欄）。制度の無い世界では無効化 |
| `src/Branches.tsx` | **「Compare: fixed vs learning」**（現 tick で fork し B の全員を選んだ方策に `set_policy`。ADR-0013 D8）、A（main）と B（fork）の比較表（tick・方策構成・人口・食料・食料消費・移転・加害・発信・各ワーカーの ms/tick・ハッシュと差分）、「Verify: fork again」（同一スナップショット + 同一介入で作り直して同 tick のハッシュを照合）、Discard。4-8: **「Compare: with vs without rule」**（現 tick で fork し、選んだ種類の規則が無ければ `enact`、有れば `repeal`）、比較表の規則数・係争・侵害・違反・制裁 |

## プロトコルの約束

- すべての要求は現在の `Frame`（または `error`）で応答する。UI はワーカーから受け取っていない状態を持たない。
- `seek(tick)`: `frontier`（到達した最大 tick）を超えると前進、戻る場合はカーネル側の cursor（最寄りのスナップショットからの再実行）。
  cursor 位置からの `step` は自分の台帳と照合しながら同じ履歴を再実行し、head に追いつくと以後は head を延ばす。
- 介入は head でのみ受け付ける（`intervene`）。cursor 位置での介入 = 分岐（2b-6）。`history` は台帳の `Intervened` を返す。
- シークはドラッグ中に間引かれ（進行中は最新の要求だけ残す）、Worker の `seek` 所要時間がフレームに載る。
- 分岐（2b-6）: `snapshot` 要求で A の現在ビュー（その tick の開始状態）のバイナリスナップショットを受け取り、2 つ目のワーカーを `init({ snapshot })` で作って介入を適用する。以後は再生・シークを両方に送る（B は fork tick より前に戻れない）。
- 再現性の検証: 保持しておいたスナップショットから 3 つ目のワーカーを作り、同じ介入を適用して同じ tick まで進め、B とハッシュを照合する（MATCH / MISMATCH を表示）。
- 社会層（ADR-0013 3-9a）: `Frame.social` はビューが直前に実行した tick（tick − 1）の移転・加害（台帳から）と発信（各個体の `last_signal` から）をビューの個体位置に置いたもの。`Frame.ledger` はカーネルの累計（状態の一部なので巻き戻し・分岐に追随）。`WorldClient.watch({ id, window })` を設定すると以後のすべてのフレームに `watched`（`WasmWorld::agent_json` と `agent_events_json` の結果）が同梱され、UI は個体について何も覚えない。モックは `social` が常に空で、watched の記憶も空。
- 制度（ADR-0014 D12、4-7 / 4-8）: `WorldInit.nomos` で kernel worker は `WasmWorld::new(..., {"nomos":true,"record_violations":…})` = `WorldConfig::nomos_preset()` の世界を建てる。`Frame.institution` は毎フレーム状態から、`timeline / violations / sanctions` は一回限りの要求で台帳から（規則 id の復元と「制裁が答えた係争」の対応付けは `crates/cz-web/src/institution.rs`）。布告は `intervene({ op: "enact" | "repeal" | "charter", … })` で、制度の無い世界ではカーネルが拒否する。「Compare: with vs without rule」は fork + `enact` / `repeal` で、Verify の経路は 2b-6 と同じ。制裁のマーカーは kernel worker が `events_at(tick − 1)` の `sanctioned` から作る（ページ側の近似ではない）。モックは制度を持たない。
- 固定 vs 学習（ADR-0013 3-9c）: 「Compare: fixed vs learning」は `snapshot` → 2 つ目のワーカーを `init({ snapshot })` → `intervene({ op: "set_policy", who: "all", kind, params: "default" })` で、2b-6 の fork そのもの。Verify も同じ経路なので学習世界でも MATCH になる（Q 更新は整数演算）。カーブの材料は各フレームの `ledger` 累計（`food_consumed / transfers / harms / signals`）だけで、B の台帳はスナップショットに入って A の値から続く。`WorldInit.policy` の `reflex` / `mixed-learning` は `cz run` と同じく突然変異 ON（`cz-web::evolves`）。モックの reflex / learner は採集者と同じ動きで学ばない（スタンドイン）。
- 交渉スレッド（ADR-0013 3-9b）: `watch` に `partner` を添えると `watched.thread`（`WasmWorld::thread_json(id, partner, tick − threadWindow, tick)`）が付く。中身は 2 者を名指しする台帳イベントの台帳順の列と、各 `Accepted` を取引として判定した `deals`（kept / open / broken、不足側、決着 tick）。**判定は wasm 側の Rust（`crates/cz-web/src/thread.rs`）が行い、UI は語彙を並べるだけ**。スレッドは「ビューの tick で読んだ台帳」なので、巻き戻せば短くなり期限内の取引は open に戻る。`WorldClient.deals(from, to)` は窓内の全取引（一回限りの要求）。モックはどちらも空。

## 計測（Apple M1、アプリ内蔵ブラウザ、約 1,330 体、2026-10-17）

- 再生: 30 tick/フレームで 1,802 tick/s・60 fps（Worker 0.22 ms/tick）。4 tick/フレームで 240 tick/s。
- 巻き戻し: 249 tick 再実行（間隔 250 の最悪）45〜56 ms、スナップショット上 0 ms。
- 介入: UI で tick 300 に干ばつ → tick 600 / 900 の state hash が `cz run --drought-at 300 --drought-amount 0 --drought-interval 10` と一致。
- 分岐: fork 21 ms、2 世界の同期再生 1,800 tick/s・60 fps。tick 400 で A / B のハッシュが native（無介入 / 干ばつ）と一致、Verify = MATCH（90 ms）。
- 制度（4-9、2026-10-18、公開ビルドのプロファイル = カーネル `opt-level 3` / 残り `z` / `wasm-opt -Oz`、397,333 B）: `scripts/rates-sampler.js`（コンソールに貼る。rAF ≥ 57 Hz の秒だけ採る）で前面: forager 単独 30 tick/フレーム 1,800 tick/s・60 fps・0.27 ms/tick（`s` ビルド）、学習世界単独 900 tick/s・30 fps・0.59 ms/tick、制度世界単独（`?policy=institution`、tick 3,000 から）807 tick/s・27 fps・0.42 ms/tick、fixed vs learning 15 tick/フレーム 898 tick/s・60 fps（fork 直後。tick 60,000 では 593・40 fps）、制度世界の with vs without rule 435 tick/s・29 fps・A 0.47 ms/tick（主スレッドが律速）。巻き戻しの最悪（学習世界、249 tick 再実行）171〜177 ms、fork 51 ms（forager → learner）/ 94〜103 ms（制度世界 1.05 MB）。node（V8）: `scripts/bench-wasm.mjs` forager 0.206 / mixed-learning 0.393 ms/tick、`scripts/bench-seek.mjs` 学習世界 0.458 ms/tick・巻き戻し最悪 115 ms、制度世界（`?policy=institution`、tick 4,500、1,130 体）0.396 ms/tick・98 ms・スナップショット 1,203,213 B（STATUS）。
- 固定 vs 学習（3-9c、A forager ≈ 1,330 体 + B learner ≈ 1,600 体の同期再生、前面）: 15 tick/フレームで 899 tick/s・60 fps（B のワーカー 0.55〜0.57 ms/tick）、30 tick/フレームで 1,008〜1,043 tick/s・34〜35 fps（B が律速）。fork 18〜23 ms、Verify = MATCH（143 ms、tick 600）。A / B の tick 600 のハッシュが `cz run --seed 42 --ticks 600`（`--set-policy learner --set-policy-at 300`）と一致。学習世界単独（`?policy=learner`、約 1,570〜1,600 体）は 30 tick/フレームで 1,100〜1,248 tick/s・37〜42 fps・0.49〜0.59 ms/tick（STATUS）。

## 検証

- `cargo test -p cz-web`: ラッパーがカーネルと同一の状態・ハッシュを出す、seek → step が head に合流する、介入は head のみ、6 つの方策構成が `cz run --policy` と同じ割当（evolving mix の突然変異含む）で policy 列が 4 種を数える、`set_policy` の JSON が通り compare の fork が再現可能で `Multiverse::fork` と一致、Learner 世界のゴールデン、`social()` がカーネルの `TickReport` から組んだ期待値と一致（seek 後・fork 後も）、`agent_json` がカーネルの `Agent` そのもの、`agent_events_json` の絞り込みと半開区間。スレッド（`threads::*`）: 手組みの台帳で履行・不履行・拒否の紐付け・重なる取引の判定、Random 世界で `agent_events_json` との整合とビュー依存、seed 23 の #909 ↔ #363 の取引（TUTORIAL が名指しするもの）と seed 42 に順序どおりの取引が無いこと。
- `wasm-pack test --node crates/cz-web --release`: **wasm 上で native のゴールデンハッシュに到達する**（Forager・Learner・制度世界（`institution` ミックス、tick 100 の布告、tick 200。ADR-0014 4-9）の 3 本、CI）。

## 配信

`npm run build` の `dist/` は静的ファイルだけ（`base: "./"` なのでサブパスでも動く）。フッタにビルド元のコミットが出る。

- GitHub Pages: `.github/workflows/pages.yml`。Settings → Pages で Source を GitHub Actions にし、リポジトリ変数 `PAGES_ENABLED=true` を設定すると `main` への push で配信される。private リポジトリでは Pages 対応プランが要る。
- 配布: CI の `toy1-dist` アーティファクトを unzip し、任意の静的サーバーで配信する（`file://` では Worker が動かないブラウザがある）。
- ローカル確認: `python3 -m http.server 8766` を `dist` の親ディレクトリで実行し `/dist/` を開く。

## チュートリアル

右列の「Five-minute tour」カードが 6 ステップ（観察 → 巻き戻し → 介入 → 分岐 → Verify → 規則の布告）を世界の状態から自動判定する。本文は [docs/TUTORIAL.md](../docs/TUTORIAL.md)（Phase 3 の節 6〜8: マーカーと記憶、交渉スレッド、固定 vs 学習の比較。Phase 4 の節 10: 布告・違反・制裁・規則の有無の比較）。

## 次

Toy 1 は機能完了・配信中。Phase 3 の UI は 3-9a（社会マーカー・インスペクタ）、3-9b（交渉スレッド、`?seed=`）、3-9c（fixed vs learning の比較、学習世界の選択）まで入り、Phase 4 の制度エクスプローラ（4-7 の読み手 + 4-8 の画面）が入った。`institution` の方策構成（4-5、4-9 から Enforcer 10% を保つ `WorldConfig::institution_preset()`）も入った。残りは動議の表示の磨き込み、主スレッドの描画コスト（制度世界の 2 世界同期で 29 fps、長時間再生で fps が落ちる。4-9 の計測、docs/TODO.md）、B 側のインスペクタと介入履歴（任意）。
