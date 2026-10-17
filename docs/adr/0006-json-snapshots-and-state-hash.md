# ADR-0006: スナップショットは JSON、同一性判定は独自 FNV-1a ハッシュ

- 状態: 採用 (2026-10-17)、Phase 2 で再評価

## 決定
- スナップショット `{format: 1, state_hash, world}` を serde_json で保存。読み込み時に config 検証・不変条件・ハッシュを照合し、壊れた/改竄された状態を拒否する。
- `World::state_hash()` は全フィールドを固定順・リトルエンディアンで FNV-1a に流す。

## 理由
- Phase 1 では可読性・デバッグ容易性を優先。1,000 体世界の往復は 1.13 ms（計測値）で十分。
- `std::hash::Hash` の出力は Rust のバージョン間で安定が保証されないため使わない。

## 再検討
Phase 2 で数千スナップショットを持つ fork ツリーを扱う際、バイナリ形式（postcard 等）+ 差分保存を検討する。FNV-1a は暗号的でないので、外部から受け取った状態の真正性検証には使わない。

## 改訂 (2026-10-17, ADR-0010 2a-6): バイナリ形式を追加し、実行時の既定にする

- 追加: `World::to_bytes` / `World::from_bytes`。先頭 4 バイトのマジック `CZS\0` の後に、JSON と同じ `{format, state_hash, world}` レコードを **postcard** で符号化する。
  読み込み時の検証（形式版 → config → 不変条件 → ハッシュ）は JSON と共通（`validate_snapshot`）。
- postcard を選んだ理由: serde ベースで既存の derive がそのまま使える、varint で小さい、`no_std` 対応（wasm で軽い）、ワイヤ形式の安定を明言、MIT/Apache-2.0、保守が活発。
  bincode は 2.x で API が serde から離れ、MessagePack/CBOR は自己記述型で大きい。
- 注意: postcard は `#[serde(tag = …)]`（内部タグ付き enum）を復号できない。`World` には含まれないが、`Event` / `Intervention` は内部タグ付きなので、台帳をバイナリ化するときは別のワイヤ型が要る（JSON Lines のまま）。
- 計測（1,357 体, 128², tick 1,000）: **56,846 bytes**（JSON 304,107 bytes の 1/5.3）。往復 **0.69 ms**（JSON 1.63 ms）。目標（≤ 100 KB、≤ 1 ms）を満たす。
- CLI: `--snapshot` のパスが `.json` なら JSON、それ以外はバイナリ。
- JSON 形式は残す（可読・デバッグ・テスト用）。両形式とも `SNAPSHOT_FORMAT`（現在 3）を共有する。
