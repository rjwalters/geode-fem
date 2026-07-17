"""Skill-local lib for ``anvil:ip-uspto`` (first module: issue #445).

The skill directory name is hyphenated (``ip-uspto``), so this package is
NOT importable via a dotted ``python -m`` path. Consumers invoke modules
by direct file path (the project-migrate / project-share precedent), e.g.:

    python3 anvil/skills/ip-uspto/lib/inventorship_interview.py --help

and tests load modules by file path via ``importlib`` under a unique
module name (see ``tests/test_ip_uspto_inventorship_interview.py``).

Per the lib-promotion convention (CLAUDE.md "skill-local first, lib
promotion later"), modules here move to ``anvil/lib/`` only once a second
skill consumes them.

- ``inventorship_evidence.py`` (#445) was **promoted to**
  ``anvil/lib/inventorship_evidence.py`` in issue #516 once
  ``anvil:ip-uspto-provisional``'s inventorship-lite pass became its second
  consumer. It is deliberately consumer-agnostic (repo path + element->paths
  map inputs; no BRIEF or claims parsing). This skill's ``--evidence`` mode
  and ``inventorship_interview.py`` now reference the promoted location.
- ``inventorship_interview.py`` (#493/#511) — interview-packet templating +
  ``--synthesize`` determination parsing — stays **skill-local**: it is
  judgment-laden and consumes v1 artifacts (``inventorship_map.json`` +
  ``evidence.jsonl``); no second consumer yet. It loads
  ``is_vendored_path`` from the promoted ``anvil/lib/inventorship_evidence.py``
  by file path.
"""
