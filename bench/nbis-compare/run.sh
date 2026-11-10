#!/usr/bin/env bash
# Runs the comparison of docs/nbis-comparison.md: identity checks, then timing rounds.
#
#   NBIS=/path/to/Rel_5.0.0 [NBIS_ARCHIVE=/path/to/nbis_v5_0_0.zip] [WITH_RELEASED=1] \
#     [THREADS="1 2 4 8"] bench/nbis-compare/run.sh WORKDIR [ROUNDS [ROUNDS_LARGE]]
#
# NBIS is the unpacked NBIS 5.0.0 source tree. NBIS_ARCHIVE, optional, is the archive it came
# from: its SHA-256 is checked against the one in docs/nbis-comparison.md and recorded.
# WITH_RELEASED=1 also builds the crates.io release pinned in Cargo.released.lock, measured
# at 1 and 4 threads next to this checkout's revision. ROUNDS (default 21) is the number of
# timed calls per image, rate and implementation; ROUNDS_LARGE (default 11) applies to the
# 5500x8000 image. THREADS (default "1 2 4 8") lists the thread counts measured for this
# checkout; it must include 1. NBIS has no threads and is always measured once.
#
# Output in WORKDIR: machine.txt, identity.txt, results.csv, summary.md (distributions),
# ci.md (medians with 95 % bootstrap intervals, ratios to NBIS, thread scaling, memory).
#
# Real fingerprints, on the maintainer's machine only (the corpus never leaves it):
#
#   WSQ_CORPUS_DIR=/path/to/corpus NBIS=... bench/nbis-compare/run.sh WORKDIR [REPS]
#
# switches to the corpus mode of corpus.sh: it measures the .pgm (8-bit binary) and .wsq files
# found under that directory and writes corpus-summary.md, which holds aggregates only (counts,
# ratios, intervals), no file name and no per-file row. See bench/nbis-compare/README.md.
#
# Linux only (/proc). Nothing is written outside WORKDIR, except target/ of this crate.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
work=${1:?usage: run.sh WORKDIR [ROUNDS [ROUNDS_LARGE]]}
rounds=${2:-21}
rounds_large=${3:-11}
threads_list=${THREADS:-1 2 4 8}
: "${NBIS:?set NBIS to the NBIS 5.0.0 source directory (Rel_5.0.0)}"
nbis_sha256=0adf8ab0f6b0e4208de50ca00ba21d3d77112ecd66288757ddfed21f6bee92c3
mkdir -p "$work"
work=$(cd "$work" && pwd)

# cpu_sample SECONDS: busy and steal share of all CPUs over SECONDS, from /proc/stat.
cpu_sample() {
  local a b
  a=$(head -1 /proc/stat); sleep "$1"; b=$(head -1 /proc/stat)
  awk -v a="$a" -v b="$b" 'BEGIN {
    split(a, x, " "); split(b, y, " ")
    for (i = 2; i <= 9; i++) { t += y[i] - x[i] }
    idle = (y[5] - x[5]) + (y[6] - x[6]); steal = y[9] - x[9]
    printf "busy %.1f%%, steal %.1f%%", 100 * (t - idle) / t, 100 * steal / t }'
}

# --- the machine as it is before this script starts any work -------------------------------------
load_before=$(cut -d' ' -f1-3 /proc/loadavg)
cpu_before=$(cpu_sample 5)

# --- builds ---------------------------------------------------------------------------------
srcs="$NBIS/imgtools/src/lib/wsq/*.c $NBIS/imgtools/src/lib/jpegl/*.c \
      $NBIS/commonnbis/src/lib/fet/*.c $NBIS/commonnbis/src/lib/ioutil/*.c \
      $NBIS/commonnbis/src/lib/util/*.c"
inc="-I$NBIS/commonnbis/include -I$NBIS/imgtools/include"
# shellcheck disable=SC2086
gcc -O2 -w -ffp-contract=off -fno-fast-math -D__NBISLE__ $inc \
  -o "$work/nbis_time" "$here/nbis_time.c" $srcs -lm

