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
| `src/protocol.ts` | UI ↔ ワーカーのメッセージ型（v5）。`Frame` は `source`（kernel / mock）、`hash`、食料グリッド（`Uint8Array`）、個体配列（`Int32Array`、1 体 8 フィールド = カーネルの trace と同じ並び。policy 列は `POLICY_KINDS` の添字 0..=3）、社会マーカー `social`（`Int32Array`、1 件 7 値: kind / tick / x / y / x2 / y2 / value）を Transferable で渡し、台帳の累計 `ledger` を載せる。要求に `watch { id, window }` を添えると `Frame.watched`（その個体の `Agent`、学習者なら Q 表と `last_step` 込み、と台帳の出来事）が付き、`watch.partner` を添えると 2 者間の交渉スレッド `Watched.thread`（`Thread` / `ThreadEntry` / `Deal`）も付く。`deals` 要求は窓内の全取引。介入には `set_policy { who, kind, params }`。`WorldInit.policy` = forager / mixed / random / reflex / learner / mixed-learning |
| `src/policies.ts` | フレームの方策構成の集計（policy 列を数える）と `set_policy` の言葉、「Compare」が作る介入 |
| `src/client.ts` | `WorldClient`: 1 インスタンス = 1 世界。`init / step / seek / frame` を Promise で返す。`kernel()` / `mock()` |
| `src/worker/kernel.worker.ts` | `WasmWorld`（`crates/cz-web`）を駆動。要求は到着順に直列処理 |
| `src/worker/mock.worker.ts` | スタンドイン世界（台帳も決定論の契約もハッシュもない） |
| `src/render.ts` | Canvas2D 描画（食料 = ImageData を拡大、個体 = 方策ごとの色の矩形、透明度 = エネルギー、社会マーカー = 移転の線と点 / 加害の線と ✕ / 発信の輪。`MarkerTrail` が直近 0.6 s のマーカーを薄く残す）、人口スパークライン、`History`（tick 順の標本）と `drawCurves`（A/B の人口・摂取量/体/tick・移転/tick・加害/tick・発信/tick。率は直近 `RATE_WINDOW` = 50 tick の台帳差分、1,200 点超は間引き描画） |
| `src/App.tsx` | 画面: キャンバス、再生/速度/タイムライン/`Go`、ヘッダの世界選択（`?policy=` の 6 構成、変更で再読み込み。`?seed=` で世界のシード、既定 42）、統計（state hash、方策構成、実測 tick/s・fps・Worker ms/tick、最後の巻き戻し時間、台帳の移転・加害・発信の累計と直近 1 s の tick あたり件数）、人口 + 食料カーブ、「Compare · A solid, B dashed」の 5 カーブ、「Deals · ledger」（直近 2,000 tick の受諾された提案を kept / open / broken の判定付きで一覧、`read` でそのスレッドへ）、個体インスペクタ（本体、最後の発信、カーネルが書いた記憶 `recent` を tick 付きで、台帳の直近 100 tick の出来事、学習者なら **last step**（Q 表の行・行動・報酬）と **table**（その行の Q 値 8 個）、**Thread · with #id**: 相手を選ぶと 2 者間の台帳が「tick · 誰が → 誰に · 何を」で古い順に並び、各取引の判定が付く） |
| `src/Intervene.tsx` | 介入パネル（干ばつ・資源量スケール・個体追加・個体除去・**Set policy**: all / fraction / one agent × kind × default / keep）と、台帳から読み戻した介入履歴。head 以外では Apply 無効。「Fork here」で任意 tick から分岐 |
| `src/Branches.tsx` | **「Compare: fixed vs learning」**（現 tick で fork し B の全員を選んだ方策に `set_policy`。ADR-0013 D8）、A（main）と B（fork）の比較表（tick・方策構成・人口・食料・食料消費・移転・加害・発信・各ワーカーの ms/tick・ハッシュと差分）、「Verify: fork again」（同一スナップショット + 同一介入で作り直して同 tick のハッシュを照合）、Discard |

## プロトコルの約束

- すべての要求は現在の `Frame`（または `error`）で応答する。UI はワーカーから受け取っていない状態を持たない。
- `seek(tick)`: `frontier`（到達した最大 tick）を超えると前進、戻る場合はカーネル側の cursor（最寄りのスナップショットからの再実行）。
  cursor 位置からの `step` は自分の台帳と照合しながら同じ履歴を再実行し、head に追いつくと以後は head を延ばす。
