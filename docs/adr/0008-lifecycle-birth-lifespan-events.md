# ADR-0008: 出生（エネルギー分裂）・寿命・死因・ライフサイクルイベント・方策パラメータの継承

- 状態: 採用 (2026-10-17)
- 影響: `simulation-core` 0.1 → 遷移関数の変更、スナップショット形式 1 → 2、ゴールデンハッシュ更新

## 文脈

Phase 1 のカーネルには出生も寿命もなく、初期の餓死の後は個体数が固定されていた（STATUS.md 旧版）。
世代交代がなければ進化（Phase 6）も、制度の長期的な安定性（H2/H3）も扱えない。
また Phase 2 のイベント台帳に、出生・死亡をそのまま載せられる形式が必要である。

## 決定

### 1. 出生はエネルギーの分裂として実装する

- 条件: Upkeep 後に生存しており `energy >= birth_threshold`（既定 80,000）。
- 効果: 親の `energy` から `child_energy`（既定 40,000）を子へ移す。親の在庫（食料）は親に残る。
  子は親と同じセルに、`health = max_health`, `age = 0`, `inventory = 0` で生まれる。
- 保存則: エネルギーは移動するだけで生成されない。台帳の `energy_initial/gained/spent/written_off` は変えず、
  `ledger.births` を加える。`check_invariants` のエネルギー保存式はそのまま成り立つ（`tests/determinism.rs::generations_conserve_energy_and_food`）。
- `birth_threshold: None` で無効化。`0 < child_energy <= birth_threshold <= max_energy` を検証。

検討した代替案:
- 親のエネルギーを比率で分ける（半分ずつ）: 奇数の端数規則が必要。固定量の方が「次の出生までに child_energy 分を稼ぐ」という不応期が明確。
- 出生コスト（燃焼）を別途課す: 保存則の検算項が増える。必要になれば介入パラメータとして追加する。

### 2. 寿命と死因

- `max_age: Option<u64>`（既定 3,000）。Upkeep で `age += 1` した後、Death で `age >= max_age` なら `OldAge`。
- **死因の優先順位**: `health <= 0` → `Starvation` を先に判定し、それ以外で `OldAge`。両方同時でも 1 死因。
- `DeathCause::COUNT` 配列で `Ledger.deaths_by_cause` と `TickReport.deaths_by_cause` に集計。`deaths` は合計で、両者の整合を不変条件で検査。
- 初期個体の `age` は `max_age` が設定されているとき `AgentSpawn` ストリームから `0..max_age` の一様乱数で与える。
  全員 age 0 だと tick = max_age で創設世代が一斉死する（コホート効果）ため。位置の乱数は従来どおり先に引くので、位置は変わらない。
  同期コホートが必要な実験のためのフラグは、必要になった時点で追加する。

### 3. フェーズ順序: Death → Birth

死者は繁殖しない（`the_dead_do_not_reproduce`）。寿命に達した親のエネルギーは子に渡らず台帳で償却される。
逆順（Birth → Death）も自然だが、「子は死者から生まれ得るか」という問いを避ける方を選んだ。

### 4. 新規 ID の採番仕様

- `next_agent_id` は単調増加で再利用しない（既存）。
- 同一 tick の出生は **親 id の昇順**に処理し、子 id を `next_agent_id, next_agent_id+1, …` の順に割り当てる。
  「親 id が小さいほど子 id が小さい」が仕様（`birth_splits_energy_and_numbers_children_in_parent_order`）。
- 子は `agents` の末尾に追加される。新 id は既存のどの id より大きいので昇順不変条件は保たれ、ソート不要。
- 子は生まれた tick には行動しない（Decide は tick 開始時の個体集合に対して行われる）。

### 5. 方策パラメータの継承と突然変異

- `PolicyParams { hunger_permille }` を各個体に持たせ、Forager は `energy * 1000 < max_energy * hunger_permille` で「空腹」を判定する
  （以前の `energy < max_energy / 2` と、既定値 500・`max_energy` が偶数のとき同値）。
- 子は親の `policy`（種類）と `params` を**そのまま**継承する。
- `mutation_enabled`（既定 **false**）のときだけ、`RngStream(seed, tick, child_id, Stream::Mutation)` から各パラメータを `[-step, +step]` の一様整数で摂動し範囲にクランプする。
  子 id をキーにするのは、出生ごとに一意で、親の処理順に依存しないため。
- 突然変異を OFF にしておく理由: Phase 1 の受け入れ基準（個体数の収束）を進化と切り離して測るため。ON の実験は Phase 6 で事前登録する。

### 6. ライフサイクルイベント

```
Event::Born { tick, child, parent, pos, energy }
Event::Died { tick, agent, cause, pos, age, energy_written_off, food_dropped }
```

- 各レコードが tick を含み自己完結。serde の内部タグ付き enum（`"kind": "born" | "died"`）で、JSON Lines にそのまま書ける。
- `TickReport.events` に発生順（死亡は id 順、出生は親 id 順）で入る。Phase 2 の台帳は `Acted { agent, intent, outcome }` と `Intervened { … }` を同じ enum に加える予定で、本 ADR の 2 種は変更不要の想定。

## ゴールデンハッシュの更新

`tests/determinism.rs::golden_hash_is_stable`（seed 42, default, 200 tick）:
`0xaac57b73180a3d65` → `0xd1890b311b92678e`

変わった理由（すべて意図した変更）:
1. `WorldConfig` にフィールドが増えた（state_hash は config の JSON を含む）。
2. 既定で出生と寿命が有効になり、200 tick 時点の個体集合そのものが変わる。
3. 初期個体の age が乱数化された。
4. state_hash が `params` と `births` / `deaths_by_cause` を含むようになった。
5. Forager の視界走査が抽選キーを遅延して引くようになった（同順位のセルだけがキーを持つ。
   勝者の分布は同じだが乱数の消費列が変わる）。計測上の理由: `decide` が実行時間の 82% を占めていた（macOS `sample`）。
   効果は約 15%（定常状態 181 → 152 ns/体·tick）に留まり、視界走査自体が主因と判明。調査は TODO に残す。

途中の中間値 `0x01cd715b494ac71e`（5. を適用する前）はコミットしていない。

## 帰結

- 既定の世界は 1,000 体から出発し、約 300 tick で 1,370〜1,510 体のピークを経て、約 1,250〜1,365 体の帯域に収束する（EXP-001、10 シード）。
- スナップショット形式 2。形式 1 のファイルは読めない（`SnapshotError::Format`）。
- Random 方策は出生閾値に届かず（20,000 tick で出生 0〜2 件）全滅する。帰無モデルとして期待どおり。
- 設定項目が 6 つ増えた。`WorldConfig::default()` の値は「発散も絶滅もしない」ことだけを根拠に選んだ暫定値であり、研究上の意味はない。
