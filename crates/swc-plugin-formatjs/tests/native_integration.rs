//! End-to-end test of the **native** consumption path.
//!
//! Drives a representative fixture through `swc::Compiler::process_js_with_custom_pass`
//! — the same entry point downstream consumers will use after pulling
//! `swc-plugin-formatjs` from crates.io. Catches version-resolution
//! surprises (different `swc_ecma_ast` versions between `swc` and our
//! `swc_core` pin would surface as a trait-bound mismatch here) and
//! confirms `formatjs_pass(...)` plugs into the high-level API with no
//! adapter glue at the call site.
//!
//! If this test fails on a future `swc_core` bump, the published crate's
//! native consumers will break in the same way — fix here first.

use std::sync::Arc;

use swc::{
    config::{Config, IsModule, Options},
    Compiler,
};
use swc_core::common::{errors::Handler, FileName, SourceMap, GLOBALS};
use swc_core::ecma::ast::noop_pass;

use swc_plugin_formatjs::{formatjs_pass, Config as PluginConfig};

#[test]
fn process_js_with_custom_pass_inserts_expected_id_and_strips_description() {
    // `swc::Compiler::run` requires a `GLOBALS` scope (interning state for
    // Marks/SyntaxContexts). The wasm plugin host installs one for us;
    // native callers have to install it themselves.
    GLOBALS.set(&Default::default(), || run_test());
}

fn run_test() {
    let cm: Arc<SourceMap> = Default::default();
    let handler = Handler::with_emitter_writer(Box::new(std::io::stderr()), Some(cm.clone()));
    let compiler = Compiler::new(cm.clone());

    let source = r#"
import { useIntl } from "react-intl";
export function Greeting() {
    const intl = useIntl();
    return intl.formatMessage({
        defaultMessage: "Hello, {name}!",
        description: "Greets the user by name",
    });
}
"#;

    let fm = cm.new_source_file(
        Arc::new(FileName::Real("test.tsx".into())),
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

    let comments = swc_core::common::comments::SingleThreadedComments::default();

    let result = compiler
        .process_js_with_custom_pass(
            fm,
            None,
            &handler,
            &opts,
            comments,
            |_program| formatjs_pass(PluginConfig::default()),
            |_program| noop_pass(),
        )
        .expect("process_js_with_custom_pass should succeed");

    // Hash cross-checked against Node:
    //   crypto.createHash('sha512')
    //         .update('Hello, {name}!#Greets the user by name')
    //         .digest('base64').slice(0, 6)  ===  'LlYGNJ'
    assert!(
        result.code.contains("LlYGNJ"),
        "expected generated id `LlYGNJ` in output; got:\n{}",
        result.code
    );
    assert!(
        result.code.contains("Hello, {name}!"),
        "expected `defaultMessage` to be preserved; got:\n{}",
        result.code
    );
    assert!(
        !result.code.contains("Greets the user by name"),
        "expected `description` to be stripped; got:\n{}",
        result.code
    );
}
