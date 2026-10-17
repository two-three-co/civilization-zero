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
| `src/protocol.ts` | UI ↔ ワーカーのメッセージ型。`Frame` は `source`（kernel / mock）、`hash`、食料グリッド（`Uint8Array`）、個体配列（`Int32Array`、1 体 8 フィールド = カーネルの trace と同じ並び）を Transferable で渡す |
| `src/client.ts` | `WorldClient`: 1 インスタンス = 1 世界。`init / step / seek / frame` を Promise で返す。`kernel()` / `mock()` |
| `src/worker/kernel.worker.ts` | `WasmWorld`（`crates/cz-web`）を駆動。要求は到着順に直列処理 |
| `src/worker/mock.worker.ts` | スタンドイン世界（台帳も決定論の契約もハッシュもない） |
| `src/render.ts` | Canvas2D 描画（食料 = ImageData を拡大、個体 = 矩形、透明度 = エネルギー）、人口スパークライン |
| `src/App.tsx` | 画面: キャンバス、再生/速度/タイムライン/`Go`、統計（state hash、実測 tick/s・fps・Worker ms/tick、最後の巻き戻し時間）、人口 + 食料カーブ、個体インスペクタ、分岐のプレースホルダ |
| `src/Intervene.tsx` | 介入パネル（干ばつ・資源量スケール・個体追加・個体除去）と、台帳から読み戻した介入履歴。head 以外では Apply 無効。「Fork here」で任意 tick から分岐 |
| `src/Branches.tsx` | A（main）と B（fork）の比較表（tick・人口・食料・ハッシュと差分）、「Verify: fork again」（同一スナップショット + 同一介入で作り直して同 tick のハッシュを照合）、Discard |

## プロトコルの約束

- すべての要求は現在の `Frame`（または `error`）で応答する。UI はワーカーから受け取っていない状態を持たない。
- `seek(tick)`: `frontier`（到達した最大 tick）を超えると前進、戻る場合はカーネル側の cursor（最寄りのスナップショットからの再実行）。
  cursor 位置からの `step` は自分の台帳と照合しながら同じ履歴を再実行し、head に追いつくと以後は head を延ばす。
- 介入は head でのみ受け付ける（`intervene`）。cursor 位置での介入 = 分岐（2b-6）。`history` は台帳の `Intervened` を返す。
- シークはドラッグ中に間引かれ（進行中は最新の要求だけ残す）、Worker の `seek` 所要時間がフレームに載る。
- 分岐（2b-6）: `snapshot` 要求で A の現在ビュー（その tick の開始状態）のバイナリスナップショットを受け取り、2 つ目のワーカーを `init({ snapshot })` で作って介入を適用する。以後は再生・シークを両方に送る（B は fork tick より前に戻れない）。
- 再現性の検証: 保持しておいたスナップショットから 3 つ目のワーカーを作り、同じ介入を適用して同じ tick まで進め、B とハッシュを照合する（MATCH / MISMATCH を表示）。

## 計測（Apple M1、アプリ内蔵ブラウザ、約 1,330 体、2026-10-17）

- 再生: 30 tick/フレームで 1,802 tick/s・60 fps（Worker 0.22 ms/tick）。4 tick/フレームで 240 tick/s。
- 巻き戻し: 249 tick 再実行（間隔 250 の最悪）45〜56 ms、スナップショット上 0 ms。
- 介入: UI で tick 300 に干ばつ → tick 600 / 900 の state hash が `cz run --drought-at 300 --drought-amount 0 --drought-interval 10` と一致。
- 分岐: fork 21 ms、2 世界の同期再生 1,800 tick/s・60 fps。tick 400 で A / B のハッシュが native（無介入 / 干ばつ）と一致、Verify = MATCH（90 ms）。

## 検証

- `cargo test -p cz-web`: ラッパーがカーネルと同一の状態・ハッシュを出す、seek → step が head に合流する、介入は head のみ。
- `wasm-pack test --node crates/cz-web --release`: **wasm 上で native のゴールデンハッシュに到達する**（CI）。

## 次

2b-7（静的配信とチュートリアル）。これで PRODUCT.md の Toy 1 受け入れ基準 4 つが揃う。
