# Reruns development rules

These instructions apply to every coding assistant working in this repository, including delegated agents. Read them before editing. Keep this file as the shared source of project rules; tool-specific instruction files should point here rather than duplicate it.

## Non-negotiables

- Never create, commit on, push, or open a PR from an AI-branded branch. Tool defaults are not an exception.
- Preserve user changes, original recordings, metadata, and working capture behavior.
- Keep the game responsive. Measure performance claims; do not infer them from the framework or a successful build.
- Write direct, useful code and documentation. No promotional filler, invented results, or assistant branding.
- Follow the requested scope. Do not turn a small fix into an unrelated refactor, redesign, dependency upgrade, or feature expansion.

## Git branches and commits

Branches describe the work, never the assistant, provider, model, or session.

Allowed format: `<type>/<short-kebab-case-task>`.

Allowed types: `feature`, `fix`, `hotfix`, `refactor`, `perf`, `test`, `docs`, `chore`, `build`, `ci`, `release`.

Examples:

- `feature/session-history`
- `fix/clipboard-retry`
- `perf/thumbnail-cache`
- `refactor/audio-routing`
- `docs/linux-installation`
- `release/prepare-v2-0-0`

The entire branch name must match:

```regex
^(feature|fix|hotfix|refactor|perf|test|docs|chore|build|ci|release)/[a-z0-9]+(-[a-z0-9]+)*$
```

Never use assistant branding anywhere in the branch, including inside an otherwise valid prefix. Forbidden examples: `claude/fix-audio`, `codex/update-ui`, `feature/claude-redesign`, `fix/codex-clipboard`, `feature/ai-generated-library`. This applies to other tools and model names too: ChatGPT, Copilot, Gemini, Cursor, OpenAI, Anthropic, GPT, Opus, Sonnet, and equivalents. Slugs must describe the product change, not how it was generated. Avoid meaningless suffixes, random IDs, and names such as `updates` or `improvements`. Check branding without regard to case; do not reject incidental substrings in ordinary words such as `repair`.

Before editing, inspect `git status --short --branch` and `git branch --show-current`. Immediately before every commit, push, or PR creation, validate the actual source branch again, including explicitly named push refs and branches used by remote APIs. A valid prefix does not excuse a branded suffix.

- Do not commit directly to `main` or `master`, or from a detached HEAD. Create an appropriately named task branch first.
- If the current branch has a forbidden name, move the work to a compliant local branch while preserving the current commit and working changes. Rename only a branch you own that is local and unpublished; otherwise create a new branch. Do not rename or delete shared remote branches without authorization.
- If a tool truly enforces a forbidden prefix, do not commit, push, or open a PR under that name. Leave the work uncommitted and report the specific constraint. Do not weaken these instructions to accommodate the tool.
- Inspect the diff and stage only intended paths or hunks. Do not blindly stage unrelated work.
- Keep commits focused and reviewable. Use concise Conventional Commit subjects, such as `fix(audio): preserve mic track when exporting`.
- Do not add generated-by footers, assistant co-author trailers, model names, or tool advertisements to commits, PRs, source comments, or release notes. Preserve legitimate existing authorship and license notices. Never falsify identity.
- Do not bypass hooks or checks with `--no-verify`. Do not force-push, rewrite shared history, or discard changes without explicit authorization.

This file defines required behavior; it is not an installed Git hook. Do not claim branch rules are mechanically enforced unless an actual hook or repository rule has been configured and verified.

## Understand the project before changing it

Reruns is a Windows/Linux desktop replay recorder and clip library. Its main workflow is capture, browse, trim, and share. The UI uses Svelte 5 and Tauri; the native workspace handles capture, media processing, and integration.

Current boundaries:

- `app/src/`: frontend and interaction state.
- `app/src-tauri/`: native application integration, settings, library, and commands.
- `crates/clip-core/`: capture and media logic, independent of the UI.
- `crates/clip-ipc/` and `crates/clip-cli/`: local control protocol and CLI.
- `docs/`: design, platform, release, and operational documentation.

