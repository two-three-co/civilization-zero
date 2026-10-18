#!/bin/sh
# EXP-004 の計測ループ（仕様 docs/research/experiments/EXP-004-first-occupant-norm.md）。
# リポジトリのルートで `sh docs/research/experiments/data/EXP-004/run.sh` と実行する
# （zsh は変数を単語に分割しないので、必ず sh で）。
# `out/EXP-004/` に CSV を、`out/EXP-004/RUN.log` に各腕の標準出力と壁時計時間を書く。
# 実行後に out/EXP-004/* を docs/research/experiments/data/EXP-004/ へコピーする。
set -eu
BIN=${BIN:-target/release/cz}
OUT=${OUT:-out/EXP-004}
LOG=$OUT/RUN.log
mkdir -p "$OUT"
COMMON="--ticks 20000 --window 50 --sample 100"
INST="--policy institution --no-enforcer-motions --enforcer-share 100"
LRN="--policy learner --preset nomos --learner-states 128"
{
  echo "# EXP-004 run log (stdout of every arm, wall time per arm)"
  echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ) UTC / $(date '+%Y-%m-%d %H:%M:%S %Z')"
  echo "commit: $(git rev-parse HEAD) — $(git log -1 --format=%s)"
  echo "worktree: $(git status --porcelain | wc -l | tr -d ' ') modified paths"
  echo "machine: $(sysctl -n machdep.cpu.brand_string), $(sysctl -n hw.ncpu) cores, $(( $(sysctl -n hw.memsize) / 1073741824 )) GB, macOS $(sw_vers -productVersion), $(rustc --version)"
  echo "load at start: $(uptime | sed 's/.*load averages*: //')"
  echo "binary: $BIN, sha256 $(shasum -a 256 "$BIN" | cut -c1-16)…"
  echo "arms: le0 / le / l0 (30 seeds), l1, n_rand, n_blind, n_nosanction, n_nocomm, n_nomem, w0, w1 (10 seeds); 20,000 ticks, window 50%, sample 100 (n_rand 10)"
} > "$LOG"
run_arm() {
  name=$1; seeds=$2; shift 2
  echo "" >> "$LOG"
  echo "## $name: cz sweep $COMMON --seeds $seeds $* --out $OUT/$name" >> "$LOG"
  start=$(date +%s)
  "$BIN" sweep $COMMON --seeds "$seeds" "$@" --out "$OUT/$name" 2>&1 | tee -a "$LOG"
  end=$(date +%s)
  echo "wall time: $((end - start)) s" | tee -a "$LOG"
}
run_arm le0 30 $INST
run_arm le 30 $INST --enact encroach
run_arm l0 30 $LRN
run_arm l1 10 $LRN --enact encroach
run_arm n_rand 10 --policy random --preset nomos --enact encroach --sample 10
run_arm n_blind 10 $INST --enact encroach --blind
run_arm n_nosanction 10 $INST --enact encroach:32:none
run_arm n_nocomm 10 $INST --enact encroach --signal-radius 0
run_arm n_nomem 10 $INST --enact encroach --memory-slots 0
run_arm w0 10 $INST --learner-health-weight 2
run_arm w1 10 $INST --enact encroach --learner-health-weight 2
echo "" >> "$LOG"
echo "finished: $(date -u +%Y-%m-%dT%H:%M:%SZ) UTC, load $(uptime | sed 's/.*load averages*: //')" | tee -a "$LOG"
