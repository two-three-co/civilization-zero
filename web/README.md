# cz-web — Toy 1 の UI（ADR-0010 2b）

ブラウザ内で文明を観察 → 巻き戻し → 介入 → 分岐比較するための UI。現状は **2b-2: 骨格** で、世界は
`src/worker/mock.worker.ts` の**モック**が動かしている（カーネルではない。台帳も決定論の契約もハッシュもない）。
2b-1 で wasm 化したカーネルのワーカーに差し替えるが、`src/protocol.ts` と UI は変えない前提。

## 動かす

```bash
cd web && npm install && npm run dev
```

`http://127.0.0.1:5173/` を開く。`npm run build` は型検査（`tsc --noEmit`）と Vite ビルド。

## 構成

| ファイル | 役割 |
|---|---|
| `src/protocol.ts` | UI ↔ ワーカーのメッセージ型。`Frame` は食料グリッド（`Uint8Array`）と個体配列（`Int32Array`、1 体 8 フィールド = カーネルの trace と同じ並び）を Transferable で渡す |
| `src/client.ts` | `WorldClient`: 1 インスタンス = 1 世界。`init / step / seek / frame` を Promise で返す |
| `src/worker/mock.worker.ts` | スタンドイン世界。パッチ・採集・代謝・分裂出生・餓死/老衰・再生を真似る。50 tick ごとのスナップショットで `seek` |
| `src/render.ts` | Canvas2D 描画（食料 = ImageData を拡大、個体 = 矩形、透明度 = エネルギー）、人口スパークライン |
| `src/App.tsx` | 画面: キャンバス、再生/速度/タイムライン、統計、個体インスペクタ、介入・分岐のプレースホルダ |

## プロトコルの約束

- すべての要求は現在の `Frame`（または `error`）で応答する。UI はワーカーから受け取っていない状態を持たない。
- `seek(tick)` は `frontier`（その世界が到達した最大 tick）を超えると前進、戻る場合は最寄りのスナップショットから再計算。
- 分岐比較（2b-6）は `WorldClient` を 2 つ持つだけで済む設計にしてある。

## 次

2b-1（`crates/cz-web`、wasm-bindgen）→ 2b-3（60 tick/s の実測表示）→ 2b-4（巻き戻し ≤ 200 ms）→ 2b-5/6。