Inspect the current code, manifests, CI, and relevant documentation. Treat old plans and comments as potentially stale. Use `rg` for targeted searches. Trace a behavior from UI to native implementation before changing its contract.

Reuse existing components and utilities when they fit. Keep platform-specific behavior behind clear boundaries. Do not assume an implementation works on Linux because it works on Windows, or vice versa.

## Code size and structure

Prefer the smallest clear implementation that handles the real requirements.

- Functions should have one coherent purpose. Around 60 logical lines is a review signal, not an automatic failure.
- Reassess handwritten modules or Svelte components around 400 lines, or earlier when they mix unrelated responsibilities. Split at meaningful boundaries, not arbitrary line counts.
- Long pipeline definitions, platform bindings, and focused test suites may justify more length. Generated code, lockfiles, and fixtures are outside these review thresholds.
- Do not compress code into dense one-liners to reduce line count. Do not create dozens of trivial wrappers or components to satisfy a quota.
- Prefer explicit names, straightforward control flow, early returns, and small public interfaces. Extract repeated domain logic when it reduces complexity; similar-looking code does not always need an abstraction.
- Separate presentation, state transitions, filesystem access, and media processing. Keep core logic testable without launching the UI.
- Avoid speculative frameworks, generic factories, unnecessary configuration layers, and dependencies for small tasks the existing stack handles well.
- Remove obsolete code and imports made redundant by the change. Do not leave commented-out implementations, duplicate paths, or unused compatibility layers.
- Comments explain a constraint, invariant, workaround, or non-obvious decision. Never narrate obvious statements or write progress reports inside source files.
- Do not hide errors with empty catches, broad suppressions, unchecked casts, or production `unwrap`/`expect` calls without a defensible invariant. Handle expected failure where it can be acted on.

## Performance and media correctness

The application runs beside a game. Protect frame pacing, bounded resource use, and recording integrity.

- Keep blocking I/O, encoding, decoding, probing, and thumbnail work off the UI thread and async executor workers. Use existing bounded workers or appropriate blocking-task facilities.
- Avoid allocation, logging, synchronization, and CPU/GPU copies in per-frame paths unless necessary. Do not hold locks across I/O or an await. Do not sacrifice timestamp or audio synchronization correctness for speed.
- Bound replay retention, caches, queues, and concurrent jobs. Define eviction and cancellation behavior. Prefer backpressure or explicit failure over silently losing retained recordings or growing memory without limit.
- Keep capture independent of window visibility and expensive editing or export work. Account for shared packet references that can retain replay-buffer memory after its normal eviction point.
- Treat zero-copy as a verified capability, not a label. Preserve working fallback paths and expose meaningful failures.
- Prefer existing events and incremental updates over frequent polling and full-library reloads. Use low-frequency health checks only for a concrete recovery need.
- Reuse thumbnails, waveform data, and preview assets. Decode only what is needed. Keep hover preview concurrency bounded and stop invisible playback.
- Release media handles, object URLs, listeners, timers, workers, and temporary files when their owners finish. Cancellation must reach the actual native work.
- In Svelte, keep derived data derived. Avoid effects that duplicate state or create update loops. Choose deep versus shallow reactivity deliberately for large media collections.
- Do not generate filmstrips or waveforms during every drag or resize. Keep pointer updates cheap and direct.
- Preserve originals. Write exports to separate files and finalize safely. Keep edits, sidecar metadata, audio tracks, markers, and source timestamps consistent.
- Storage cleanup must respect favorites, protected edits, configured retention, and ownership. Never silently apply deletion policy to imported folders. Handle full disks, interrupted writes, and partial exports.
- Validate native command inputs. Treat media metadata and paths as untrusted. Pass subprocess arguments as arguments rather than constructing shell commands.
- Keep media access local and access-controlled by default. Do not expose a local media server to the network as a convenience workaround.

