---
name: deck-imagegen-onboarding
description: Consumer onboarding walkthrough for `deck-imagegen` adapters. Five-minute smoke test with the shipped placeholder backend, the two importability layouts for the `<module>:<attr>` spec, the adapter-owned short-lived-token auth bootstrap pattern for cloud backends, the failure/retry semantics recap, and a porting checklist for consumers with an existing in-house image worker.
---

# deck-imagegen-onboarding — Consumer adapter walkthrough

`commands/deck-imagegen-adapter.md` is the **contract**; this document is the **walkthrough**. Read this when you are wiring your first adapter and want to go from zero to generated PNGs without reverse-engineering `anvil/skills/deck/lib/imagegen.py`.

## Five-minute smoke test (shipped placeholder backend)

Anvil ships exactly one backend: a deterministic placeholder-PNG generator whose only job is to prove the wiring. It produces 1280x720 solid-color PNGs (color derived from `sha256(prompt + style + steps)`) using only the Python stdlib. Use it to verify the full `config → importlib → dispatch → journal` path before writing a line of your own adapter.

1. **Register the placeholder backend.** In your repo root, in `.anvil/config.json` (the shared versioned consumer config surface — merge into the existing file if you already carry the #426 git knob or #427 figure adapters):

   ```json
   {
     "version": 1,
     "deck": {
       "imagegen": {
         "backend": "anvil.skills.deck.lib.placeholder_backend:PlaceholderBackend"
       }
     }
   }
   ```

   This dotted path resolves when the directory containing `anvil/` (your repo root in a development checkout; `.anvil/anvil/` in a consumer install) is on `sys.path` — which it is whenever commands run from the repo root. If the import fails in your environment, see "Importability" below for the supported layouts.

2. **Opt the thread in.** In `<thread>/BRIEF.md` frontmatter:

   ```yaml
   imagery_policy: generative-eligible
   imagery_style: editorial-photography
   ```

3. **Mark one slot.** In the latest `<thread>.{N}/deck.md`, above an image reference:

   ```markdown
   <!-- anvil-imagegen: hero -->
   ![hero](assets/generated/hero.png)
   ```

   And give the slot a prompt — either a sidecar file `assets/generated/hero.prompt.md` or a `## Imagery prompt: hero` section in `speaker-notes.md`.

4. **Run `deck-imagegen <thread>`.**

5. **Inspect the output.**
   - `<thread>.{N}/assets/generated/hero.png` — a solid-color 16:9 PNG. The color is a hash of your prompt+style+steps, so editing the prompt and re-running visibly changes the placeholder.
   - `<thread>.{N}/assets/_prompts.json` — the prompt journal. The `backend` field records the dotted path verbatim; the `prompt` field shows the final composed prompt (style-preset prefix + your prompt + shared suffix). This journal is also the idempotence key: re-running with an unchanged prompt+style+steps is reported `skipped-unchanged` and never calls the backend.
   - `<thread>.{N}/_progress.json` — `phases.imagegen.state = done`.

6. **(Optional) Exercise the failure path.** Put the token `ANVIL-FORCE-FAIL` anywhere in a slot's prompt and re-run. The placeholder backend raises `BackendError` for that slot; `deck-imagegen` writes `assets/generated/<slot>.png-FAILED.md` with the error body, continues with the other slots, and records `phases.imagegen.state = partial`. Remove the token and re-run: the slot regenerates and the stub is cleaned up. This is exactly how a real backend failure behaves — you have now seen the entire failure-containment story without a cloud account.

When you can complete this loop, swap the `backend =` line for your own adapter and everything else stays the same.

## Importability — where your adapter module can live

The `backend = "<module>:<attr>"` spec is resolved with `importlib.import_module(<module>)` **in the venv/interpreter that runs the `deck-imagegen` command**, followed by `getattr` for `<attr>`. Two layouts work:

1. **Installed package** (recommended for teams): your adapter ships in a package installed into the venv (`pip install my-imagery-adapter`), registered as e.g. `my_imagery_adapter.backend:FluxBackend`. Most robust — no path manipulation.
2. **Repo-local module on `PYTHONPATH`**: a module file in your repo (e.g., `tools/imagery_adapter.py` with `tools/__init__.py`, registered as `tools.imagery_adapter:Backend`), importable because the repo root is the working directory / on `PYTHONPATH`. Simplest for a single-repo studio.

Misconfiguration produces a specific `ImagegenError`, surfaced verbatim by the command — match the message to the fix:

| Symptom (`ImagegenError` message) | Cause | Fix |
|---|---|---|
| `missing ``:`` separator. Expected ``<module>:<attribute>``` | Spec has no colon | Use `module.path:Attr`, not `module.path.Attr` |
| `cannot import module '<module>': …` | Module not importable in the venv running the command | Install the package / fix `PYTHONPATH` / check layout above |
| `module '<module>' has no attribute '<attr>'` | Typo in the attribute name, or the symbol isn't exported | Match the class/function name exactly |
| `resolved to class '<attr>' but constructing it with zero arguments raised: …` | Your class constructor requires arguments | Class-form adapters need a zero-arg constructor; move config to env vars or module level |
| `resolved attribute is neither callable nor has a ``generate`` method` | The attribute is a plain object without the contract surface | Expose `generate(prompt, style, steps) -> bytes` or register a callable |
| `no ``deck.imagegen.backend`` registered in …` | Config file exists but the key is missing | Add the `deck.imagegen.backend` key to `.anvil/config.json` |
| `MIGRATION REQUIRED (#442): … config.toml still contains a [deck.imagegen] registration …` | Pre-#442 install with a stale `.anvil/config.toml` registration | Paste the JSON snippet from the error into `.anvil/config.json`, then delete the `[deck.imagegen]` section from `config.toml` |

## Auth bootstrap for cloud backends

This is the part first-time consumers most often get wrong, so the pattern is spelled out: **the adapter owns token acquisition and refresh; anvil never sees auth.** `deck-imagegen` reads `.anvil/config.json` for the dotted path and nothing else — no env-var conventions, no `.env` sourcing, no OAuth.

For backends fronted by short-lived cloud tokens (GCP-style identity tokens, STS credentials, etc.), the recommended shape: the **constructor acquires the first token**, and **`generate` checks expiry (with clock skew) and refreshes before each call**. Auth failure *after* a refresh attempt raises `BackendError` — that is a real failure of this slot's generation, not something anvil can fix by retrying.

```python
# tools/imagery_adapter.py — provider-neutral skeleton (GCP-token-shaped)
import time
import requests

class BackendError(Exception):
    """Local definition is fine: deck-imagegen catches any exception
    with `BackendError` in its MRO class-name list."""

_SKEW_SECONDS = 60  # refresh this long BEFORE nominal expiry

class CloudImageBackend:
    def __init__(self) -> None:
        # Zero-arg constructor (the class-form contract). Acquire the
        # first short-lived token eagerly so misconfigured credentials
        # fail at adapter-load time with a clear message, not mid-deck.
        self._session = requests.Session()
        self._token: str | None = None
        self._expires_at: float = 0.0
        self._refresh_token()

    def _refresh_token(self) -> None:
        try:
            # Provider-specific: metadata server, workload identity,
            # `gcloud auth print-identity-token`, STS exchange, …
            token, ttl_seconds = self._acquire_token_from_provider()
        except Exception as exc:
            raise BackendError(f"auth bootstrap failed: {exc}") from exc
        self._token = token
        self._expires_at = time.monotonic() + ttl_seconds

    def _ensure_fresh_token(self) -> None:
        if time.monotonic() >= self._expires_at - _SKEW_SECONDS:
            self._refresh_token()

    def generate(self, prompt: str, style: str, steps: int | None) -> bytes:
        self._ensure_fresh_token()  # adapter-owned refresh, every call
        resp = self._session.post(
            "https://image-worker.internal.example/generate",
            headers={"Authorization": f"Bearer {self._token}"},
            json={"prompt": prompt, "style": style, "steps": steps},
            timeout=120,
        )
        if resp.status_code == 401:
            # One refresh-and-retry on auth rejection; if it still
            # fails, the slot fails.
            self._refresh_token()
            resp = self._session.post(  # …same call again…
                "https://image-worker.internal.example/generate",
                headers={"Authorization": f"Bearer {self._token}"},
                json={"prompt": prompt, "style": style, "steps": steps},
                timeout=120,
            )
        if resp.status_code != 200:
            raise BackendError(f"image worker returned {resp.status_code}: {resp.text[:500]}")
        # deck-imagegen accepts PNG / JPEG / WebP and transcodes JPEG/WebP
        # to PNG on disk (issue #564). Adapters do not need to enforce
        # PNG-only on the bytes coming back from their worker.
        return resp.content
```

Why this shape:

- **Eager first acquisition in `__init__`** turns "credentials are wrong" into an adapter-load failure (`ImagegenError`, run aborts cleanly before any slot dispatches) instead of N per-slot failures.
- **Expiry-with-skew check in `generate`** means a long multi-slot run survives token expiry mid-run without any anvil involvement. anvil dispatches slots serially, so a 30-slot deck against a 15-minute token *will* cross an expiry boundary — the adapter must own this.
- **`BackendError` only after refresh is exhausted** keeps the per-slot containment semantics honest: a raised `BackendError` means "this slot genuinely failed," producing a `*-FAILED.md` stub and a `partial` verdict rather than aborting the run.

No auth code enters anvil; this skeleton lives entirely in your repo.

## Failure and retry semantics (recap)

The full spec is in `commands/deck-imagegen-adapter.md` § "Non-goals" and `commands/deck-imagegen.md` § "Failure modes". The operational summary:

- **Anvil never retries.** One `generate` call per slot per run. Retry/backoff (transient network errors, 429s with `Retry-After`, provider flakiness) lives inside your adapter; raise `BackendError` only when your retry budget is exhausted.
- **Per-slot containment.** A `BackendError` (or unrecognized-image-format bytes) on one slot writes `assets/generated/<slot>.png-FAILED.md` and the run continues. `phases.imagegen.state` is `partial` when at least one slot succeeded, `failed` when every slot failed. JPEG and WebP bytes are auto-transcoded to PNG on disk (issue #564) — they are NOT per-slot failures.
- **Stubs clean up on later success.** Fix the cause, re-run `deck-imagegen`, and a succeeding slot deletes its stale `*-FAILED.md` stub.
- **Non-`BackendError` exceptions propagate** and abort the run — they indicate a bug in adapter glue, not a generation failure. Wrap everything your provider can throw.
- **Idempotence is journal-keyed.** Unchanged prompt+style+steps with an existing PNG → `skipped-unchanged`, zero backend calls. Changing any element of the contract re-dispatches that slot only.

## Porting checklist — existing in-house image worker

For consumers with a working image pipeline (e.g., a slides skill calling an in-house Flux 1 Schnell worker):

- [ ] **Map your call into `generate(prompt, style, steps) -> bytes`.** Your existing "send prompt, get image" function becomes the body of `generate`. Return the raw bytes your worker produces — anvil accepts PNG, JPEG, or WebP and transcodes JPEG/WebP to PNG on disk (issue #564). If you're returning JPEG or WebP, install the optional extra: `pip install 'anvil[deck_imagegen]'`. PNG-native adapters need no extras.
- [ ] **Fold model routing onto `style`.** If your worker takes a model or LoRA selector, derive it from the `style` preset key (e.g., `documentary` → photo model, `diagram` → graphic model). The prompt already includes the preset's prose prefix; `style` is the routing hint.
- [ ] **Fold step counts onto `steps`.** `steps=None` means "your default" — map it to whatever your worker's default inference-step count is. Per-slide overrides arrive via `<!-- anvil-imagegen: <slot> steps=N -->` markers.
- [ ] **Move auth into the adapter** per "Auth bootstrap" above (constructor bootstraps, `generate` refreshes).
- [ ] **Move retry into the adapter**; raise `BackendError` only on exhaustion.
- [ ] **Register the dotted path** under `deck.imagegen.backend` in `.anvil/config.json` and re-run the five-minute smoke test against YOUR adapter (including one `ANVIL-FORCE-FAIL`-style induced failure of your own, e.g., an unreachable worker URL, to confirm the stub path).
- [ ] **Note for slides-skill migrants**: `anvil:deck` is the imagegen-capable presentation class. `anvil:slides` (technical talks) deliberately has no imagegen path — its figures are data-derived (`slides-figures`: mermaid/matplotlib). Decks that need generative imagery are authored with `anvil:deck`; see `anvil/skills/slides/SKILL.md` § "Generative imagery".

## Cross-references

- `commands/deck-imagegen-adapter.md` — the adapter **contract** (signature, registration, non-goals, anvil's responsibility boundary).
- `commands/deck-imagegen.md` — the dispatching command (gates, procedure, failure-mode table).
- `anvil/skills/deck/lib/placeholder_backend.py` — the shipped reference backend used in the smoke test.
- `assets/imagery-style-presets.md` — the style preset library and prompt-composition rules.
