# swc-plugin-formatjs

Rust/SWC port of [`babel-plugin-formatjs`](https://www.npmjs.com/package/babel-plugin-formatjs).
Version lock-stepped with the upstream npm package — `swc-plugin-formatjs 10.5.41`
mirrors `babel-plugin-formatjs@10.5.41`.

Ships in two consumption modes from a single crate:

- **WASI plugin** for `@swc/core`'s wasm plugin host.
- **Native rlib** that links into a native `@swc/core` build via
  `swc::Compiler::process_js_with_custom_pass`.

## Native usage

```rust
use swc::{config::{Config, IsModule, Options}, Compiler};
use swc_core::common::{errors::Handler, FileName, SourceMap, GLOBALS};
use swc_core::ecma::ast::noop_pass;
use swc_plugin_formatjs::{formatjs_pass, Config as FormatJsConfig};
use std::sync::Arc;

GLOBALS.set(&Default::default(), || {
    let cm: Arc<SourceMap> = Default::default();
    let handler = Handler::with_emitter_writer(Box::new(std::io::stderr()), Some(cm.clone()));
    let compiler = Compiler::new(cm.clone());

    let fm = cm.new_source_file(
        Arc::new(FileName::Real("input.tsx".into())),
        source.to_string(),
    );

    let opts = Options {
        config: Config {
            is_module: Some(IsModule::Bool(true)),
            ..Default::default()
        },
        swcrc: false,
        ..Default::default()
    };

    let result = compiler.process_js_with_custom_pass(
        fm, None, &handler, &opts,
        swc_core::common::comments::SingleThreadedComments::default(),
        |_| formatjs_pass(FormatJsConfig::default()),
        |_| noop_pass(),
    ).unwrap();
});
```

## WASI plugin usage

Configure via `@swc/core`'s `experimental.plugins`:

```js
require("@swc/core").transform(source, {
  jsc: {
    parser: { syntax: "typescript", tsx: true },
    experimental: {
      plugins: [
        ["swc-plugin-formatjs", {
          idInterpolationPattern: "[sha512:contenthash:base64:6]",
          removeDefaultMessage: false,
        }],
      ],
    },
  },
});
```

## Version-compat matrix

| `swc-plugin-formatjs` | `swc_core` | `swc` (high-level) |
| --------------------- | ---------- | ------------------ |
| `10.5.41`             | `=54.0.0`  | `=52.0.0`          |

These are deliberately exact-pinned. Drifting any of them is a behavior
change — re-run the upstream parity harness before publishing.

## Supported options

Mirrors the babel `Options` interface that real upstream consumers exercise:

- `idInterpolationPattern` (default: `[sha512:contenthash:base64:6]`)
- `removeDefaultMessage`
- `additionalComponentNames`
- `additionalFunctionNames`
- `preserveWhitespace`

## Hard-error policy

Where babel's plugin would silently fold a non-literal `defaultMessage` via
`path.evaluate()` (string concatenation, identifier-to-const refs), this port
**hard-errors** through `swc_common::errors::HANDLER` and halts the transform.
The motivation is: silent misses across a large codebase are far more
expensive than a loud build break that points to a single line.

Specifically rejected (with span-attached diagnostics):

- Non-string-literal `id` / `defaultMessage` / `description` (call expression and JSX)
- Spread (`...rest`) inside a message descriptor object
- Computed or numeric property keys in a descriptor
- Shorthand `{ defaultMessage }` for descriptor fields
- Empty `defaultMessage=""` with no `id`
- Computed-member callees (`intl[fnName](...)`)
- `ast: true`, `extractSourceLocation`, `pragma`, `overrideIdFn`,
  `onMsgExtracted`, `onMetaExtracted` config options
- `idInterpolationPattern` tokens other than `[hash...]`/`[contenthash...]`
- Hash type ≠ `sha512` and digest type ≠ `base64`/`hex`

JSX spread *without* an explicit `defaultMessage` attr (`<FormattedMessage {...descriptor} />`)
is silently skipped, matching babel exactly — the descriptor is assumed to
be extracted at the spread source.

## License

MIT
