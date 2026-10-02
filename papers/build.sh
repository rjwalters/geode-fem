#!/usr/bin/env bash
# papers/build.sh: build one Anvil paper version dir to main.pdf + main.bbl.
#
# Usage:
#   papers/build.sh <version-dir>
#   e.g. papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.4
#
# Runs the explicit pdflatex -> bibtex -> pdflatex x2 sequence. After that it
# keeps rerunning pdflatex while LaTeX reports "Label(s) may have changed",
# with a cap of 5 pdflatex passes in total. This is the same convergence loop
# as anvil paper-audit step 4. The build fails (nonzero exit) on any pdflatex
# or bibtex error, on an undefined citation, or if the loop hasn't converged
# by the cap.
#
# Class lookup: TEXINPUTS is ".:<repo>/papers/_shared:". A copy of
# anvil-paper.cls inside the version dir always wins, because "." comes first.
# papers/_shared is only a fallback. Never point TEXINPUTS at another paper's
# version dir: those dirs hold a committed main.bbl, and pdflatex would read it
# in place of this paper's bibliography (see papers/README.md).
#
# Full compiler output goes to <version-dir>/build.log, which is gitignored.
set -euo pipefail

usage() { echo "usage: $0 <version-dir>" >&2; exit 2; }
[ $# -eq 1 ] || usage

papers_dir="$(cd "$(dirname "$0")" && pwd)"
shared_dir="$papers_dir/_shared"
dir="$1"
[ -d "$dir" ] || { echo "error: not a directory: $dir" >&2; exit 2; }
[ -f "$dir/main.tex" ] || { echo "error: no main.tex in $dir" >&2; exit 2; }
cd "$dir"

for tool in pdflatex bibtex; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool not on PATH" >&2; exit 3; }
done

# Warn if the version dir's vendored class has drifted from the shared copy.
# This is not an error: frozen historical versions keep their own bytes.
if [ -f anvil-paper.cls ] && [ -f "$shared_dir/anvil-paper.cls" ] \
   && ! cmp -s anvil-paper.cls "$shared_dir/anvil-paper.cls"; then
  echo "note: $dir/anvil-paper.cls differs from papers/_shared/anvil-paper.cls (using the local copy)" >&2
fi

export TEXINPUTS=".:$shared_dir:"
log=build.log
: > "$log"

run() {
  echo "=== $* ===" >> "$log"
  if ! "$@" >> "$log" 2>&1; then
    echo "error: '$*' failed in $dir; see $dir/$log" >&2
    tail -n 30 "$log" >&2
    exit 1
  fi
}

pdf() { run pdflatex -interaction=nonstopmode -halt-on-error main.tex; }

pdf
run bibtex main
pdf
pdf
passes=3
while grep -q 'Label(s) may have changed' main.log; do
  if [ "$passes" -ge 5 ]; then
    echo "error: labels not converged after $passes pdflatex passes; see $dir/$log" >&2
    exit 1
  fi
  pdf
  passes=$((passes + 1))
done

if grep -q -E "Citation .* undefined|There were undefined citations" main.log; then
  echo "error: undefined citations remain; see $dir/main.log" >&2
  grep -E "Citation .* undefined" main.log >&2 || true
  exit 1
fi

echo "ok: $dir/main.pdf ($passes pdflatex passes; log in $dir/$log)"
