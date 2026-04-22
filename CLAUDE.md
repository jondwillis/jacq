# jacq — notes for Claude Code sessions

## Supply-chain workflow

This project uses `cargo-deny` for *enforced* supply-chain hygiene (advisories,
licenses, bans, sources) and `cargo-vet` for *opportunistic* auditing. Only
`cargo-deny` is gated in CI; `cargo-vet` exists to give the audit backlog
structure without blocking PRs.

Before suggesting any dependency change:

1. Run `cargo deny check` and confirm it still passes. This is the CI gate.
2. **Do NOT add git-based dependencies.** `deny.toml` sets
   `unknown-git = "deny"`; any `git = "..."` entry in Cargo.toml will break
   CI. If you think a git dep is the only option, surface it — don't bypass.
3. **Do NOT add alternate registries.** Only crates.io is allowed.
4. **New deps must use a license in `deny.toml`'s `[licenses] allow` list.**
   If a new dep pulls in an unfamiliar license, flag it rather than silently
   extending the allow list. The list is deliberately conservative.
5. **New deps must be auditable.** Prefer crates that (a) are audited by
   Google/Mozilla/Bytecode Alliance/Embark/Zcash (see `supply-chain/config.toml`
   imports), or (b) have a small, reviewable surface. Large C-binding or
   proc-macro-heavy deps require explicit justification.
6. **The Rust toolchain is pinned in `rust-toolchain.toml`.** Bumping it is a
   deliberate commit, not a side effect of another change.

## cargo-vet specifically

- `cargo-vet` is **not gated in CI**. Running it is voluntary.
- The `supply-chain/config.toml` exemptions list is a **visible audit backlog**,
  not a rubber stamp. After trusted imports, ~122 crates remain unaudited.
- **Do NOT run `cargo vet regenerate exemptions`** as a habit on dep bumps.
  That converts "I audited nothing new" into "I audited everything," which is
  exactly the failure mode this setup prevents. Instead:
  - Use `cargo vet certify <crate>` to record an audit you actually did.
  - Use `cargo vet prune` after adding new trusted imports — it only removes
    exemptions that are already covered by an import.
  - Add a targeted `[[exemptions.<crate>]]` entry with a justification comment
    if you need to temporarily accept an un-audited crate.
- Running `cargo vet prune` will rewrite `supply-chain/config.toml` and strip
  any freeform comments in it. Keep policy notes here, not there.

## Why comrak is not a dependency

`comrak` was in the original Phase 1 scaffold for parsing skill/agent markdown
frontmatter (see `PLAN.md:205,216,258`) but was deliberately dropped — see
`docs/learning-guide.md:250-265` ("No Comrak for Frontmatter"). Frontmatter is
handled by a ~15-line `split_frontmatter()` function. Do not re-add comrak
without a concrete use case; it pulls in `syntect` by default, which drags in
two unmaintained advisories (`bincode 1.3.3`, `yaml-rust 0.4.5`).

## Dev containers

`.devcontainer/devcontainer.json` defines the canonical build environment and
bootstraps `cargo-deny` + a `cargo fetch` warm-up in `postCreateCommand`. If
you need `cargo-vet` locally, install on-demand: `cargo binstall cargo-vet`.

### What the container is isolating

The devcontainer serves two distinct isolation goals, both load-bearing:

1. **Agent / build-script containment.** Claude Code file writes, `cargo build`
   scripts, and proc-macros execute inside the container FS, not against the
   host. A malicious `build.rs` in a dep can touch `/workspaces/jacq` and the
   cargo cache volume, but cannot reach `~/.ssh`, browser profiles, shell
   history, cloud credentials, or anything else on the host — those paths
   simply do not exist inside the container.
2. **Supply-chain attack surface reduction.** This is the containment layer
   Kerkour's article recommends as the single highest-ROI mitigation. Combined
   with `deny.toml`'s `unknown-git = "deny"` (blocks unreviewed git sources)
   and `cargo-vet`'s trusted imports (audits for 42 of the 165 transitive
   deps), the container bounds the blast radius of any compromise that slips
   through the other gates.

### Why persistent cache volumes are intentional

The `mounts` block defines named Docker volumes (`jacq-cargo-cache`,
`jacq-target-cache`) that survive container rebuilds. This is a **deliberate
tradeoff** and should not be "fixed" by removing them:

- ✓ Warm cache across rebuilds — `cargo build` stays fast day-to-day.
- ✓ The isolation boundary is still the container root, not individual volumes.
  A compromised build script can write to the cargo cache volume, but it
  still cannot escape to the host FS.
- ✗ Weakens clean-slate semantics — a persisted cache means a rebuild is not
  fully pristine. Accept this for dev UX.

**To force a clean slate when you need one** (e.g., after reviewing a
suspicious dep, or to reproduce a fresh-environment bug):

```bash
docker volume rm jacq-cargo-cache jacq-target-cache
# then "Rebuild Container" in VS Code or `devcontainer up --workspace-folder .`
```

### Why the warm-up uses `cargo fetch`, not `cargo build`

`postCreateCommand` ends with `cargo fetch`. This pre-populates the registry
volume with dep source trees so the first real in-container `cargo build`
after a rebuild isn't cold-starting an empty cache. Crucially, `cargo fetch`
only **downloads tarballs** — it does NOT invoke `build.rs` scripts or
proc-macros. A `cargo build` warm-up would execute build scripts during
container creation, before you've had a chance to review anything, which
would defeat the containment purpose. Keep the warm-up as `fetch`.

## CI

`.github/workflows/ci.yml` runs `fmt --check`, `clippy -D warnings`, `test`,
and `cargo deny check` (via `EmbarkStudios/cargo-deny-action@v2`). The `deny`
job also runs on a weekly cron (Mondays 13:00 UTC) so newly published RustSec
advisories against unchanged deps break CI without requiring a code change.

## Scope decisions explicitly skipped

These are in the Kerkour supply-chain article but deliberately not adopted
here; revisit if the threat model changes:

- **`cargo-audit` standalone** — redundant with `cargo-deny`'s `advisories`
  section.
- **`[patch.crates-io]` git-SHA pinning of all deps** — high-maintenance
  nuclear option, wrong threat model for a local plugin compiler.
- **CI-cut signed release pipeline (cosign / SLSA)** — no releases exist.
  Revisit when publishing to crates.io.
- **`cargo vet` as a CI gate** — see reasoning above.
