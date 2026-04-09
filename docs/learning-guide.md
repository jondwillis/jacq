# jacq Learning Guide

A companion guide for understanding jacq's architecture, Rust patterns, and compiler design decisions as the project evolves. Each phase adds a new section.

---

## Phase 1: IR Types & Project Foundation

### What is an IR (Intermediate Representation)?

Compilers work in stages. Instead of going directly from source to output, they parse source into an intermediate data structure that's easier to analyze and transform. In traditional compilers:

```
Source Code → [Parser] → AST/IR → [Analyzer] → [Code Generator] → Output
```

jacq follows the same pattern, but for plugin definitions:

```
plugin.yaml or plugin.json → [Parser] → PluginIR → [Analyzer] → [Emitters] → Claude Code / OpenCode / Codex output
```

The `PluginIR` struct is jacq's AST. It holds everything the compiler knows about a plugin after parsing: the manifest metadata, all discovered skills, agents, hooks, MCP servers, instructions, and per-target overrides.

### Why a Superset (not a Separate Format)?

jacq's core design principle is: **a valid Claude Code plugin is already valid IR input**. This is the same relationship TypeScript has with JavaScript — every JS program is a valid TS program. The benefits:

1. **Zero migration cost** — existing Claude Code plugins work immediately
2. **Gradual adoption** — add IR-specific fields (targets, capabilities, fallbacks) when you need them
3. **One canonical source** — Claude Code output from the IR is identical to the original

This is implemented via `#[serde(default)]` on all IR-specific fields in `PluginManifest`. When parsing a Claude Code plugin.json that has no `ir_version`, `targets`, or `requires` fields, serde fills them with `None`/`Vec::new()`/`BTreeMap::new()`.

### Rust Patterns Used in Phase 1

#### 1. Algebraic Data Types for the IR

Rust enums with associated data model the IR naturally:

```rust
pub enum Author {
    Name(String),                                    // "Jon Willis"
    Structured { name: String, email: Option<String> }, // { name: "Jon", email: "..." }
}
```

This is an **algebraic data type** (ADT) — a type that can be one of several variants, each carrying different data. In TypeScript you'd use a discriminated union (`type Author = string | { name: string; email?: string }`). In Rust, the compiler enforces that you handle every variant when you `match` on it.

#### 2. Serde's Untagged Enums

```rust
#[serde(untagged)]
pub enum Author {
    Name(String),
    Structured { name: String, email: Option<String> },
}
```

The `#[serde(untagged)]` attribute tells serde to try each variant in order when deserializing. For JSON `"Jon Willis"`, it tries `Name(String)` first — success. For `{"name": "Jon"}`, `Name(String)` fails (not a string), so it tries `Structured` — success.

**Gotcha**: Order matters with untagged enums. If `Structured` came first, a plain string would fail both variants because serde tries them in declaration order.

#### 3. Serde's `from`/`into` for Custom Parsing

```rust
#[serde(from = "String", into = "String")]
pub struct Capability {
    pub category: CapabilityCategory,
    pub feature: Option<String>,
}
```

This tells serde: "To deserialize a `Capability`, first deserialize a `String`, then convert it via `From<String> for Capability`." This lets us parse `"hooks.pre-tool-use"` as a structured type with `category: Hooks, feature: Some("pre-tool-use")`.

The `into = "String"` does the reverse for serialization. Together they provide a clean YAML/JSON interface (`"hooks.pre-tool-use"`) while the Rust code works with structured types that the compiler can check.

#### 4. Serde's `flatten` for Forward Compatibility

```rust
pub struct SkillFrontmatter {
    pub description: Option<String>,
    // ...known fields...

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}
```

`#[serde(flatten)]` captures any YAML/JSON keys that don't match known fields into the `extra` map. This means jacq won't reject a skill file just because it uses a frontmatter field we haven't modeled yet. Forward compatibility for free.

#### 5. `rename_all` vs. Explicit `rename`

```rust
#[serde(rename_all = "kebab-case")]
pub enum Target {
    ClaudeCode,           // → "claude-code" ✓
    #[serde(rename = "opencode")]
    OpenCode,             // would be "open-code" without explicit rename
    Codex,                // → "codex" ✓
}
```

`rename_all = "kebab-case"` splits on camelCase boundaries: `OpenCode` → `Open` + `Code` → `open-code`. But the actual target name is `opencode` (one word). TDD caught this — the `parse_full_ir_manifest_yaml` test failed because serde expected `open-code` in the YAML. The fix: explicit `#[serde(rename = "opencode")]` overrides the `rename_all` rule for specific variants.

**Lesson**: `rename_all` is a convenience, not a guarantee. When your naming convention doesn't match the camelCase splitting rule, override explicitly.

#### 6. Capability Matrices as Data

```rust
pub fn capability_matrix(target: Target) -> CapabilityMatrix {
    match target {
        Target::ClaudeCode => claude_code_matrix(),
        Target::OpenCode => opencode_matrix(),
        // ...
    }
}
```

The capability matrices are embedded as Rust data (BTreeMaps), not loaded from config files. This means:
- They're type-checked at compile time
- Adding a new `Target` variant forces you to add a match arm (exhaustive matching)
- They ship as part of the binary — no runtime file loading

The trade-off: updating a matrix requires recompiling jacq. But these change infrequently and correctness matters more than hot-reloading here.

### The Compiler's Type System IS the Schema

In a TypeScript/Zod approach, you'd have:
1. A Zod schema (runtime validation)
2. TypeScript types (compile-time checking)
3. Hope that they stay in sync

In Rust with serde:
1. The struct definition IS the schema
2. `#[derive(Deserialize)]` generates the parser from the struct
3. They cannot diverge

This is why the plan chose Rust — the type system and the validation layer are the same thing.

---

## Quiz: Phase 1

