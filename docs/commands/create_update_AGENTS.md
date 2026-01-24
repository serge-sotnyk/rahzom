### One-shot, idempotent **Init/Update `AGENTS.md`** command (drop-in prompt)

> **Role**: You maintain the repository's `AGENTS.md` (the "README for agents").

> **Goal**: If `AGENTS.md` is missing, create a high-quality initial file. If it exists, **minimally update** it to match the project as it is today. Keep authors' wording where possible and avoid large diffs.

**Operating rules (strict):**

* Work only in `AGENTS.md` at the repository root. If the repo is a monorepo and you find clear subprojects, add a short *Monorepo* section describing where additional `AGENTS.md` files **may** live, but do not create them unless explicitly asked.
* Preserve existing section order and tone. Keep unknown sections intact. Only insert missing sections or fix contradictions/outdated commands.
* Prefer factual, concise checklists. No marketing language.
* Do not invent secrets or API keys. Use placeholders (e.g., `YOUR_API_KEY`).
* When multiple equivalent commands exist, prefer ones already used in CI or project scripts.
* At the end, produce the **final Markdown content only** (no commentary), ready to save as `AGENTS.md`.

**Project understanding (auto-discovery):**

* Detect stacks and tools by scanning repo metadata:
  * Python: `pyproject.toml`, `requirements*.txt`, `uv.lock`, `poetry.lock`, `setup.cfg`, `mypy.ini`, `ruff.toml`, `pytest.ini`.
  * JS/TS: `package.json`, lockfiles and scripts.
  * Other ecosystems: `go.mod`, `Cargo.toml`, `pom.xml`, etc.
* Infer canonical commands from scripts/configs (e.g., `pytest`, `ruff`, `mypy`, `npm test`, `pnpm dev`).
* Prefer CI-backed commands (GitHub Actions, Azure DevOps, etc.) over ad-hoc ones if both exist.

**Required `AGENTS.md` structure (add missing sections, reuse existing headings when similar):**

1. **Setup**
   * Languages & versions (only what you detect), package managers (e.g., `uv`, `pip`, `pnpm`, `npm`).
   * Install commands (Python: prefer `uv sync` if `uv` is present; else `pip install -r requirements.txt`).
   * Environment files: point to `.env.example` if present; warn not to commit secrets.

2. **Run / Build**
   * How to start local dev server / main app.
   * Build/package steps when applicable.

3. **Tests**
   * Unit/integration test commands (Python: `pytest -q` if pytest is configured).
   * How to run a subset (e.g., `pytest -k "<expr>"`) if common in repo.

4. **Quality Gates**
   * Lint/format (Python: `ruff check .` and `ruff format --check .` if `ruff` exists; else `black --check .` if Black exists).
   * Type checks (Python: `mypy src/` if configured).
   * State explicitly: "Agents must run all commands in this section and fix issues until they pass."

5. **Code Style & Conventions**
   * Link or summarize key style rules (naming, logging policy, frameworks used—e.g., FastAPI, Pydantic v2—only if detected).
   * Keep bullets short; link to deeper docs instead of duplicating.

6. **Repository Layout**
   * Brief map of important directories (src/tests/config), focusing on what agents actually need.

7. **Monorepo Notes (if applicable)**
   * State where subprojects live and that closest `AGENTS.md` should win if your tools support nesting. Do not generate sub-files here.

8. **Security & Safety**
   * Secrets management (env vars, `.env` patterns).
   * Data/privacy cautions specific to this repo if documented.

9. **PR & Commit Rules**
   * Commit header format (if enforced), required PR checklist items (tests passing, lint/type clean, links to issues).
   * Keep it as a short checklist.

10. **Troubleshooting / Common Tasks (optional, brief)**
    * 3–7 bullets for the most frequent pitfalls (cache clean, local emulator start, etc.), only if they exist in docs/scripts.

**Python-leaning defaults (use only when evidence exists or when no alternative is present):**

* Prefer Python ≥3.9; if `pyproject.toml` declares 3.12/3.13, reflect that exactly.
* If `uv.lock` or `tool.uv` present → show `uv` commands; otherwise fall back to `pip`.
* Prefer `pytest` for tests, `ruff` for lint/format, `mypy` for types if configs are present; otherwise omit or add a TODO bullet.

**Editing policy (minimal-diff):**

* Do not delete author text unless clearly wrong/obsolete; instead, correct the command inline or add a brief bullet replacing it.
* Keep existing headings; if you must add a new section, append it in a logical place.
* If something is unclear, add a single HTML comment `<!-- TODO: explain X -->` rather than speculative content.

**Output format:**

* Return **only** the complete Markdown content of `AGENTS.md`. No explanations, no shell commands to write the file.

**Now perform:**

1. Check whether `AGENTS.md` exists.
2. Create or update it according to the rules above.
3. Produce the final Markdown for `AGENTS.md`.