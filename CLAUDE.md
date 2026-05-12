# babel-plugin-formatjs → SWC parity port

## Goal

Reimplement `babel-plugin-formatjs@10.5.41` (EXACT version) as a Rust crate
on top of `swc_core = "=54.0.0"` so the same transform can run from a
**WASI** SWC plugin (the `@swc/core` wasm host) and as a **native** Rust
visitor linked directly into a host `@swc/core` build.

The bar is **byte-identical parity**:

> Any input file run through the original babel plugin must produce
> byte-identical output to the new Rust/SWC plugin **after** accounting for
> babel-vs-swc cosmetic codegen differences (quote style, semicolons,
> indentation, trailing commas).
>
> - Difference caused by the **plugin** (OLD babel plugin vs NEW rust plugin) → **immediate fail**.
> - Difference caused by the underlying **codegen** (BABEL printer vs SWC printer) → fine, normalized away.

The normalization is implemented by re-printing both outputs through
`@swc/core` and comparing the resulting strings byte-for-byte
(`scripts/normalize.mjs` → `scripts/parity.mjs`).

## Pinned versions (DO NOT BUMP without a parity re-run)

| Component                  | Version           |
| -------------------------- | ----------------- |
| `babel-plugin-formatjs`    | `10.5.41` (exact) |
| `swc_core`                 | `=54.0.0`         |
| Required swc_core features | `ecma_ast`, `ecma_visit`, `common`, plus `plugin_transform` under the `plugin` feature |

`package.json` pins the npm side. `crates/swc-plugin-formatjs/Cargo.toml`
pins the rust side with `version = "=54.0.0"`. Both are load-bearing for
parity — bumping either is a behavior change.

## Layout

```
/
├── package.json                       # npm side, pins babel-plugin-formatjs@10.5.41
├── Cargo.toml                         # cargo workspace
├── crates/
│   └── swc-plugin-formatjs/
│       ├── Cargo.toml                 # cdylib + rlib, features: native (default) | plugin
│       └── src/lib.rs                 # FormatJsTransform (VisitMut) + WASI entry
├── scripts/
│   ├── run-babel.mjs                  # transforms a fixture with babel-plugin-formatjs
│   ├── run-swc.mjs                    # transforms a fixture with the wasm SWC plugin
│   ├── normalize.mjs                  # re-prints code through swc to strip cosmetic diffs
│   └── parity.mjs                     # full harness: run both, normalize, byte-compare
└── fixtures/
    ├── 01-call-formatmessage/         # intl.formatMessage({...}) extraction
    ├── 02-formatted-message-jsx/      # <FormattedMessage .../> extraction
    └── 03-remove-default-message/     # removeDefaultMessage: true
```

## Build & run

```bash
# 1. Install npm deps (pins babel-plugin-formatjs@10.5.41).
npm install

# 2. Build the rust plugin as a WASI artifact for @swc/core.
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1 \
  -p swc-plugin-formatjs --features plugin --no-default-features
# -> target/wasm32-wasip1/release/swc_plugin_formatjs.wasm

# 3. Build the rust plugin as a native rlib (for native @swc/core embedders).
cargo build --release -p swc-plugin-formatjs
# -> target/release/libswc_plugin_formatjs.rlib (+ .so/.dylib/.dll)

# 4. Run the parity harness.
node scripts/parity.mjs
```

Exit code is `0` only if every fixture's babel output (after swc-print
normalization) is byte-identical to its swc output.

## Dual-target design (WASI + native)

A single crate emits both artifacts via cargo features:

- `native` (default) — `FormatJsTransform` + `formatjs(config)` constructor
  for embedders that link the crate as a regular Rust dependency. No WASI
  symbols. `cargo check` / `cargo test` on the host platform Just Works.
- `plugin` — adds `swc_core/plugin_transform` and the `#[plugin_transform]`
  entry point (`process_transform`) needed by the `@swc/core` wasm host.

`[lib] crate-type = ["cdylib", "rlib"]` is required so the same crate can
produce both the `.wasm` cdylib (WASI) and an importable rlib (native).

## Plugin options surface

`Config` in `src/lib.rs` covers the subset of babel-plugin-formatjs@10.5.41
`Options` that upstream actually uses:

| Option                       | Notes                                                       |
| ---------------------------- | ----------------------------------------------------------- |
| `idInterpolationPattern`     | Default: `[sha512:contenthash:base64:6]`. **Must hash-match babel byte-for-byte** — this is the consumer-observable parity bar. |
| `removeDefaultMessage`       | Strip `defaultMessage` from emitted descriptors             |
| `additionalComponentNames`   | Extra JSX names treated like `<FormattedMessage>`           |
| `additionalFunctionNames`    | Extra call names treated like `formatMessage` / `$t` / `$formatMessage` |
| `pragma`                     | File-level meta extraction comment marker                   |
| `extractSourceLocation`      | Attach `{file, start, end}` to each message                 |
| `ast`                        | Compile messages to icu-messageformat AST in-place          |
| `preserveWhitespace`         | Skip the whitespace collapser on extracted messages         |