For optimization work, establish a baseline and compare the same workload. Record the hardware, OS, settings, and measurement method. Separate tray/UI memory from replay-buffer memory. Report game frame times, capture pacing, startup, or export speed when relevant. Do not claim "zero overhead," "lag-free," or "faster" without evidence and scope.

## UI and behavior

Preserve the established visual direction unless the task requests a change. Keep frequent actions obvious and occasional controls out of the main workflow. Follow the current design tokens rather than adding arbitrary styles per component.

Use semantic controls, visible keyboard focus, readable contrast, adequate hit areas, and reduced-motion support. Preserve library scroll position, navigation state, and in-progress edits. Do not shrink text or scale the whole UI to conceal overflow.

Errors must explain what failed and provide a useful recovery action when possible. Only show success after the operation completes. Clipboard, export, capture, and audio behavior must be real; do not substitute success-shaped mock behavior in production.

## Verification

Run checks appropriate to the changed area and the repository's current CI. Inspect scripts before assuming these commands are still current.

Frontend, from `app/`:

```sh
pnpm check
pnpm build
pnpm test:ui
```

Rust, from the repository root, with the required platform libraries installed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Use relevant focused tests while iterating and required broader checks before finalizing. Add regression tests for nontrivial behavior and fixed defects. Do not add tests that merely duplicate the implementation or brittle assertions for cosmetic changes.

Inspect rendered UI for visual changes at normal, minimum supported, and maximized sizes. Exercise keyboard, loading, failure, and cancellation states when affected. Test representative media aspect ratios.

The Chromium harness mocks the Tauri bridge. Passing it does not prove native capture, clipboard, audio, encoding, or either system webview works. Verify native behavior on available hardware and state exactly what remains untested. Never report a check as passed unless it ran and passed. Do not weaken tests to hide a failure.

## Writing: source, UI, README, PRs, and release notes

Every sentence must communicate behavior, a decision, an instruction, a limitation, or useful evidence. Delete sentences that only announce effort or praise the result.

- Use plain words and concrete verbs. Name the affected action and what changes for the user.
- Never use generic AI filler: "delve," "leverage," "seamless experience," "robust solution," "game-changing," "elevate your workflow," "beautifully crafted," or "various improvements." Use exact technical terms when they carry real meaning.
- No assistant introductions, self-congratulation, emoji headings, decorative badge collections, fake testimonials, invented benchmarks, or claims such as "production-ready" without a defined basis.
- Avoid canned "Overview / Key Features / Why Choose Us / Getting Started / Conclusion" scaffolding. Choose only sections the content needs. Ordinary useful headings such as Installation or Usage are fine.
- Do not repeat the same information in a heading, opening sentence, bullets, and conclusion. Do not add a concluding recap to a short document.
- Keep README content focused on what Reruns does, supported platforms and limitations, installation, actual usage, and the commands needed to develop it. Keep deep internals and long plans in the relevant docs.
- Release notes describe shipped changes since the preceding release. Include platform labels, migration steps, or known limits when useful. Exclude unshipped plans, internal task lists, test inventories, and unrelated refactors.
- PR descriptions explain the problem, resulting behavior, relevant validation, and material limitations. Small changes need short descriptions. Omit empty template sections, implementation diaries, and lists of every touched file.
- Update documentation affected by a change; do not create another summary document for every task. Keep required project records concise and current.

Illustrative wording patterns, not claims about existing releases:

- Bad: "Enhanced the audio experience with robust improvements."
- Good: "Clips keep microphone audio after the headset reconnects."
- Bad: "Optimized performance for a seamless browsing experience."
- Good: "Reuses cached thumbnails when reopening the library."
- Bad: "Various bug fixes and quality-of-life enhancements."
- Good: "Retrying a failed clipboard copy no longer exports the clip twice."

## Finishing work

Review the diff for unrelated changes, unnecessary code, placeholder behavior, and filler copy. Revalidate branch naming before any commit, push, or PR action. Report what changed, what was actually checked, and any remaining limitation. Keep the report proportional to the task.