- 介入は head でのみ受け付ける（`intervene`）。cursor 位置での介入 = 分岐（2b-6）。`history` は台帳の `Intervened` を返す。
- シークはドラッグ中に間引かれ（進行中は最新の要求だけ残す）、Worker の `seek` 所要時間がフレームに載る。
- 分岐（2b-6）: `snapshot` 要求で A の現在ビュー（その tick の開始状態）のバイナリスナップショットを受け取り、2 つ目のワーカーを `init({ snapshot })` で作って介入を適用する。以後は再生・シークを両方に送る（B は fork tick より前に戻れない）。
- 再現性の検証: 保持しておいたスナップショットから 3 つ目のワーカーを作り、同じ介入を適用して同じ tick まで進め、B とハッシュを照合する（MATCH / MISMATCH を表示）。
- 社会層（ADR-0013 3-9a）: `Frame.social` はビューが直前に実行した tick（tick − 1）の移転・加害（台帳から）と発信（各個体の `last_signal` から）をビューの個体位置に置いたもの。`Frame.ledger` はカーネルの累計（状態の一部なので巻き戻し・分岐に追随）。`WorldClient.watch({ id, window })` を設定すると以後のすべてのフレームに `watched`（`WasmWorld::agent_json` と `agent_events_json` の結果）が同梱され、UI は個体について何も覚えない。モックは `social` が常に空で、watched の記憶も空。
- 固定 vs 学習（ADR-0013 3-9c）: 「Compare: fixed vs learning」は `snapshot` → 2 つ目のワーカーを `init({ snapshot })` → `intervene({ op: "set_policy", who: "all", kind, params: "default" })` で、2b-6 の fork そのもの。Verify も同じ経路なので学習世界でも MATCH になる（Q 更新は整数演算）。カーブの材料は各フレームの `ledger` 累計（`food_consumed / transfers / harms / signals`）だけで、B の台帳はスナップショットに入って A の値から続く。`WorldInit.policy` の `reflex` / `mixed-learning` は `cz run` と同じく突然変異 ON（`cz-web::evolves`）。モックの reflex / learner は採集者と同じ動きで学ばない（スタンドイン）。
- 交渉スレッド（ADR-0013 3-9b）: `watch` に `partner` を添えると `watched.thread`（`WasmWorld::thread_json(id, partner, tick − threadWindow, tick)`）が付く。中身は 2 者を名指しする台帳イベントの台帳順の列と、各 `Accepted` を取引として判定した `deals`（kept / open / broken、不足側、決着 tick）。**判定は wasm 側の Rust（`crates/cz-web/src/thread.rs`）が行い、UI は語彙を並べるだけ**。スレッドは「ビューの tick で読んだ台帳」なので、巻き戻せば短くなり期限内の取引は open に戻る。`WorldClient.deals(from, to)` は窓内の全取引（一回限りの要求）。モックはどちらも空。

## 計測（Apple M1、アプリ内蔵ブラウザ、約 1,330 体、2026-10-17）

- 再生: 30 tick/フレームで 1,802 tick/s・60 fps（Worker 0.22 ms/tick）。4 tick/フレームで 240 tick/s。
- 巻き戻し: 249 tick 再実行（間隔 250 の最悪）45〜56 ms、スナップショット上 0 ms。
- 介入: UI で tick 300 に干ばつ → tick 600 / 900 の state hash が `cz run --drought-at 300 --drought-amount 0 --drought-interval 10` と一致。
- 分岐: fork 21 ms、2 世界の同期再生 1,800 tick/s・60 fps。tick 400 で A / B のハッシュが native（無介入 / 干ばつ）と一致、Verify = MATCH（90 ms）。
- 固定 vs 学習（3-9c、A forager ≈ 1,330 体 + B learner ≈ 1,600 体の同期再生、前面）: 15 tick/フレームで 899 tick/s・60 fps（B のワーカー 0.55〜0.57 ms/tick）、30 tick/フレームで 1,008〜1,043 tick/s・34〜35 fps（B が律速）。fork 18〜23 ms、Verify = MATCH（143 ms、tick 600）。A / B の tick 600 のハッシュが `cz run --seed 42 --ticks 600`（`--set-policy learner --set-policy-at 300`）と一致。学習世界単独（`?policy=learner`、約 1,570〜1,600 体）は 30 tick/フレームで 1,100〜1,248 tick/s・37〜42 fps・0.49〜0.59 ms/tick（STATUS）。

## 検証

- `cargo test -p cz-web`: ラッパーがカーネルと同一の状態・ハッシュを出す、seek → step が head に合流する、介入は head のみ、6 つの方策構成が `cz run --policy` と同じ割当（evolving mix の突然変異含む）で policy 列が 4 種を数える、`set_policy` の JSON が通り compare の fork が再現可能で `Multiverse::fork` と一致、Learner 世界のゴールデン、`social()` がカーネルの `TickReport` から組んだ期待値と一致（seek 後・fork 後も）、`agent_json` がカーネルの `Agent` そのもの、`agent_events_json` の絞り込みと半開区間。スレッド（`threads::*`）: 手組みの台帳で履行・不履行・拒否の紐付け・重なる取引の判定、Random 世界で `agent_events_json` との整合とビュー依存、seed 23 の #909 ↔ #363 の取引（TUTORIAL が名指しするもの）と seed 42 に順序どおりの取引が無いこと。
- `wasm-pack test --node crates/cz-web --release`: **wasm 上で native のゴールデンハッシュに到達する**（Forager と Learner の両方、CI）。

## 配信

`npm run build` の `dist/` は静的ファイルだけ（`base: "./"` なのでサブパスでも動く）。フッタにビルド元のコミットが出る。

- GitHub Pages: `.github/workflows/pages.yml`。Settings → Pages で Source を GitHub Actions にし、リポジトリ変数 `PAGES_ENABLED=true` を設定すると `main` への push で配信される。private リポジトリでは Pages 対応プランが要る。
- 配布: CI の `toy1-dist` アーティファクトを unzip し、任意の静的サーバーで配信する（`file://` では Worker が動かないブラウザがある）。
- ローカル確認: `python3 -m http.server 8766` を `dist` の親ディレクトリで実行し `/dist/` を開く。

## チュートリアル

右列の「Five-minute tour」カードが 5 ステップ（観察 → 巻き戻し → 介入 → 分岐 → Verify）を世界の状態から自動判定する。本文は [docs/TUTORIAL.md](../docs/TUTORIAL.md)（Phase 3 の節 6〜8: マーカーと記憶、交渉スレッド、固定 vs 学習の比較）。

## 次

Toy 1 は機能完了・配信中。Phase 3 の UI は 3-9a（社会マーカー・インスペクタ）、3-9b（交渉スレッド、`?seed=`）、3-9c（fixed vs learning の比較、学習世界の選択）まで入った。残りは B 側のインスペクタと介入履歴（任意）、学習世界単独の画面計測、3-10 の文書。