### Deliberately not supported

| Option                                   | Why                                                                  |
| ---------------------------------------- | -------------------------------------------------------------------- |
| `overrideIdFn`                           | JS callback. Upstream doesn't use it. No WASI host-call shim needed. |
| `onMsgExtracted` / `onMetaExtracted`     | JS callbacks. Upstream doesn't use them.                             |
| `moduleSourceName`                       | Removed from `babel-plugin-formatjs` before the 10.x line. Upstream will drop it from call sites during migration. |

## Parity rules — what we fail on

We **must** fail (and stop) when any of these diverge between babel and rust:

- Computed message `id` (hash, override pattern, or pass-through)
- Presence/absence of `defaultMessage` / `description` after transform
- ICU-compiled `ast` shape (when `ast: true`)
- Component/function name matching (`additionalComponentNames` etc.)
- Whitespace-collapse behavior on message strings
- Source-location attachment when `extractSourceLocation: true`
- Set of extracted messages (and their order)

We **may** diverge (normalized away) on:

- Quote style (`'` vs `"`), trailing commas, semicolons
- Indentation, line wrapping
- Property key ordering when babel/swc disagree but semantics match
  *(note: this is a grey area — if the babel plugin emits a deterministic
  property order, the rust plugin must match it pre-normalization)*

## Hard-error policy

Silent misses are unacceptable. Where babel's plugin would extract a message
(via `path.evaluate()`'s constant folding) but this Rust port can't statically
resolve the value, we **hard error** via `swc_common::errors::HANDLER` and panic
the transform — so the build breaks loudly instead of producing divergent
output across a 90 GB monorepo.

| Construct                                                       | Behavior |
| --------------------------------------------------------------- | -------- |
| `defaultMessage: 'a' + 'b'`, `defaultMessage: someConst`        | **fail** — constant folding not implemented |
| `{...spread, defaultMessage: '...'}` in a descriptor            | **fail** — expand the spread to explicit keys |
| `{[computed]: '...'}` in a descriptor                           | **fail** — use literal keys |
| `{ defaultMessage }` shorthand in a descriptor                  | **fail** — expand to `defaultMessage: '...'` |
| `<FormattedMessage defaultMessage={someVar} />`                 | **fail** — value must be a string literal |
| `<FormattedMessage defaultMessage={<span/>} />`                 | **fail** — JSX/fragment values rejected |
| `<FormattedMessage defaultMessage="" />` with no `id`           | **fail** — empty + no id can't generate one |
| `intl[fnName](...)` computed callee                             | **fail** — can't tell statically if it's formatMessage |
| `ast: true` config option                                       | **fail** — no Rust ICU parser yet |
| `extractSourceLocation: true`                                   | **fail** — not implemented |
| `pragma` (non-empty)                                            | **fail** — not implemented |
| `idInterpolationPattern` with `[name]`/`[ext]`/`[path]` tokens  | **fail** — only `[hash...]`/`[contenthash...]` supported |
| Hash type other than `sha512`                                   | **fail** — only sha512 ported |
| Digest type other than `base64` / `hex`                         | **fail** — only base64/hex ported |
| `<FormattedMessage {...spread} />` (no explicit `defaultMessage`) | silently skip — matches babel (extracted elsewhere) |
| `intl.formatMessage(notAnObjectLiteral)`                        | silently skip — matches babel |

Each error includes a span (when possible) and a one-line remediation hint
so the developer can either inline the literal or extend this plugin.

The error-path coverage lives in `fixtures-error/`. The harness asserts
that the plugin fails with a substring matching `options.json::expectError`
for each one — so adding a new "we don't support that yet" case means
adding both the construct AND a fixture that proves the error fires.

## Where to start porting

`node_modules/babel-plugin-formatjs/` holds the reference implementation:

- `index.js` — plugin entry, sets up state, registers visitors.
- `visitors/call-expression.js` — handles `formatMessage` / `$t` / `$formatMessage`.
- `visitors/jsx-opening-element.js` — handles `<FormattedMessage>` and friends.
- `types.d.ts` — the `Options` interface that `Config` in `src/lib.rs` mirrors.

The hash/ID interpolation logic lives in
`@formatjs/ts-transformer` (a transitive dep) — that module also has to
be ported (or its hashing algorithm replicated bit-exactly) since the
default `idInterpolationPattern` produces a content hash.
