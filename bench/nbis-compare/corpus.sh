# Corpus mode of run.sh (WSQ_CORPUS_DIR). Sourced by run.sh after the builds; not run alone.
#
# Measures NBIS 5.0.0 and this checkout's wsq-codec on the files of a local directory of real
# fingerprints, and writes aggregates only. Privacy rules this script keeps:
#   - no file name, path or per-file row is ever written to WORKDIR or printed: files are only
#     known by their rank in the sorted list, and the per-call rows (which carry that rank) live
#     in a scratch directory that is deleted when the script ends, whatever the outcome;
#   - the tools' error messages (which may name a file) are discarded, only counts are kept;
#   - the pixels of a .pgm are copied to a raw file in the scratch directory for the length of
#     one file, then overwritten by the next one and deleted at the end;
#   - corpus-summary.md and machine.txt are the only files meant to leave WORKDIR; machine.txt
#     does not mention the corpus.
#
# Files used, found recursively under $WSQ_CORPUS_DIR (extension case ignored):
#   *.pgm  binary 8-bit PGM: encoded by both implementations at CORPUS_RATE (default 0.75 bpp,
#          500 ppi declared), then the stream NBIS wrote is decoded by both;
#   *.wsq  decoded by both, as is (a stream inside another file is not looked for: use
#          `wsq extract` first).
# REPS, the second argument of run.sh (default 5), is the number of timed calls per file,
# implementation and operation; the implementations are interleaved and rotated as in the
# synthetic mode. Every file is first checked for identity (stream bytes, pixels, in both
# directions), as in the synthetic mode; a difference is counted in the summary, and the timing
# of that file is kept (the summary says how many files differed).

corpus="$WSQ_CORPUS_DIR"
[ -d "$corpus" ] || { echo "WSQ_CORPUS_DIR is not a directory" >&2; exit 1; }
reps=${2:-5}
rate=${CORPUS_RATE:-0.75}
scratch="$work/.corpus-scratch"
rm -rf "$scratch"; mkdir -p "$scratch"
trap 'rm -rf "$scratch"' EXIT
calls="$scratch/calls.csv"
checks="$scratch/checks.csv"
echo "file,round,impl,threads,op,ms,out_bytes,fnv,extra_rss_kb" > "$calls"
echo "file,check,ok" > "$checks"
rust="$work/wsq-nbis-current"
nbis_bin="$work/nbis_time"

# run_call IMPL THREADS OP KIND: one timed call on the current file; the harness line, or nothing.
# KIND pgm: encode reads $scratch/in.raw (W H in $w $h), decode reads $scratch/ref.wsq;
# KIND wsq: decode reads $scratch/cur.wsq.
run_call() {
  local impl=$1 threads=$2 op=$3 kind=$4 stream
  if [ "$kind" = pgm ]; then stream="$scratch/ref.wsq"; else stream="$scratch/cur.wsq"; fi
  if [ "$impl" = nbis ]; then
    if [ "$op" = encode ]; then "$nbis_bin" e "$scratch/in.raw" "$w" "$h" "$rate" 500 2> /dev/null
    else "$nbis_bin" d "$stream" 2> /dev/null; fi
  else
    if [ "$op" = encode ]; then "$rust" encode "$scratch/in.raw" "$w" "$h" "$rate" 500 "$threads" 2> /dev/null
    else "$rust" decode "$stream" "$threads" 2> /dev/null; fi
  fi
}

# check FILE_INDEX NAME STATUS: records one identity check (no name of any file).
check() { echo "$1,$2,$3" >> "$checks"; }
same() { if cmp -s "$1" "$2"; then echo 1; else echo 0; fi; }