cargo build --release --locked --manifest-path "$here/Cargo.toml" --target-dir "$work/target-current"
cp "$work/target-current/release/wsq-nbis" "$work/wsq-nbis-current"
impls=(nbis:0)
for t in $threads_list; do impls+=("current:$t"); done
released_version=
if [ "${WITH_RELEASED:-0}" = 1 ]; then
  released_version=$(grep -A1 'name = "wsq-codec"' "$here/Cargo.released.lock" | sed -n 's/^version = "\(.*\)"/\1/p')
  rm -rf "$work/released-src"; mkdir -p "$work/released-src"
  cp -r "$here/src" "$here/Cargo.toml" "$work/released-src/"
  cp "$here/Cargo.released.lock" "$work/released-src/Cargo.lock"
  sed -i "s|^wsq-codec = .*|wsq-codec = \"=$released_version\"|" "$work/released-src/Cargo.toml"
  cargo build --release --locked --manifest-path "$work/released-src/Cargo.toml" --target-dir "$work/target-released"
  cp "$work/target-released/release/wsq-nbis" "$work/wsq-nbis-released"
  impls+=(released:1 released:4)
fi

# --- conditions -----------------------------------------------------------------------------
repo=$(cd "$here/../.." && pwd)
if revision=$(git -C "$repo" rev-parse --short HEAD 2>/dev/null); then
  # The benchmark's own files (bench/, .gitignore) do not count: they are not the code under test.
  dirty=$(git -C "$repo" status --porcelain -- . ':(exclude)bench' ':(exclude).gitignore' 2>/dev/null | wc -l)
  [ "$dirty" = 0 ] && tree=clean || tree="modified ($dirty paths differ from HEAD)"
else
  revision=unknown; tree=unknown
fi
if [ -n "${NBIS_ARCHIVE:-}" ]; then
  actual=$(sha256sum "$NBIS_ARCHIVE" | cut -d' ' -f1)
  [ "$actual" = "$nbis_sha256" ] || { echo "NBIS_ARCHIVE has SHA-256 $actual, expected $nbis_sha256" >&2; exit 1; }
  nbis_note="archive SHA-256 $actual (matches docs/nbis-comparison.md)"
else
  nbis_note="archive not given, source tree not verified"
fi
{
  echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "kernel: $(uname -sr) $(uname -m)"
  echo "cpu: $(lscpu | sed -n 's/^Model name: *//p'), $(nproc) logical CPUs"
  echo "memory: $(free -m | awk '/^Mem:/ {print $2 " MiB"}')"
  echo "rustc: $(rustc -V)"
  echo "RUSTFLAGS: ${RUSTFLAGS:-unset}"
  echo "thread counts measured for wsq-codec: $threads_list"
  echo "gcc: $(gcc --version | head -1)"
  echo "nbis flags: -O2 -w -ffp-contract=off -fno-fast-math -D__NBISLE__"
  echo "nbis source: $nbis_note"
  echo "wsq-codec current: revision $revision, working tree $tree"
  [ -n "$released_version" ] && echo "wsq-codec released: $released_version from crates.io (Cargo.released.lock)"
  echo "load average before any work (1, 5, 15 min): $load_before"
  echo "CPU over 5 s before any work: $cpu_before"
} > "$work/machine.txt"

# --- corpus mode: real fingerprints, aggregates only ---------------------------------------------
if [ -n "${WSQ_CORPUS_DIR:-}" ]; then
  # shellcheck source=corpus.sh
  . "$here/corpus.sh"
  exit 0
fi

