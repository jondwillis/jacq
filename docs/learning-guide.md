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

*This guide grows with each phase. Phase 3 (Analyzer) will cover capability comparison, compatibility reporting, and fallback resolution.*