n=${#impls[@]}
idx=0; skipped=0; used=0; failed_calls=0
files=()
while IFS= read -r -d '' f; do files+=("$f"); done < <(
  LC_ALL=C find "$corpus" -type f \( -iname '*.pgm' -o -iname '*.wsq' \) -print0 | LC_ALL=C sort -z)
total=${#files[@]}
[ "$total" -gt 0 ] || { echo "no .pgm or .wsq file found in the corpus" >&2; exit 1; }
echo "corpus mode: $total candidate files, $reps timed calls each (no names are printed)" >&2

for f in "${files[@]}"; do
  idx=$((idx + 1))
  case "${f##*.}" in [Pp][Gg][Mm]) kind=pgm ;; *) kind=wsq ;; esac
  if [ "$kind" = pgm ]; then
    if ! dims=$("$rust" pgm2raw "$f" "$scratch/in.raw" 2> /dev/null); then skipped=$((skipped + 1)); continue; fi
    read -r w h <<< "$dims"
    # Reference stream: NBIS's, for the decoding series and the identity checks.
    if ! "$nbis_bin" e "$scratch/in.raw" "$w" "$h" "$rate" 500 "$scratch/ref.wsq" > /dev/null 2>&1; then
      skipped=$((skipped + 1)); continue; fi
  else
    cp -- "$f" "$scratch/cur.wsq"
    w=0; h=0
    cp "$scratch/cur.wsq" "$scratch/ref.wsq"
  fi
  if ! "$nbis_bin" d "$scratch/ref.wsq" "$scratch/nbis.raw" > /dev/null 2>&1; then
    skipped=$((skipped + 1)); continue; fi
  used=$((used + 1))
  # Identity, per thread count of the crate.
  for spec in "${impls[@]:1}"; do
    threads=${spec#*:}
    if [ "$kind" = pgm ]; then
      "$rust" encode "$scratch/in.raw" "$w" "$h" "$rate" 500 "$threads" "$scratch/rust.wsq" > /dev/null 2>&1 || true
      check "$idx" "encoder stream = NBIS's [$threads thread(s)]" "$(same "$scratch/ref.wsq" "$scratch/rust.wsq")"
      "$nbis_bin" d "$scratch/rust.wsq" "$scratch/nbis2.raw" > /dev/null 2>&1 || true
      check "$idx" "NBIS pixels from the crate's stream = NBIS's own [$threads thread(s)]" "$(same "$scratch/nbis.raw" "$scratch/nbis2.raw")"
    fi
    "$rust" decode "$scratch/ref.wsq" "$threads" "$scratch/rust.raw" > /dev/null 2>&1 || true
    check "$idx" "decoder pixels = NBIS's [$threads thread(s)]" "$(same "$scratch/nbis.raw" "$scratch/rust.raw")"
  done
  # Timing: one call per process, implementations interleaved, order rotated each round.
  ops=(decode); [ "$kind" = pgm ] && ops=(encode decode)
  for ((round = 1; round <= reps; round++)); do
    for op in "${ops[@]}"; do
      for ((k = 0; k < n; k++)); do
        spec=${impls[(k + round) % n]}
        impl=${spec%:*}; threads=${spec#*:}
        line=$(run_call "$impl" "$threads" "$op" "$kind" || true)
        case "$line" in ms=*) ;; *) failed_calls=$((failed_calls + 1)); continue ;; esac
        ms=${line#ms=}; ms=${ms%% *}
        bytes=${line#*out_bytes=}; bytes=${bytes%% *}
        fnv=${line#*fnv=}; fnv=${fnv%% *}
        rss=${line#*extra_rss_kb=}
        echo "$idx,$round,$impl,$threads,$op,$ms,$bytes,$fnv,$rss" >> "$calls"
      done
    done
  done
  [ $((idx % 10)) = 0 ] && echo "  $idx / $total files" >&2
done

{
  echo "# Corpus run: aggregates only"
  echo
  echo "Date: $(date -u +%Y-%m-%d). Machine and revision: see machine.txt of the same run."
  echo "NBIS 5.0.0 and wsq-codec (this checkout), thread counts of the crate: $threads_list. Rate $rate bpp, 500 ppi declared."
  echo "$total candidate files (.pgm, .wsq): $used measured, $skipped skipped (unreadable or not 8-bit), $failed_calls timed calls failed."
  echo "$reps timed calls per file, implementation and operation."
  echo
  python3 -I "$here/summarize.py" "$calls" "$checks" --corpus
} > "$work/corpus-summary.md"
cat "$work/corpus-summary.md"
echo "corpus-summary.md holds aggregates only; nothing else in $work is meant to be shared." >&2
