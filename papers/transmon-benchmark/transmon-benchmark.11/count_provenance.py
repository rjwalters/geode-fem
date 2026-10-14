#!/usr/bin/env python3
"""Recompute the Section 9 provenance counts from local git history (Python >= 3.11).

Read-only and offline: it runs `git log` against a pinned base commit and never
touches the network or the working tree. It needs a full (non-shallow) clone
that contains BASE.

Method, at BASE (the main commit this revision was drafted against):

- PRs: first-parent history of main over PATHS; the PR number is taken from a
  "Merge pull request #N" subject or a trailing "(#N)" squash subject, then
  deduplicated.
- Commits: non-merge history over PATHS. Each commit's Co-Authored-By trailers
  are read; a commit counts once per Claude family (Opus, Fable, Sonnet) it
  names, and a commit with no Claude trailer is listed separately (these are
  the commits made through Codex, which adds no trailer).
- Codex line share: added text lines (git numstat, binary files excluded) of
  the trailer-less commits over PATHS, as a fraction of all added text lines
  of the counted commits over PATHS.
- Crutcher Dunnavant: non-merge commits on main at BASE whose author email is
  AUTHOR_CRUTCHER, with their date range.

Not computed here (needs the forge): which PRs received a review. That figure
(46 of 48; #539 and #592 have no review or comment) is transcribed in
validation.md from read-only `gh pr view` queries.

Run from any working directory; prints a JSON object.
"""
from pathlib import Path
import json
import re
import subprocess

BASE = "3fe5f706456a594b7452eff7285ec5836569363a"
PATHS = ["papers/transmon-benchmark", "benchmarks/transmon*"]
FAMILIES = ["Opus", "Fable", "Sonnet"]
AUTHOR_CRUTCHER = "crutcher@gmail.com"
REPO = Path(__file__).resolve().parents[3]


def git(*args):
    return subprocess.run(["git", "-C", str(REPO), *args], check=True, capture_output=True, text=True).stdout


def counts():
    try:
        git("cat-file", "-e", BASE + "^{commit}")
    except subprocess.CalledProcessError as exc:
        raise SystemExit(f"base commit {BASE} not in local history (shallow clone?)") from exc

    prs = set()
    for subject in git("log", "--first-parent", BASE, "--format=%s", "--", *PATHS).splitlines():
        m = re.match(r"Merge pull request #(\d+)", subject) or re.search(r"\(#(\d+)\)$", subject)
        assert m, f"subject without a PR number: {subject}"
        prs.add(int(m.group(1)))

    log = git("log", BASE, "--no-merges", "--format=@@%H%n%(trailers:key=Co-Authored-By,valueonly)", "--", *PATHS)
    family = {f: 0 for f in FAMILIES}
    commits, untrailered = [], []
    for block in log.split("@@")[1:]:
        sha, _, trailers = block.partition("\n")
        commits.append(sha)
        names = [f for f in FAMILIES if re.search(rf"Claude {f}\b", trailers)]
        for f in names:
            family[f] += 1
        if not names:
            assert "Claude" not in trailers, (sha, trailers)
            untrailered.append(sha)

    def added(*revs, log_args=()):
        out = git(*log_args, "--numstat", "--format=", *revs, "--", *PATHS)
        return sum(int(c[0]) for c in (l.split("\t") for l in out.splitlines() if l) if c[0] != "-")

    total = added(BASE, log_args=("log", "--no-merges"))
    codex = sum(added(sha, log_args=("show",)) for sha in untrailered)

    crutcher = git("log", BASE, "--no-merges", f"--author={AUTHOR_CRUTCHER}", "--format=%ad", "--date=short").split()

    return {
        "base": BASE,
        "prs": len(prs),
        "pr_range": [min(prs), max(prs)],
        "commits": len(commits),
        "claude_trailered": len(commits) - len(untrailered),
        "families": family,
        "codex_commits": [sha[:8] for sha in untrailered],
        "codex_added_lines": codex,
        "total_added_lines": total,
        "codex_line_share_pct": round(100 * codex / total, 1),
        "crutcher_commits": len(crutcher),
        "crutcher_dates": [min(crutcher), max(crutcher)],
    }


if __name__ == "__main__":
    print(json.dumps(counts(), indent=2))
