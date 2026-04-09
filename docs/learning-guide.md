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

*This guide grows with each phase. Phase 2 (Parser) will cover YAML frontmatter extraction, directory walking, and error reporting with miette.*
