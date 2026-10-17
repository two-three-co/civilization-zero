#!/bin/sh
# EXP-003 の計測ループ（仕様 docs/research/experiments/EXP-003-baseline-grid.md）。
# リポジトリのルートで `sh docs/research/experiments/data/EXP-003/run.sh` と実行する。
# `out/EXP-003/` に CSV を、`out/EXP-003/RUN.log` に各腕の標準出力と壁時計時間を書く。
# 実行後に out/EXP-003/* を docs/research/experiments/data/EXP-003/ へコピーする。
set -eu
BIN=${BIN:-target/release/cz}
OUT=${OUT:-out/EXP-003}
LOG=$OUT/RUN.log
mkdir -p "$OUT"
COMMON="--seeds 10 --ticks 20000 --window 50 --sample 100"
{
  echo "# EXP-003 run log (stdout of every arm, wall time per arm)"
  echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ) UTC / $(date '+%Y-%m-%d %H:%M:%S %Z')"
  echo "commit: $(git rev-parse HEAD) — $(git log -1 --format=%s)"
  echo "worktree: $(git status --porcelain | wc -l | tr -d ' ') modified paths"
  echo "machine: $(sysctl -n machdep.cpu.brand_string), $(sysctl -n hw.ncpu) cores, $(( $(sysctl -n hw.memsize) / 1073741824 )) GB, macOS $(sw_vers -productVersion), $(rustc --version)"
  echo "binary: $BIN, sha256 $(shasum -a 256 "$BIN" | cut -c1-16)…"
  echo "arms: 12 grid cells (learner 32 rows, nomos on, ε × harm_cost × learner_health_weight), forager, learner128"
} > "$LOG"
run_arm() {
  name=$1; shift
  echo "" >> "$LOG"
  echo "## $name: cz sweep $COMMON $* --out $OUT/$name" >> "$LOG"
  start=$(date +%s)
  "$BIN" sweep $COMMON "$@" --out "$OUT/$name" 2>&1 | tee -a "$LOG"
  end=$(date +%s)
  echo "wall time: $((end - start)) s" | tee -a "$LOG"
}
for eps in 0 10 50; do for h in 2000 8000; do for w in 0 2; do
  run_arm "grid_e${eps}_h${h}_w${w}" --policy learner --nomos --learner-epsilon "$eps" --harm-cost "$h" --learner-health-weight "$w"
done; done; done
run_arm forager --policy forager --nomos
run_arm learner128 --policy learner --nomos --learner-states 128
echo "" >> "$LOG"
echo "finished: $(date -u +%Y-%m-%dT%H:%M:%SZ) UTC" | tee -a "$LOG"
