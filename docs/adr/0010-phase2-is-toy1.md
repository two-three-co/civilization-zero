# ADR-0010: Phase 2 の完了条件を「Toy 1 がブラウザで動く」に置き換える

- 状態: 採用 (2026-10-17)
- 前提: docs/PRODUCT.md、ADR-0009

## 文脈

当初の Phase 2「Multiverse Foundations」は、イベントログ・スナップショット・再生・分岐に加え、実験スケジューラと統計解析を含んでいた。
製品ゴール（PRODUCT.md）により、Phase 2 の成果物は「ユーザーがブラウザで巻き戻し・介入・分岐比較を体験できる Toy 1」とする。
ADR-0009 で、カーネルのブラウザ内実行が決定論を保ったまま十分な性能で可能だと計測した。

## 決定

**Phase 2 = Toy 1。** 完了条件は PRODUCT.md の受け入れ基準 1〜4 に、計測基準（下記）を加えたもの。

元の Phase 2 項目のうち **実験スケジューラ・Parquet 出力・統計解析は Phase 5（Research Platform）へ移す**。Toy 1 に不要で、2a-5 の fork API が揃えば Phase 5 で並列化するだけになる。

Phase 2 を **2a（カーネル側、CLI で検証可能）** と **2b（ブラウザ側）** に分ける。2b の UI 骨格（2b-2）はモックの Worker で 2a と並行して作れる。

## タスク分解

| # | タスク | 依存 | 受け入れ基準 |
|---|---|---|---|
| 2a-1 ✅ | `Intervention` enum と `World::intervene()`。初期セット: `Drought { regrow_amount, regrow_interval }`、`ScaleFood { permille }`、`SpawnAgents { n, pos, policy }`、`RemoveAgent { id }`（1 体ずつ。`Event` を `Copy` に保つため）。各介入は `Event::Intervened` として記録し、食料・エネルギーの差分を台帳に計上する。**済 2026-10-17**（`src/intervention.rs`, `tests/intervention.rs`） | – | 介入後も `check_invariants` が通る（保存則は介入分を台帳で説明）。各介入の単体テスト |
| 2a-2 ✅ | イベント台帳 `EventLog`: tick ごとの `Vec<Event>` の追記、JSON Lines の書き出し/読み込み、tick 範囲の取り出し。**済 2026-10-17**（`src/event_log.rs`、20,000 tick = 54,649 イベント / 6.27 MB） | 2a-1 | 20,000 tick の台帳を書いて読み戻せる。サイズを計測して記録 |
| 2a-3 | Store / Transfer 行動と `Event::Acted { agent, intent, outcome }`（全行動記録は設定で ON/OFF） | 2a-2 | 二重移転なし・保存則のテスト。記録 ON 時の台帳サイズを計測 |
| 2a-4 | 再生: `replay(snapshot_at_k, log[k..t])` が直接実行と同一ハッシュに到達する | 2a-2 | 任意 tick の復元 = 直接実行（テスト）。復元時間を計測 |
| 2a-5 | fork: スナップショット + 介入 → 新しい世界（親世界 id・分岐 tick の系譜付き）。CLI `cz fork` | 2a-1, 2a-4 | 同一介入を同一 tick で 2 回 fork してハッシュ一致（テスト + CLI） |
| 2a-6 | バイナリスナップショット（postcard を第一候補に比較検討、ADR-0006 を更新） | – | 1,300 体で ≤ 100 KB、往復 ≤ 1 ms（native）。形式バージョン付き、壊れたデータを拒否 |
| 2b-1 | `crates/cz-web`: wasm-bindgen API（new / step / snapshot / restore / fork / intervene / frame）。CI で wasm ビルドと native-wasm ハッシュ一致テスト | 2a-1, 2a-5, 2a-6 | CI 緑。ハッシュ一致テストが CI に入っている |
| 2b-2 ✅ | Vite + React + TypeScript の UI 骨格、Worker プロトコル（型付き配列のフレーム転送）。**済 2026-10-17**（`web/`、`web/README.md`） | – | モック Worker でタイムラインとキャンバスが動く |
| 2b-3 | 観察: Canvas2D 描画、人口・食料カーブ | 2b-1, 2b-2 | 1,300 体の世界が実時間 **≥ 60 tick/s** で描画される（画面に実測値を表示） |
| 2b-4 | 巻き戻し: K tick ごとのスナップショット + 再生でタイムラインをスクラブ | 2b-3, 2a-4 | 任意 tick へ **≤ 200 ms** で移動（実測値を表示） |
| 2b-5 | 介入パネル（干ばつ・資源量・個体の追加/除去） | 2b-4, 2a-1 | 介入が台帳に載り、UI の履歴に出る |
| 2b-6 | 分岐の並列比較: Worker 2 つの同期再生、指標の差分、両世界の状態ハッシュ表示 | 2b-5 | PRODUCT.md 基準 1〜3。同一介入 2 回でハッシュ一致が画面で確認できる |
| 2b-7 | 静的配信（GitHub Pages 等）と 5 分で終わるチュートリアル | 2b-6 | PRODUCT.md 基準 4。ユーザーがリンクを開いて自力で体験できる |

工数見積りは出さない（計測主義）。各タスクは受け入れ基準を満たしたら STATUS.md に移す。

## Phase 2 の完了条件（まとめ）

1. PRODUCT.md の Toy 1 受け入れ基準 1〜4。
2. 計測: 1,300 体で ≥ 60 tick/s、巻き戻し ≤ 200 ms、いずれも画面に実測値を表示。
3. 決定論: 同一介入 2 回で状態ハッシュ一致。CI に native-wasm ハッシュ一致テスト。
4. **ユーザーがブラウザで確認するデモ = Toy 1 そのもの。** 配信 URL を STATUS.md に記録する。

## 研究要件との整合

- Toy 1 の fork + 介入は、そのまま H4（モデル内因果効果）の実験単位になる。Phase 5 はこれを多シード・条件格子で並列化し、統計解析を加える。
- 2a-2 / 2a-3 の台帳は Phase 4（規則違反の判定）の基盤。
- LLM は Toy 1 に登場しない。将来の「年代記の語り」は台帳を読むだけの別コンポーネントとして追加する（PRODUCT.md 原則 2）。

## 影響

- Phase 3（認知）は Toy 1 完成後に着手。ただし 2a 完了後、Phase 3 のカーネル側作業（他者観測の空間インデックス）は 2b と並行できる。
- フロントの技術選定: Vite + React + TypeScript + Canvas2D。PixiJS は 10,000 体以上の描画が必要になった時点で（ADR-0007 の方針どおり）。
- Phase 5 の内容が増える（スケジューラ・統計）。Phase 5 の完了条件にもブラウザデモ（実験ダッシュボード）を含める（PRODUCT.md）。
