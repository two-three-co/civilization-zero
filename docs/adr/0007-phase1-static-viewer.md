# ADR-0007: Phase 1 のビューアは依存なしの静的 HTML + Canvas2D

- 状態: 採用 (2026-10-17)。React + Vite + PixiJS への移行は Phase 2 で行う

## 文脈
マスタープロンプトのスタックは TypeScript / React / Vite / Three.js or PixiJS。Phase 1 のビューアの役割は「CLI が出した trace を再生して目視確認すること」のみ。

## 決定
`viewer/index.html` 1 ファイル（ビルド不要）。食料は 1px/セルの ImageData を拡大、エージェントは矩形描画。

## 理由
- ライブ配信（WebSocket）・複数パネル・状態管理が必要になるのは Phase 2 以降。今 React/Vite を入れても使う機能がない（原則 C: 過剰な抽象化を避ける）。
- 1,000 体 × 128² グリッドは Canvas2D で十分（ブラウザで目視確認済み）。数万体以上は PixiJS/WebGL が必要になる見込みで、そのときに移行する。

## 帰結
trace 形式（`format: 1`）は使い捨て。Phase 2 でカーネルのイベント台帳と統合した配信形式に置き換える。