Test your understanding of the patterns and decisions in Phase 1.

### Q1: Superset Design
Why does jacq's IR use `#[serde(default)]` on fields like `ir_version`, `targets`, and `requires`?

<details>
<summary>Answer</summary>

So that a Claude Code `plugin.json` (which doesn't have these fields) can still be deserialized as a `PluginManifest`. The missing fields get default values (`None`, empty `Vec`, empty `BTreeMap`), preserving the "valid Claude Code plugin IS valid IR" principle. Without `#[serde(default)]`, parsing a plugin.json would fail with "missing field" errors for every IR-specific field.
</details>

### Q2: Untagged Enum Ordering
What would break if we swapped the variant order in the `Author` enum?

```rust
#[serde(untagged)]
pub enum Author {
    Structured { name: String, email: Option<String> },  // moved first
    Name(String),                                          // moved second
}
```

<details>
<summary>Answer</summary>

Nothing would break in this specific case. Serde tries variants in order: for `"Jon Willis"` (a string), `Structured` would fail (expects an object), then `Name(String)` would succeed. For `{"name": "Jon"}`, `Structured` would succeed on the first try.

However, if both variants could match the same input, order would matter. For example, if one variant used `serde_yaml::Value` (which matches anything), it would consume all inputs and the later variants would never be tried. The general rule: put more specific variants first.
</details>

### Q3: Exhaustive Matching
If we add a new target `Target::Windsurf`, which parts of the code would fail to compile?

<details>
<summary>Answer</summary>

Every `match target { ... }` expression without a wildcard `_` arm:
1. `capability_matrix()` in `targets.rs` — needs a `Target::Windsurf` arm
2. `Target::as_str()` — needs to return `"windsurf"`
3. `Target::all()` — needs the variant in the array
4. `Target::FromStr` in `cli.rs` — needs `"windsurf" => Ok(Target::Windsurf)`

This is the core value of Rust's exhaustive matching: the compiler tells you everywhere that needs updating when the data model changes. In TypeScript/Python, you'd have to grep and hope.
</details>

### Q4: The TDD Catch
The test `parse_full_ir_manifest_yaml` failed because serde expected `"open-code"` but the YAML had `"opencode"`. Why did `rename_all = "kebab-case"` produce `"open-code"`?

<details>
<summary>Answer</summary>

`rename_all = "kebab-case"` splits identifiers on camelCase boundaries before inserting hyphens. `OpenCode` has two uppercase-initiated segments: `Open` and `Code`, producing `open-code`. The variant name `Codex` has no internal boundary, so it stays `codex`. The fix was `#[serde(rename = "opencode")]` to override the automatic rule for variants where the kebab-case split doesn't match the desired string.
</details>

### Q5: Forward Compatibility
A plugin author adds a custom field `requires-confirmation: true` to their skill frontmatter. What happens when jacq parses it?

<details>
<summary>Answer</summary>

The field is captured in `SkillFrontmatter.extra` as a `BTreeMap<String, serde_yaml::Value>` entry: `("requires-confirmation", Value::Bool(true))`. The `#[serde(flatten)]` attribute routes any unrecognized fields to `extra` instead of failing. This means jacq won't reject plugins that use frontmatter fields it doesn't know about yet — forward compatibility without schema updates.
</details>

### Q6: Why BTreeMap Instead of HashMap?

jacq uses `BTreeMap` everywhere instead of `HashMap`. Why?

<details>
<summary>Answer</summary>

`BTreeMap` is sorted by key. This means:
1. **Deterministic serialization** — when jacq emits YAML/JSON, the keys are always in the same order. This makes snapshot tests stable and diffs readable.
2. **Deterministic iteration** — capability matrices are compared and displayed in consistent order.
3. **`Ord` requirement** — `BTreeMap` keys need `Ord`, not `Hash`. The `Target` enum derives `Ord` (alphabetical by variant), which works naturally as a `BTreeMap` key.

`HashMap` would be slightly faster for lookups, but jacq's maps are small (10-20 entries). Determinism matters more than microseconds here.
</details>

### Q7: Design Decision
Why are capability matrices embedded in Rust code rather than loaded from YAML/TOML config files?

<details>
<summary>Answer</summary>

Three reasons:
1. **Compile-time safety** — if you add a `Target::Windsurf` variant, you're forced to add its matrix. A config file can't enforce this.
2. **Single binary distribution** — `curl | sh` installs jacq with no config files to manage.
3. **Versioned correctness** — the matrices are part of the jacq release. Users on jacq 0.1.3 get the matrices that were tested with 0.1.3. External configs could drift.

The trade-off is that updating a matrix requires a jacq release. This is acceptable because capability changes to target platforms are infrequent and should be validated before shipping.
</details>

---

## Phase 2: Parser

### What the Parser Does

The parser's job is to read a plugin directory from disk and produce a `PluginIR` — the in-memory AST that downstream phases (analyzer, emitters) consume. It handles two input formats:

1. **Claude Code native**: `.claude-plugin/plugin.json` + `commands/*.md`
2. **IR format**: `plugin.yaml` + `skills/*.md` + `agents/*.md` + `hooks/*.yaml` + `mcp/*.yaml` + `instructions/*.md` + `targets/*/`

The parser auto-detects which format by checking which manifest file exists (IR format takes priority).

### Key Design Decisions

#### 1. No Comrak for Frontmatter

The plan originally called for comrak (a CommonMark parser) to handle frontmatter. We dropped it. YAML frontmatter is a simple format:

```
---
key: value
---
markdown body
```

A 15-line `split_frontmatter()` function handles this reliably. Comrak would add complexity for no benefit — it parses markdown ASTs, but we don't need to understand the markdown structure, only split it from the YAML header.

**Lesson**: Don't reach for a library when the problem is simpler than the library's domain. Frontmatter extraction is string splitting, not markdown parsing.

#### 2. Convention Over Configuration

The parser discovers capabilities from directory structure, not from manifest declarations:

- `skills/*.md` → skills
- `commands/*.md` → skills (Claude Code calls them "commands")
- `agents/*.md` → agents
- `hooks/*.yaml` → hooks
- `mcp/*.yaml` → MCP servers
- `instructions/*.md` → instructions
- `targets/<name>/*` → per-target overrides

This is the Next.js pattern: the filesystem IS the configuration. The manifest declares metadata and cross-platform concerns (targets, capabilities, fallbacks), but the plugin's actual content is discovered by walking directories.

#### 3. Sorted Output for Determinism

Every parser function sorts its output by name before returning. This means the same plugin directory always produces the same `PluginIR` regardless of filesystem enumeration order. This matters for:
- Snapshot tests (insta) — same input = same output
- Diffing build output — deterministic generation
- Cross-platform consistency — macOS and Linux may enumerate differently

#### 4. Test Fixtures as Specification

The `tests/fixtures/` directory contains synthetic plugins that serve as both test data and format specification:

- `claude-code-plugin/` — minimal Claude Code native format
- `ir-plugin/` — full IR format with all feature types
- `bad-frontmatter/` — malformed YAML in a skill file
- `empty-dir/` — no manifest at all

Plus we test against the real `notes-app-plugin` in `/Volumes/Sidecar/` — this is the dogfooding test that proves jacq can parse a real-world Claude Code plugin.

### Rust Patterns Used in Phase 2

#### 1. `walkdir` for Recursive Directory Traversal

```rust
for entry in WalkDir::new(&search_dir)
    .min_depth(1)
    .max_depth(2)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
{
```

`walkdir` is the standard Rust crate for filesystem traversal. Key details:
- `min_depth(1)` skips the directory itself
- `max_depth(2)` prevents unbounded recursion
- `filter_map(|e| e.ok())` silently skips entries we can't read (permission errors)
- `is_some_and()` (stable since Rust 1.70) is cleaner than `map_or(false, |ext| ...)`

#### 2. `Path::strip_prefix` for Relative Paths

```rust
let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();
```

Plugin content stores relative paths (e.g., `commands/greet.md`) rather than absolute paths. This keeps the IR portable — it doesn't encode the machine-specific location where the plugin was loaded from.

#### 3. `env!("CARGO_MANIFEST_DIR")` for Test Fixtures

```rust
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
```

`CARGO_MANIFEST_DIR` is set by Cargo at compile time to the directory containing `Cargo.toml`. This means tests find fixtures regardless of the working directory — `cargo test` works from any location.

#### 4. Graceful Skipping for Environment-Dependent Tests

```rust
#[test]
fn parse_notes_app_plugin() {
    let path = Path::new("/Volumes/Sidecar/notes-app-plugin");
    if !path.exists() {
        return; // Skip if not available (CI, other machines)
    }
    // ...
}
```

The real-plugin test gracefully skips when the fixture isn't available. This is better than `#[ignore]` because it runs automatically when the fixture exists but doesn't fail in CI.

---

### Quiz: Phase 2

### Q1: Format Detection Priority
Why does the parser check for `plugin.yaml` before `.claude-plugin/plugin.json`?

<details>
<summary>Answer</summary>

Because the IR format is a superset. If a plugin has both files (e.g., during migration from Claude Code to IR format), the IR manifest contains more information (targets, capabilities, fallbacks). Prioritizing `plugin.yaml` ensures the richer format is used. This also means a developer can add a `plugin.yaml` to an existing Claude Code plugin without deleting the original `plugin.json`.
</details>

### Q2: Commands vs Skills
Both `commands/*.md` and `skills/*.md` are parsed into `Vec<SkillDef>`. Why not keep them separate?

<details>
<summary>Answer</summary>

They're the same concept with different names. Claude Code calls them "commands" (slash commands the user invokes). The IR uses "skills" as the generic term. Merging them into one `Vec<SkillDef>` means the analyzer and emitters don't need to handle two identical types. The `source_path` preserves which directory they came from if an emitter needs to know.
</details>

### Q3: Why Sort?
Every parser function sorts output by name. What breaks if you remove the sorting?

<details>
<summary>Answer</summary>

Determinism. `walkdir` enumerates files in filesystem order, which varies by OS and filesystem. On macOS (APFS), it's roughly creation-time order. On Linux (ext4), it's inode order. Without sorting, the same plugin directory could produce different `PluginIR` values on different machines, causing snapshot test failures and non-reproducible builds.
</details>

### Q4: Error Handling
The parser uses `filter_map(|e| e.ok())` when walking directories, silently skipping unreadable entries. Is this a good pattern for a compiler?

<details>
<summary>Answer</summary>

It's debatable. Silently skipping unreadable files means a permission-denied error on a skill file won't be reported — the skill just won't appear in the IR. For a compiler that promises correctness, this is arguably wrong. A stricter approach would collect errors and report them. However, for Phase 2 this is acceptable because: (1) the common case is all files are readable, (2) missing skills will surface later when capability analysis doesn't find expected features, and (3) adding strict error collection is a refinement that can come later without changing the architecture.
</details>

### Q5: The `split_frontmatter` Function
What happens if a markdown file contains `---` in its body (e.g., as a horizontal rule)?

<details>
<summary>Answer</summary>

The function finds the *first* `\n---` after the opening delimiter, so a `---` horizontal rule later in the body won't interfere — it's already past the closing delimiter. The only edge case is if the YAML frontmatter itself contains `\n---` on a line, which would prematurely close the frontmatter. This is extremely rare in practice (YAML doesn't use `---` as content), and the same limitation exists in every frontmatter parser (Hugo, Jekyll, etc.).
</details>

---

## Phase 3: Analyzer

### What the Analyzer Does

The analyzer sits between the parser and the emitters. It takes a `PluginIR` and answers: "Can this plugin compile for each declared target?" It produces an `AnalysisReport` with three severity levels:

- **Error** — capability not supported, no fallback declared. Build will fail.
- **Warning** — capability has partial/flags support, no fallback. Build succeeds but output may differ.
- **Info** — a fallback strategy is handling the gap. Build succeeds with noted degradation.

### Key Design Decisions

#### 1. Capability Inference (Don't Trust Declarations Alone)

The plan's `requires.capabilities` field lets authors declare what they need. But the analyzer also **infers** capabilities from the plugin's actual content:

- Has `skills/*.md` files → needs `skills`
- Has hooks with `event: pre-tool-use` → needs `hooks.pre-tool-use`
- Has `mcp/*.yaml` files → needs `mcp-servers`

This inference is the safety net. Even if the author forgets to declare capabilities in `requires`, the analyzer catches incompatibilities by looking at what the plugin actually contains.

#### 2. Specific Over Parent Capabilities

A hook with `event: pre-tool-use` infers `hooks.pre-tool-use`, NOT the parent `hooks`. TDD caught this: when we inferred both, a fallback declared for `hooks.pre-tool-use` wouldn't cover the parent `hooks`, causing false errors.

The principle: **infer at the most specific level that the target matrix supports**. The capability matrices have entries for both `hooks` and `hooks.pre-tool-use`, but the specific entry is what matters for checking and fallback resolution.

#### 3. Fallback Resolution as Severity Downgrade

Without fallback: unsupported capability → Error.
With fallback: unsupported capability → Info (with description of what will happen).

This is not "hiding" the problem — the diagnostic still appears. The severity change means the build succeeds and the report explains exactly what degradation will occur. The emitters will use the fallback strategy to generate appropriate output.

#### 4. Pure Function Over Data

```rust
pub fn analyze(ir: &PluginIR) -> AnalysisReport
```

The analyzer is a pure function: `PluginIR` in, `AnalysisReport` out. No filesystem access, no side effects. This makes it trivially testable — every test constructs a `PluginIR` in memory and asserts on the report.

### Rust Patterns Used in Phase 3

#### 1. `BTreeSet` for Ordered Inferred Capabilities

```rust
fn infer_capabilities(ir: &PluginIR) -> BTreeSet<String> {
```

`BTreeSet` (not `HashSet`) for the same reason as `BTreeMap` everywhere: deterministic iteration order. When iterating over inferred capabilities to check against matrices, the order of diagnostics in the report is stable.

#### 2. Tuple Pattern Matching for Decision Matrix

```rust
match (support, fallback) {
    (SupportLevel::Full, _) => {}
    (SupportLevel::None, Some(fb)) => { /* info */ }
    (SupportLevel::None, None) => { /* error */ }
    (SupportLevel::Partial | SupportLevel::Flags, Some(fb)) => { /* info */ }
    (SupportLevel::Partial | SupportLevel::Flags, None) => { /* warning */ }
}
```

Matching on a tuple of `(SupportLevel, Option<&FallbackStrategy>)` turns the decision matrix into an exhaustive pattern match. The compiler guarantees all combinations are handled. If `SupportLevel` gains a new variant, this match will fail to compile — forcing us to decide what happens.

#### 3. Iterator Combinators for Report Queries

```rust
pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
    self.diagnostics.iter().filter(|d| d.severity == Severity::Error)
}

pub fn for_target(&self, target: Target) -> impl Iterator<Item = &Diagnostic> {
    self.diagnostics.iter().filter(move |d| d.target == target)
}
```

The `move` keyword on the closure in `for_target` is necessary because the closure captures `target` by value. Without `move`, the closure would borrow `target` — but `target` is a function parameter that's dropped when the function returns, while the iterator lives longer. `move` transfers ownership of `target` (which is `Copy`) into the closure.

---

### Quiz: Phase 3

### Q1: Why Infer Instead of Only Declaring?
The manifest has a `requires.capabilities` field. Why does the analyzer also infer capabilities from the plugin content?

<details>
<summary>Answer</summary>

Two reasons: (1) **Safety net** — if the author forgets to declare a capability, the analyzer still catches incompatibilities. A plugin with hooks that doesn't declare `hooks` in `requires` would silently compile with missing hooks on Cursor if we only checked declarations. (2) **Claude Code native plugins** — they have no `requires` field at all (it's an IR extension). The analyzer must infer everything from content to analyze these plugins.
</details>

### Q2: The TDD Catch
The first implementation inferred both `hooks` (parent) and `hooks.pre-tool-use` (specific). Why did this cause test failures with fallbacks?

<details>
<summary>Answer</summary>

The fallback was declared for `hooks.pre-tool-use` on Cursor. But the inferred capabilities also included `hooks` (the parent). Cursor doesn't support `hooks` either, and there was no fallback for the parent. So `hooks` errored while `hooks.pre-tool-use` was correctly downgraded to info. The fix: only infer specific hook capabilities, not the parent, when specific events are identified. The parent adds no information that the specifics don't already cover.
</details>

### Q3: Why Three Severity Levels?
Why not just pass/fail per capability?

<details>
<summary>Answer</summary>

Because "partial support" is a real and important middle ground. When OpenCode has `Partial` support for skills (JS exports only, not markdown-based), the plugin will work — just differently. That's a warning, not an error. The author should know about it but shouldn't be blocked. Similarly, when a fallback is declared, the author has acknowledged and planned for the degradation — that's informational, not a problem.
</details>

### Q4: Pure Function Design
The analyzer takes `&PluginIR` and returns `AnalysisReport`. Why not modify the IR in place (e.g., annotating each skill with its compatibility)?

<details>
<summary>Answer</summary>

Separation of concerns. The IR represents what the plugin IS (its structure and content). The report represents what the analyzer FOUND (compatibility issues). Mixing them would mean the IR's shape depends on which targets you're analyzing for. The emitters need both — the original IR to know what to generate, and the report to know how to degrade. Keeping them separate also means you can analyze once and emit many times without re-analyzing.
</details>

### Q5: The `move` Keyword
In `for_target(&self, target: Target)`, why does the returned iterator's closure need `move`?

<details>
<summary>Answer</summary>

The `target` parameter is a `Target` (which is `Copy`). Without `move`, the closure captures `&target` — a reference to the function parameter. But the function returns the iterator, and the parameter's stack frame is gone by then. The reference would dangle. `move` copies `target` into the closure so it owns its own copy. This is a common pattern when returning closures that capture function parameters. Rust's borrow checker catches this at compile time if you forget `move`.
</details>

---

## Phase 4: Emitters

### What Emitters Do

Emitters are the code generators — the final stage of the compiler pipeline. Each emitter takes a `PluginIR` and writes files in the format a specific target platform expects. The output is a ready-to-install plugin.

### Per-Target Output

| Target | Manifest | Skills | Agents | MCP | Instructions |
|---|---|---|---|---|---|
| Claude Code | `plugin.json` | `commands/*.md` | `agents/*.md` | `.mcp.json` | `CLAUDE.md` |
| OpenCode | `package.json` | In `AGENTS.md` | In `AGENTS.md` | — | `AGENTS.md` |
| Codex | `plugin.json` | `skills/*.md` | In `AGENTS.md` | — | `AGENTS.md` |
| Cursor | — | — | — | `.cursor/mcp.json` | `.cursorrules` |
| OpenClaw | `package.json` | — | — | — | `README.md` |

### Key Design Decisions

#### 1. No Tera Templates (Yet)

The plan called for Tera (Jinja2-like templates) for file generation. We skipped it for v0.1. The output formats are simple enough that direct string construction is clearer and more debuggable than template files. Templates make sense when output formats are complex or when end-users need to customize the output — neither applies yet.

#### 2. Roundtrip Test as the Gold Standard

The most important emitter test is the roundtrip:

```rust
fn roundtrip_parse_emitted_claude_code() {
    let ir = build_ir(vec![Target::ClaudeCode]);
    let (_tmp, out) = emit_claude_code(&ir);
    let parsed = jacq::parser::parse_plugin(&out).unwrap();
    assert_eq!(parsed.manifest.name, "test-plugin");
    assert_eq!(parsed.skills.len(), 1);
}
```

This proves that `emit → parse` produces equivalent output. If the emitter generates a broken `plugin.json` or malformed frontmatter, the parser catches it. This is the compiler's self-consistency check.

#### 3. AGENTS.md as the Universal Fallback

OpenCode and Codex both use `AGENTS.md` as their instruction file. The emitter combines instructions, skill descriptions, and agent descriptions into sections within `AGENTS.md`. This is the "instruction-based fallback" — when a target can't natively express skills or agents, they become documented text that the AI agent reads as context.

#### 4. Functions Not Traits

The plan proposed `trait Emitter { fn emit(...) }`. We used plain functions instead. A trait makes sense when you need dynamic dispatch (e.g., loading emitters as plugins). For 5 hardcoded targets matched exhaustively, a `match` is simpler and the compiler already ensures all targets are handled.

### Rust Patterns Used in Phase 4

#### 1. `tempfile::TempDir` for Test Isolation

```rust
let tmp = TempDir::new().unwrap();
emit(&ir, tmp.path(), &opts).unwrap();
// tmp is dropped at end of scope → directory deleted automatically
```

`TempDir` creates a unique temporary directory that's automatically cleaned up when dropped. Each test gets its own directory — no test pollution, no cleanup code. The `_tmp` binding pattern (keeping the `TempDir` alive while we inspect its contents via `out`) is a Rust idiom for RAII-managed resources.

#### 2. `serde_json::json!` Macro for JSON Construction

```rust
let plugin_json = serde_json::json!({
    "name": ir.manifest.name,
    "version": ir.manifest.version,
});
```

The `json!` macro builds `serde_json::Value` from a JSON-like literal syntax. It's terser than constructing `serde_json::Map` manually and reads like the output format. The trade-off: it's runtime construction, not compile-time-checked. Typos in keys won't be caught until tests run.

---

### Quiz: Phase 4

### Q1: Why Roundtrip?
Why is `emit → parse` the most important emitter test, rather than just checking file content?

<details>
<summary>Answer</summary>

File content checks are fragile — they break on whitespace changes, key ordering, etc. The roundtrip test checks semantic equivalence: "does the emitted plugin parse back to the same data?" This is the actual invariant we care about. If we change how YAML frontmatter is formatted (e.g., different quote styles), file content tests break but the roundtrip passes. The roundtrip also validates that the parser and emitter agree on the format — catching bugs in either.
</details>

### Q2: Functions vs Traits
When would it make sense to refactor the emitters from functions to a trait?

<details>
<summary>Answer</summary>

When the emitter set needs to be extensible without modifying jacq's source. If community contributors should be able to add new target emitters as plugins (loaded from shared libraries, WASM modules, or separate crates), a trait is necessary for dynamic dispatch. The current `match` in `emit()` is closed — adding a target means editing `emitter.rs` and `targets.rs`. A trait would open this up: `Box<dyn Emitter>` loaded at runtime. The plan's Principle #4 ("minimal core, maximal plugins") suggests this is a future direction.
</details>

### Q3: The AGENTS.md Pattern
Why do OpenCode and Codex both get `AGENTS.md` while Claude Code gets separate `CLAUDE.md`, `commands/`, and `agents/` directories?

<details>
<summary>Answer</summary>

Claude Code has a rich plugin system with native support for discrete skills (frontmatter-based .md files), agents (frontmatter-based .md files), and instructions (`CLAUDE.md`). These are first-class concepts with specific loading behavior. OpenCode and Codex have simpler models where the AI reads a single instruction document. Cramming skills and agents into `AGENTS.md` sections is the "instruction-based fallback" — the features become documented text rather than native plugin constructs. The AI still sees them, but the host platform doesn't manage them as discrete units.
</details>

---

## Phase 5: CLI Wiring

### What This Phase Does

Connects the pipeline stages (parse → analyze → emit) into working CLI commands. After Phase 5, `jacq` is a functional tool:

- `jacq init my-plugin` — scaffold a new IR plugin
- `jacq init my-plugin --from ./existing` — import a Claude Code plugin
- `jacq validate ./my-plugin` — parse + analyze, report issues
- `jacq build ./my-plugin` — parse + analyze + emit to `dist/`
- `jacq build ./my-plugin --target opencode` — build for one target
- `jacq inspect ./my-plugin` — show capability matrix and compatibility
- `jacq test ./my-plugin` — alias for validate (schema validation later)

### Key Design Decision: Subprocess Tests

The CLI tests use `std::process::Command` to run `jacq` as a real subprocess:

```rust
fn jacq() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jacq"))
}
```

`CARGO_BIN_EXE_jacq` is a Cargo-provided environment variable that points to the compiled binary. This tests the real CLI — argument parsing, exit codes, stdout/stderr — not just the library functions. The emitter and analyzer tests cover the library; the CLI tests cover the user-facing behavior.

### Quiz: Phase 5

### Q1: Why Subprocess Tests?
The analyzer and emitter tests call library functions directly. Why do CLI tests run jacq as a subprocess instead?

<details>
<summary>Answer</summary>

Library tests verify correctness of individual stages. CLI tests verify the user experience: Does `--target claude-code` actually filter? Does a missing manifest produce a non-zero exit code? Does `--from` import correctly? These behaviors involve clap argument parsing, exit code handling, and stdout/stderr output — none of which are exercised by calling library functions. A bug in main.rs (e.g., forgetting to pass `strict` to the emitter) would only be caught by subprocess tests.
</details>

### Q2: Exit Codes
`jacq validate` on an empty directory returns exit code 1. Why not use specific exit codes for different failure types?

<details>
<summary>Answer</summary>

For v0.1, binary pass/fail is sufficient. The diagnostic output tells the user what went wrong. Specific exit codes (e.g., 2 for parse errors, 3 for analysis failures) are useful for scripting (`if jacq validate; then jacq build; fi`), but the simpler contract (0 = success, 1 = failure) is correct and adequate. Exit code semantics can be added later without breaking existing usage.
</details>

---

## Phase 6: Dogfooding

### What We Did

We tested jacq against two real Claude Code plugins to validate the compiler end-to-end and discover IR gaps.

#### notes-app-plugin (simple — committed as `examples/notes-app/`)

1. **Import:** `jacq init examples/notes-app --from /Volumes/Sidecar/notes-app-plugin`
   - jacq read the `.claude-plugin/plugin.json` and `commands/notes.md`
   - Generated `plugin.yaml` with IR fields defaulted
   - Copied skill files into `skills/`

2. **Clean up manifest:** The auto-generated YAML had `email: null`, `requires: null`, `fallbacks: {}`. We hand-edited `plugin.yaml` to a cleaner form and added `targets: [claude-code, opencode, codex]` with a `requires` block.

3. **Validate:** `jacq validate examples/notes-app` — confirmed 1 skill parsed, OpenCode has a warning (partial skill support), Claude Code and Codex are clean.

4. **Inspect:** `jacq inspect examples/notes-app` — printed the capability matrix showing Full/Partial/None across all targets.

5. **Build:** `jacq build examples/notes-app -o examples/notes-app/dist` — generated:
   - `claude-code/plugin.json` + `commands/notes.md`
   - `opencode/package.json` + `AGENTS.md`
   - `codex/plugin.json` + `skills/notes.md` + `AGENTS.md`

6. **Roundtrip:** `jacq validate examples/notes-app/dist/claude-code` — emitted Claude Code output parsed back successfully.

#### kinelo-connect (complex — tested locally, not committed)

1. **Cloned** the repo to `/tmp/` for inspection.

2. **Analyzed** the directory structure: 5 skills (in `skills/<name>/SKILL.md` subdirectory pattern), 6 agents, 2 hooks (`SessionStart` + `Stop`), 1 HTTP MCP server, scripts.

3. **Created IR by hand** in `/tmp/kinelo-ir/`:
   - Wrote `plugin.yaml` with 3 targets, capability requirements, and fallback strategies
   - Copied skills/agents, flattening from `skills/<name>/SKILL.md` to `skills/<name>.md`
   - Modeled the HTTP MCP server as a command-based proxy (`npx @anthropic-ai/mcp-proxy <url>`)
   - Wrote instructions summarizing usage

4. **Validated + built** — 5 skills, 6 agents, 1 MCP server parsed. Fallbacks worked: agents on OpenCode/Codex showed as INFO (not errors). OpenCode AGENTS.md combined all skills and agents into a readable document.

5. **Identified 6 IR gaps** — features kinelo uses that jacq can't yet represent:
   - `SessionStart` hook event (not in our `HookEvent` enum)
   - Prompt-type hooks (model evaluates a condition, not a shell command)
   - HTTP MCP servers (`"type": "http"` with `url`, not `command` + `args`)
   - Environment variable templates (`${VAR:-default}`)
   - Plugin-relative paths (`${CLAUDE_PLUGIN_ROOT}`)
   - Subdirectory skills (`skills/ask/SKILL.md` pattern)

6. **Renamed to acme-connect**, stored locally with the directory gitignored.

### What This Proved

- The full pipeline works on real plugins, not just test fixtures
- Fallback resolution is practically useful — agents degrade to AGENTS.md sections
- The capability matrix in `jacq inspect` is immediately actionable
- The roundtrip property (emit → parse) holds on real content
- The IR has real gaps that drive the v0.2 roadmap

---

### Quiz: Phase 6

### Q1: Import Hygiene
When `jacq init --from` imported the notes-app-plugin, the generated `plugin.yaml` had `email: null`, `requires: null`, and `fallbacks: {}`. Why did serde produce these, and how should `jacq init` handle this better?

<details>
<summary>Answer</summary>

Serde serializes all fields by default, including `Option<T>` as `null` and empty collections as `{}`. The `Author::Structured { name, email: None }` serializes with an explicit `email: null` because serde doesn't skip `None` fields unless told to with `#[serde(skip_serializing_if = "Option::is_none")]`. The fix: add `skip_serializing_if` attributes to optional fields in `PluginManifest`, or have `jacq init --from` use a hand-crafted YAML template instead of `serde_yaml::to_string`.
</details>

### Q2: The Proxy Workaround
kinelo-connect uses an HTTP MCP server (`"type": "http"`, `"url": "https://..."`) but our IR only models command-based MCP servers. We worked around this by using `npx @anthropic-ai/mcp-proxy <url>`. What's the trade-off?

<details>
<summary>Answer</summary>

The workaround adds a runtime dependency (`@anthropic-ai/mcp-proxy`) and an extra process hop (npx spawns a proxy that HTTP-connects to the server). The original plugin connects directly. This works but is slower to start and requires npm/Node.js on the user's machine. The proper fix is adding HTTP MCP server support to the IR: `McpServerDef` should be an enum with `Stdio { command, args, env }` and `Http { url, headers }` variants. MCP spec supports both transport types natively.
</details>

### Q3: Fallback Validation
The kinelo IR declared `agents: agents-md-section` as a fallback for OpenCode. How does jacq verify this fallback is actually effective — i.e., that the AGENTS.md output includes the agent information?

<details>
<summary>Answer</summary>

It doesn't — not yet. The analyzer checks that a fallback is *declared* and downgrades the diagnostic from Error to Info, but it doesn't verify the emitter actually *applies* the fallback. The `agents-md-section` strategy is implemented by `render_agents_md()` which includes agent descriptions in AGENTS.md, but there's no test that connects the declared fallback to the emitter behavior. A future improvement would be a post-emission validation step that checks whether each declared fallback was actually reflected in the output.
</details>

### Q4: Subdirectory Skills
kinelo-connect uses `skills/ask/SKILL.md` (skill name from directory) while jacq expects `skills/ask.md` (name from filename). We flattened them manually during import. What would automatic handling look like in the parser?

<details>
<summary>Answer</summary>

The parser's `walk_files()` already walks with `max_depth(2)`, so it sees files in subdirectories. The issue is naming: `skills/ask/SKILL.md` would produce name `"SKILL"` (from `file_stem`). The fix: if a file is named `SKILL.md` (or `COMMAND.md`, `AGENT.md`), derive the name from the parent directory instead of the filename. This matches Claude Code's own convention where `skills/ask/SKILL.md` creates a skill named "ask".
</details>

### Q5: The Roundtrip Property
We verified that `jacq build → jacq validate` works on emitted Claude Code output. What specific things could break this roundtrip, and why is it the single most important property of the compiler?

<details>
<summary>Answer</summary>

Things that break the roundtrip: (1) The emitter generates frontmatter YAML that the parser can't re-parse (e.g., different quoting, indentation, or key ordering). (2) The emitter places files in directories the parser doesn't search (e.g., `skills/` vs `commands/`). (3) The emitter generates a `plugin.json` with fields the parser's serde model rejects.

It's the most important property because it's a *self-consistency check* — it proves the emitter and parser agree on the format. If the roundtrip breaks, either the emitter is producing invalid output (users get broken plugins) or the parser is too strict (it rejects valid plugins). Every other test checks individual components; the roundtrip tests the system.
</details>

### Q6: Why Not Commit kinelo-connect?
We renamed kinelo-connect to acme-connect and gitignored it. Beyond proprietary concerns, what technical value does having a complex local-only example provide during development?

<details>
<summary>Answer</summary>

It serves as a regression test for complex features without polluting the public test suite. Every time a developer changes the parser, analyzer, or emitter, they can run `jacq build examples/acme-connect` to verify the change doesn't break a real-world complex plugin. The notes-app example (committed) is too simple to catch most regressions — it has 1 skill, no agents, no hooks, no MCP. The acme-connect example (local) exercises 5 skills, 6 agents, 1 MCP server, and fallback strategies. It's the difference between a unit test and an integration test against production data.
</details>

---

## Template Compilation: Programmatic Reuse

### What This Phase Does

Transforms jacq from a "file structure compiler" to a "template compiler." Skill, agent, and instruction bodies are no longer opaque strings — they're parsed templates with validated variable references and target-specific rendering.

### The Core Type Change

```rust
// Before: body is an opaque string, copied verbatim
pub body: String,

// After: body is either plain text or a parsed template
pub body: BodyContent,

pub enum BodyContent {
    Plain(String),           // no {{...}}, zero overhead
    Template(TemplateBody),  // has variables, needs rendering
}
```

`From<String>` and `From<&str>` impls make migration mechanical: `body: "text".to_string()` → `body: "text".into()`. The Rust compiler tells you every location to change.

### The Pipeline Grows

```
Before:  parse → analyze → emit (bodies copied verbatim)
After:   parse → EXTRACT → VALIDATE → analyze → RENDER → emit
```

New stages:
- **Extract** — `template::extract_all(ir)` scans bodies for `{{var}}` patterns, upgrades `Plain` → `Template`
- **Validate** — `template::validate(ir)` checks all variable refs exist in `manifest.vars`
- **Render** — `template::render(body, vars, target)` substitutes values via Tera, target-specific

### Key Design Decisions

#### 1. `BodyContent` Enum (Parse, Don't Validate)

Rather than treating every body as a template (expensive, could break content with literal `{{`), the `extract()` function only upgrades bodies that actually contain `{{...}}`. Bodies without templates remain `Plain` — zero overhead, zero risk.

This is the "parse, don't validate" principle: a `BodyContent::Template` proves the body has been scanned and its variables catalogued. Downstream code knows what it's dealing with.

#### 2. Target-Specific Variable Values

```yaml
vars:
  arguments_var:
    targets:
      claude-code: "$ARGUMENTS"
      codex: "$INPUT"
      opencode: "${args}"
```

Each target has its own variable bindings. A skill body with `{{arguments_var}}` compiles to `$ARGUMENTS` for Claude Code and `$INPUT` for Codex. This is the "compile separately per platform" principle applied to content, not just structure.

#### 3. Tera for Rendering (Finally Used)

Tera has been in `Cargo.toml` since Phase 1 but was never imported. Template compilation is why it was added. `Tera::one_off()` renders each body as an inline template — no filesystem, no template registry. Fast enough for the CLI. When we add `{% include %}` (Phase 2), we'll need a persistent `Tera` instance with registered fragments.

#### 4. Source Spans for LSP Readiness

```rust
pub struct VariableRef {
    pub name: String,
    pub span: (usize, usize),  // byte offsets of {{name}} in the raw text
}
```

Every variable reference carries byte offsets. This isn't used by the CLI today, but when the LSP is built, it enables "go to definition" (click `{{var}}` → jump to its declaration in `plugin.yaml`) and red squiggles on undeclared variables at exact positions.

### Rust Patterns Used

#### 1. `From<T>` for Migration

```rust
impl From<String> for BodyContent {
    fn from(s: String) -> Self { BodyContent::Plain(s) }
}
impl From<&str> for BodyContent {
    fn from(s: &str) -> Self { BodyContent::Plain(s.to_string()) }
}
```

This is the migration strategy. 127 existing tests had `body: "text".to_string()`. With the `From` impl, they become `body: "text".into()` — the compiler flags every location, the fix is mechanical, no semantics change.

#### 2. Pattern Matching for Zero-Cost Abstraction

```rust
pub fn as_raw(&self) -> &str {
    match self {
        BodyContent::Plain(s) => s,
        BodyContent::Template(t) => &t.raw,
    }
}
```

Code that doesn't care about templates (like the emitter writing a body to disk) calls `.as_raw()` and gets a `&str` regardless. The template system adds no overhead to code paths that don't use it.

---

### Quiz: Template Compilation

### Q1: Why Not Template Everything?
Bodies without `{{` stay as `BodyContent::Plain`. Why not parse every body as a Tera template?

<details>
<summary>Answer</summary>

Three reasons: (1) **Performance** — Tera parses templates, which is unnecessary overhead for bodies that are just text. (2) **Safety** — a body containing literal `{{ }}` (e.g., documenting Jinja2 templates) would be misinterpreted. (3) **Backwards compatibility** — existing plugins that never use template variables should work identically, with zero behavioral change. The extract function is the gatekeeper: only bodies with actual `{{...}}` patterns are upgraded to `Template`.
</details>

### Q2: Why Compile-Time Variable Checking?
`template::validate()` rejects undeclared `{{var}}` references at build time. Why not just let Tera fail at render time?

<details>
<summary>Answer</summary>

Tera's runtime error would say "variable `undefined_var` not found." jacq's compile-time check says "Undeclared template variable 'undefined_var' in skills/search.md — Declare it in plugin.yaml under 'vars:'." The compile-time check is better because: (1) it runs before any emission, failing fast; (2) it includes the file path and byte span for LSP integration; (3) it checks ALL variables across ALL bodies in one pass, not one-at-a-time during rendering; (4) it's a separate validation step that can be run without emitting (`jacq validate`).
</details>

### Q3: The `$ARGUMENTS` Distinction
Claude Code uses `$ARGUMENTS` in skill bodies as a runtime variable. Why doesn't the template extractor treat `$ARGUMENTS` as a template variable?

<details>
<summary>Answer</summary>

The extractor only looks for `{{...}}` patterns (Tera/Jinja2 syntax). `$ARGUMENTS` uses a `$` sigil which is Claude Code's own runtime substitution — it happens when the skill runs, not when jacq compiles. This is a deliberate two-level design: `{{var}}` is resolved at compile time by jacq (build-time), `$ARGUMENTS` is resolved at runtime by the host agent (run-time). They compose: `{{arguments_var}}` could compile to `$ARGUMENTS` for Claude Code and `$INPUT` for Codex.
</details>

### Q4: Target-Specific Values
A VarDef has both `default` and `targets`. What happens when a body uses `{{var}}`, `var` has a default of "X", and the current target is Codex which has no override?

<details>
<summary>Answer</summary>

The render function checks `var_def.targets.get(&target)` first. If Codex has no override, it falls back to `var_def.default`. If default is `Some("X")`, the rendered value is "X". If default is `None`, the value is an empty string (Tera treats missing variables as empty). The `required: true` flag on VarDef catches this at validation time — `MissingVariableValue` error — before rendering ever runs.
</details>

---

*This guide will continue to grow as jacq evolves. Next: shared fragments (`{% include %}`), schema-driven content (`{{ schema_enum() }}`), and the LSP server.*
