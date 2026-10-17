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
| `src/App.tsx` | 画面: キャンバス、再生/速度/タイムライン、統計（state hash 含む）、個体インスペクタ、介入・分岐のプレースホルダ |

## プロトコルの約束

- すべての要求は現在の `Frame`（または `error`）で応答する。UI はワーカーから受け取っていない状態を持たない。
- `seek(tick)`: `frontier`（到達した最大 tick）を超えると前進、戻る場合はカーネル側の cursor（最寄りのスナップショットからの再実行）。
  cursor 位置からの `step` は自分の台帳と照合しながら同じ履歴を再実行し、head に追いつくと以後は head を延ばす。
- 介入は head でのみ受け付ける（`WasmWorld.intervene(json)`）。cursor 位置での介入 = 分岐（2b-5/2b-6 で UI と一緒に）。
- 分岐比較（2b-6）は `WorldClient` を 2 つ持つだけで済む設計にしてある。

## 検証

- `cargo test -p cz-web`: ラッパーがカーネルと同一の状態・ハッシュを出す、seek → step が head に合流する、介入は head のみ。
- `wasm-pack test --node crates/cz-web --release`: **wasm 上で native のゴールデンハッシュに到達する**（CI）。

## 次

2b-3（60 tick/s の実測表示）→ 2b-4（巻き戻し ≤ 200 ms の計測）→ 2b-5（介入パネル）→ 2b-6（分岐比較）→ 2b-7（配信）。