# --- images, then identity in both directions -------------------------------------------------
"$work/wsq-nbis-current" gen "$work/img"
images=(ridges_800x750 ridges_1600x1500 noise_1600x1500 dark_ridges_5500x8000)
rates=(0.75 2.25)
dims() { local n=${1##*_}; echo "${n%x*} ${n#*x}"; }

# call IMPL THREADS OP IMAGE RATE [OUT]: the harness line, optionally keeping the output.
call() {
  local impl=$1 threads=$2 op=$3 image=$4 rate=$5 out=${6:-} w h
  read -r w h <<< "$(dims "$image")"
  local stream="$work/ref/${image}_$rate.wsq"
  if [ "$impl" = nbis ]; then
    if [ "$op" = encode ]; then "$work/nbis_time" e "$work/img/$image.raw" "$w" "$h" "$rate" 500 $out
    else "$work/nbis_time" d "$stream" $out; fi
  else
    if [ "$op" = encode ]; then "$work/wsq-nbis-$impl" encode "$work/img/$image.raw" "$w" "$h" "$rate" 500 "$threads" $out
    else "$work/wsq-nbis-$impl" decode "$stream" "$threads" $out; fi
  fi
}

mkdir -p "$work/ref" "$work/tmp"
: > "$work/identity.txt"
fail=0
for image in "${images[@]}"; do
  read -r w h <<< "$(dims "$image")"
  for rate in "${rates[@]}"; do
    ref="$work/ref/${image}_$rate.wsq"
    "$work/nbis_time" e "$work/img/$image.raw" "$w" "$h" "$rate" 500 "$ref" > /dev/null
    "$work/nbis_time" d "$ref" "$work/tmp/nbis.raw" > /dev/null          # NBIS decodes NBIS
    for spec in "${impls[@]:1}"; do
      impl=${spec%:*}; threads=${spec#*:}
      tag="$image $rate $impl/$threads"
      call "$impl" "$threads" encode "$image" "$rate" "$work/tmp/rust.wsq" > /dev/null
      if cmp -s "$ref" "$work/tmp/rust.wsq"; then r=identical; else r=DIFFERENT; fail=1; fi
      echo "$tag encoder stream vs NBIS: $r ($(stat -c %s "$ref") bytes)" >> "$work/identity.txt"
      call "$impl" "$threads" decode "$image" "$rate" "$work/tmp/rust.raw" > /dev/null   # crate decodes NBIS stream
      if cmp -s "$work/tmp/nbis.raw" "$work/tmp/rust.raw"; then r=identical; else r=DIFFERENT; fail=1; fi
      echo "$tag decoder pixels (NBIS stream) vs NBIS: $r" >> "$work/identity.txt"
      "$work/nbis_time" d "$work/tmp/rust.wsq" "$work/tmp/nbis2.raw" > /dev/null          # NBIS decodes crate stream
      if cmp -s "$work/tmp/nbis.raw" "$work/tmp/nbis2.raw"; then r=identical; else r=DIFFERENT; fail=1; fi
      echo "$tag NBIS pixels from the crate's stream vs NBIS's own: $r" >> "$work/identity.txt"
    done
  done
done
rm -rf "$work/tmp"
cat "$work/identity.txt"
[ "$fail" = 0 ] || { echo "identity check failed: no timing taken" >&2; exit 1; }

# --- timing: one call per process, implementations interleaved and rotated each round ----------
csv="$work/results.csv"
echo "round,impl,threads,op,image,rate,ms,out_bytes,fnv,extra_rss_kb" > "$csv"
n=${#impls[@]}
stat_start=$(head -1 /proc/stat)
for image in "${images[@]}"; do
  count=$rounds
  [ "$image" = dark_ridges_5500x8000 ] && count=$rounds_large
  for ((round = 1; round <= count; round++)); do
    for rate in "${rates[@]}"; do
      for op in encode decode; do
        for ((k = 0; k < n; k++)); do
          spec=${impls[(k + round) % n]}
          impl=${spec%:*}; threads=${spec#*:}
          line=$(call "$impl" "$threads" "$op" "$image" "$rate")
          ms=${line#ms=}; ms=${ms%% *}
          bytes=${line#*out_bytes=}; bytes=${bytes%% *}
          fnv=${line#*fnv=}; fnv=${fnv%% *}
          rss=${line#*extra_rss_kb=}
          echo "$round,$impl,$threads,$op,$image,$rate,$ms,$bytes,$fnv,$rss" >> "$csv"
        done
      done
    done
  done
done
stat_end=$(head -1 /proc/stat)
{
  echo "load average at end (1, 5, 15 min): $(cut -d' ' -f1-3 /proc/loadavg)"
  # Share of all CPUs over the whole timing phase, this benchmark's own processes included.
  awk -v a="$stat_start" -v b="$stat_end" 'BEGIN {
    split(a, x, " "); split(b, y, " ")
    for (i = 2; i <= 9; i++) { t += y[i] - x[i] }
    idle = (y[5] - x[5]) + (y[6] - x[6]); steal = y[9] - x[9]
    printf "CPU over the timing phase: busy %.1f%%, steal %.1f%% (this benchmark included)\n", 100 * (t - idle) / t, 100 * steal / t }'
} >> "$work/machine.txt"
python3 -I "$here/summarize.py" "$csv" > "$work/summary.md"
python3 -I "$here/summarize.py" "$csv" --ci | tee "$work/ci.md"
