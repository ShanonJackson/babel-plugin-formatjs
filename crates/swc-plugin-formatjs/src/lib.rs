//! Rust/SWC port of `babel-plugin-formatjs@10.5.41`.
//!
//! Build targets:
//!   - WASI plugin (`--features plugin --no-default-features --target wasm32-wasip1`)
//!     for @swc/core's wasm plugin host. Entry point: `process_transform`.
//!   - Native rlib (`--features native`, default) for embedders that link
//!     directly into a native @swc/core build. Entry point: [`formatjs`].
//!
//! # Hard-error policy
//!
//! Per consumer requirement, this plugin MUST hard-error on any construct
//! we can't statically resolve, rather than silently skip extraction.
//!
//! Babel's plugin falls back on `path.evaluate()` for constant folding
//! (string concatenation, const references, ternaries with literal arms).
//! This Rust port does not implement scope-aware constant folding —
//! supporting it properly would mean re-implementing a chunk of @babel.
//! Until that lands, we fail loudly via [`fail`] so the build breaks
//! instead of silently producing divergent output in a 90 GB monorepo.
//!
//! See `CLAUDE.md` for the parity bar.

use std::cell::RefCell;

use serde::Deserialize;
use swc_core::common::errors::HANDLER;
use swc_core::common::{Span, Spanned, DUMMY_SP};
use swc_core::ecma::ast::*;
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

thread_local! {
    /// Filename of the file currently being transformed. Read by [`fail`]
    /// so panic messages include "(path/to/file.tsx)" — critical for
    /// diagnosing CI failures across a large monorepo where the same
    /// pattern shows up in hundreds of locations.
    ///
    /// Populated by the WASI plugin entry point from
    /// `TransformPluginMetadataContextKind::Filename`. Native callers can
    /// set it via [`with_current_file`].
    static CURRENT_FILE: RefCell<Option<String>> = const { RefCell::new(None) };
}

// Only used by the WASI plugin entry point; native callers go through
// `with_current_file` (scope guard) instead.
#[cfg(feature = "plugin")]
fn set_current_file(filename: Option<String>) {
    CURRENT_FILE.with(|c| *c.borrow_mut() = filename);
}

/// Scope guard for setting the current filename — useful for native
/// callers that want filename context in error messages without managing
/// the thread-local manually.
pub fn with_current_file<F, R>(filename: Option<String>, f: F) -> R
where
    F: FnOnce() -> R,
{
    let prev = CURRENT_FILE.with(|c| c.borrow_mut().replace(filename.unwrap_or_default()));
    let result = f();
    CURRENT_FILE.with(|c| *c.borrow_mut() = prev);
    result
}

mod hash;
mod whitespace;

use hash::{interpolate_pattern, validate_pattern};
use whitespace::normalize_whitespace;

pub const DEFAULT_ID_INTERPOLATION_PATTERN: &str = "[sha512:contenthash:base64:6]";

/// Emit a span-attached error via the SWC diagnostic handler (when
/// available — i.e. inside the WASI plugin host) and panic to halt the
/// transform. The diagnostic surfaces in @swc/core output; the panic
/// guarantees execution stops, preferable to emitting and continuing
/// to produce divergent code in a 90 GB monorepo.
///
/// Includes the filename of the file being transformed (read from the
/// `CURRENT_FILE` thread-local) — without it, panic messages from a
/// large CI run are unattributable.
///
/// `better_scoped_tls::ScopedKey` (which backs `HANDLER`) only exposes
/// `with(...)` and panics if no scope is active — i.e. in native unit
/// tests where the SWC host hasn't installed one. We sidestep that with
/// `catch_unwind` so test-only invocations of `fail` still produce a
/// clean panic with our error message.
pub(crate) fn fail(span: Span, message: impl AsRef<str>) -> ! {
    let msg = message.as_ref();
    let filename =
        CURRENT_FILE.with(|c| c.borrow().clone()).filter(|s| !s.is_empty());
    let with_file = match &filename {
        Some(f) => format!("{} (in {})", msg, f),
        None => msg.to_string(),
    };
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        HANDLER.with(|h| {
            h.struct_span_err(span, &with_file).emit();
        });
    }));
    panic!("swc-plugin-formatjs: {}", with_file);
}

/// Mirrors the option surface of `babel-plugin-formatjs@10.5.41`'s `Options`
/// that upstream actually uses.
///
/// Intentionally NOT supported (upstream doesn't use them):
///   - `overrideIdFn`, `onMsgExtracted`, `onMetaExtracted` (JS callbacks)
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub id_interpolation_pattern: Option<String>,
    pub remove_default_message: bool,
    pub additional_component_names: Vec<String>,
    pub additional_function_names: Vec<String>,
    pub pragma: Option<String>,
    pub extract_source_location: bool,
    pub ast: bool,
    pub preserve_whitespace: bool,
}

pub struct FormatJsTransform {
    config: Config,
    component_names: Vec<String>,
    function_names: Vec<String>,
}

impl FormatJsTransform {
    pub fn new(mut config: Config) -> Self {
        // Hard-error on options we don't implement, BEFORE touching any file.
        if config.ast {
            fail(
                DUMMY_SP,
                "option `ast: true` is not supported — would require a Rust port \
                 of @formatjs/icu-messageformat-parser",
            );
        }
        if config.extract_source_location {
            fail(
                DUMMY_SP,
                "option `extractSourceLocation: true` is not supported",
            );
        }
        if let Some(p) = &config.pragma {
            if !p.is_empty() {
                fail(
                    DUMMY_SP,
                    format!("option `pragma: {:?}` is not supported", p),
                );
            }
        }

        if config.id_interpolation_pattern.is_none() {
            config.id_interpolation_pattern =
                Some(DEFAULT_ID_INTERPOLATION_PATTERN.to_string());
        }
        validate_pattern(config.id_interpolation_pattern.as_deref().unwrap());

        // Mirrors `index.js`:
        //   componentNames = additionalComponentNames + 'FormattedMessage'
        //   functionNames  = additionalFunctionNames + 'formatMessage' + '$t' + '$formatMessage'
        let mut function_names = config.additional_function_names.clone();
        for fixed in ["formatMessage", "$t", "$formatMessage"] {
            if !function_names.iter().any(|n| n == fixed) {
                function_names.push(fixed.to_string());
            }
        }
        let mut component_names = config.additional_component_names.clone();
        if !component_names.iter().any(|n| n == "FormattedMessage") {
            component_names.push("FormattedMessage".to_string());
        }

        Self {
            config,
            component_names,
            function_names,
        }
    }
}

pub fn formatjs(config: Config) -> FormatJsTransform {
    FormatJsTransform::new(config)
}

/// Convenience wrapper that returns the transform as `impl Pass`, ready to
/// drop into `swc::Compiler::process_js_with_custom_pass` without the
/// caller having to import `swc_core::ecma::visit::visit_mut_pass` themselves.
///
/// Equivalent to: `swc_core::ecma::visit::visit_mut_pass(formatjs(config))`.
pub fn formatjs_pass(config: Config) -> impl swc_core::ecma::ast::Pass {
    swc_core::ecma::visit::visit_mut_pass(formatjs(config))
}

impl VisitMut for FormatJsTransform {
    fn visit_mut_call_expr(&mut self, n: &mut CallExpr) {
        n.visit_mut_children_with(self);
        self.handle_call_expr(n);
    }

    fn visit_mut_jsx_opening_element(&mut self, n: &mut JSXOpeningElement) {
        n.visit_mut_children_with(self);
        self.handle_jsx_opening(n);
    }
}

// =====================================================================
// Call expression handler
// =====================================================================
impl FormatJsTransform {
    fn handle_call_expr(&self, n: &mut CallExpr) {
        let Callee::Expr(callee_box) = &n.callee else { return };
        let callee = unwrap_ts(callee_box);
        let Some((name, _is_member)) = callee_ident_name(callee) else { return };

        if name == "defineMessage" {
            let arg = n
                .args
                .get_mut(0)
                .unwrap_or_else(|| fail(n.span, "defineMessage(...) requires an argument"));
            match unwrap_ts_mut(&mut arg.expr) {
                Expr::Object(obj) => self.process_message_object(obj),
                other => fail(
                    other.span(),
                    format!(
                        "defineMessage(...) argument must be an object literal — got {}",
                        expr_kind(other)
                    ),
                ),
            }
            return;
        }

        if name == "defineMessages" {
            let arg = n
                .args
                .get_mut(0)
                .unwrap_or_else(|| fail(n.span, "defineMessages(...) requires an argument"));
            match unwrap_ts_mut(&mut arg.expr) {
                Expr::Object(outer) => {
                    for prop in outer.props.iter_mut() {
                        match prop {
                            PropOrSpread::Spread(s) => fail(
                                s.dot3_token,
                                "spread (`...rest`) inside defineMessages({...}) is not supported \
                                 — expand to explicit `key: descriptor` entries",
                            ),
                            PropOrSpread::Prop(p) => {
                                let kv = match p.as_mut() {
                                    Prop::KeyValue(kv) => kv,
                                    Prop::Shorthand(id) => fail(
                                        id.span,
                                        "shorthand property inside defineMessages({...}) is not \
                                         supported — expand to `key: { id, defaultMessage, ... }`",
                                    ),
                                    Prop::Method(m) => fail(
                                        m.function.span,
                                        "method property inside defineMessages({...}) is not supported",
                                    ),
                                    Prop::Getter(_) | Prop::Setter(_) | Prop::Assign(_) => fail(
                                        DUMMY_SP,
                                        "getter/setter/assign properties inside defineMessages({...}) are not supported",
                                    ),
                                };
                                match unwrap_ts_mut(&mut kv.value) {
                                    Expr::Object(inner) => self.process_message_object(inner),
                                    other => fail(
                                        other.span(),
                                        format!(
                                            "defineMessages bag entry must be an object literal — got {}",
                                            expr_kind(other)
                                        ),
                                    ),
                                }
                            }
                        }
                    }
                }
                other => fail(
                    other.span(),
                    format!(
                        "defineMessages(...) argument must be an object literal — got {}",
                        expr_kind(other)
                    ),
                ),
            }
            return;
        }

        if self.is_format_message_call(&name) {
            let arg = match n.args.get_mut(0) {
                // Calling `formatMessage()` with no args is legal at runtime
                // (returns ''); babel's visitor early-returns when there's no
                // ObjectExpression in arg[0]. We do the same.
                None => return,
                Some(a) => a,
            };
            match unwrap_ts_mut(&mut arg.expr) {
                Expr::Object(obj) => self.process_message_object(obj),
                // babel does `if (messageDescriptor && messageDescriptor.isObjectExpression())`
                // — so non-object first args are SILENTLY skipped, not errored.
                // Mirror that behavior so we don't break legitimate non-extraction call sites.
                _ => {}
            }
        }
    }

    fn is_format_message_call(&self, name: &str) -> bool {
        self.function_names.iter().any(|n| n == name)
    }

    fn process_message_object(&self, obj: &mut ObjectLit) {
        let mut existing_id: Option<String> = None;
        let mut default_message: Option<String> = None;
        let mut description: Option<String> = None;
        let mut has_id_prop = false;
        let mut has_default_msg_prop = false;

        for prop in &obj.props {
            match prop {
                PropOrSpread::Spread(s) => fail(
                    s.dot3_token,
                    "spread (`...rest`) inside a message descriptor is not supported — \
                     expand to explicit `id` / `defaultMessage` / `description` properties",
                ),
                PropOrSpread::Prop(p) => match p.as_ref() {
                    Prop::KeyValue(kv) => {
                        let key = require_static_key(&kv.key);
                        if is_descriptor_key(&key) {
                            let val = require_static_string(&kv.value, &key);
                            match key.as_str() {
                                "id" => {
                                    has_id_prop = true;
                                    if !val.is_empty() {
                                        existing_id = Some(val);
                                    }
                                }
                                "defaultMessage" => {
                                    has_default_msg_prop = true;
                                    default_message = Some(val);
                                }
                                "description" => {
                                    description = Some(val);
                                }
                                _ => unreachable!(),
                            }
                        }
                        // else: unrelated property like `values` — ignored.
                    }
                    Prop::Shorthand(id) => {
                        if is_descriptor_key(&id.sym) {
                            fail(
                                id.span,
                                format!(
                                    "shorthand property `{}` in a message descriptor is not \
                                     supported — expand to `{}: <string literal>`",
                                    id.sym, id.sym
                                ),
                            );
                        }
                        // shorthand for an unrelated key (`values` etc.) is harmless.
                    }
                    Prop::Method(m) => {
                        let name = match &m.key {
                            PropName::Ident(i) => Some(i.sym.to_string()),
                            PropName::Str(s) => Some(s.value.to_atom_lossy().to_string()),
                            _ => None,
                        };
                        if name.as_deref().is_none()
                            || name.as_deref().map(is_descriptor_key).unwrap_or(true)
                        {
                            fail(
                                m.function.span,
                                "method property in a message descriptor is not supported",
                            );
                        }
                    }
                    Prop::Getter(g) => fail(
                        g.span,
                        "getter property in a message descriptor is not supported",
                    ),
                    Prop::Setter(s) => fail(
                        s.span,
                        "setter property in a message descriptor is not supported",
                    ),
                    Prop::Assign(a) => fail(
                        a.span,
                        "assignment property in a message descriptor is not supported",
                    ),
                },
            }
        }

        // No descriptor-relevant keys at all → not an extraction target. Bail
        // (matches babel: `intl.formatMessage(somethingElse)` does nothing).
        if !has_id_prop && !has_default_msg_prop {
            return;
        }

        // babel's `evaluateMessageDescriptor` only hashes when
        // `!id && idInterpolationPattern && defaultMessage`. `""` is falsy in
        // JS, so an empty defaultMessage with no id would fall through to
        // `storeMessage`, which then throws on `!id && !defaultMessage`.
        // Mirror that: hard-error early instead of silently hashing "".
        if existing_id.is_none()
            && default_message
                .as_deref()
                .map(str::is_empty)
                .unwrap_or(true)
        {
            fail(
                DUMMY_SP,
                "message descriptor has neither a non-empty `id` nor a non-empty `defaultMessage` \
                 — cannot generate an id",
            );
        }

        let normalized_msg = default_message.as_ref().map(|m| {
            if self.config.preserve_whitespace {
                m.clone()
            } else {
                normalize_whitespace(m)
            }
        });

        let final_id = compute_id(
            existing_id.as_deref(),
            normalized_msg.as_deref(),
            description.as_deref(),
            self.config
                .id_interpolation_pattern
                .as_deref()
                .unwrap_or(DEFAULT_ID_INTERPOLATION_PATTERN),
        );

        // Mutation order mirrors call-expression.js:
        //   1. Insert/replace `id` (before removing anything else).
        //   2. Remove `description`.
        //   3. Replace/remove `defaultMessage`.

        if has_id_prop {
            replace_object_value(obj, "id", string_expr(&final_id));
        } else {
            obj.props.insert(
                0,
                PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(IdentName {
                        span: DUMMY_SP,
                        sym: "id".into(),
                    }),
                    value: Box::new(string_expr(&final_id)),
                }))),
            );
        }

        obj.props.retain(|p| {
            let PropOrSpread::Prop(prop) = p else { return true };
            let Prop::KeyValue(kv) = prop.as_ref() else { return true };
            prop_name_as_str(&kv.key).as_deref() != Some("description")
        });

        if self.config.remove_default_message {
            obj.props.retain(|p| {
                let PropOrSpread::Prop(prop) = p else { return true };
                let Prop::KeyValue(kv) = prop.as_ref() else { return true };
                prop_name_as_str(&kv.key).as_deref() != Some("defaultMessage")
            });
        } else if let Some(msg) = &normalized_msg {
            replace_object_value(obj, "defaultMessage", string_expr(msg));
        }
    }
}

// =====================================================================
// JSX opening-element handler
// =====================================================================
impl FormatJsTransform {
    fn handle_jsx_opening(&self, n: &mut JSXOpeningElement) {
        let JSXElementName::Ident(name_ident) = &n.name else { return };
        if !self
            .component_names
            .iter()
            .any(|c| c.as_str() == &*name_ident.sym)
        {
            return;
        }

        let mut existing_id: Option<String> = None;
        let mut default_message: Option<String> = None;
        let mut description: Option<String> = None;
        let mut has_id_attr = false;
        let mut has_default_message_attr = false;

        for attr in &n.attrs {
            // Babel filters out spread attrs via `.filter(isJSXAttribute)` and
            // processes whatever explicit attrs remain. The descriptor is
            // assumed to come from the spread "elsewhere"; we mirror that —
            // not an error case (would break legitimate `{...rest}` usage).
            let JSXAttrOrSpread::JSXAttr(a) = attr else { continue };

            let name_str = match &a.name {
                JSXAttrName::Ident(i) => i.sym.to_string(),
                // namespaced names (`xmlns:href`) can't be id/defaultMessage/description
                JSXAttrName::JSXNamespacedName(_) => continue,
            };

            if !is_descriptor_key(&name_str) {
                continue;
            }

            // For our 3 keys, the value MUST be a static string.
            let val = match &a.value {
                None => fail(
                    a.span,
                    format!("`{}` JSX attribute requires a value", name_str),
                ),
                Some(JSXAttrValue::Str(s)) => s.value.to_atom_lossy().to_string(),
                Some(JSXAttrValue::JSXExprContainer(c)) => match &c.expr {
                    JSXExpr::Expr(e) => static_string(e).unwrap_or_else(|| {
                        fail(
                            c.span,
                            format!(
                                "`{}` JSX attribute must be a string literal — got a {}. \
                                 Constant folding (string concatenation, identifier references) is not \
                                 supported by this Rust port; inline the literal or extend the plugin.",
                                name_str,
                                expr_kind(e)
                            ),
                        )
                    }),
                    JSXExpr::JSXEmptyExpr(_) => fail(
                        c.span,
                        format!("`{}` JSX attribute is empty (`{{}}`)", name_str),
                    ),
                },
                Some(JSXAttrValue::JSXElement(e)) => fail(
                    e.span,
                    format!(
                        "`{}` JSX attribute cannot be a JSX element",
                        name_str
                    ),
                ),
                Some(JSXAttrValue::JSXFragment(f)) => fail(
                    f.span,
                    format!(
                        "`{}` JSX attribute cannot be a JSX fragment",
                        name_str
                    ),
                ),
            };

            match name_str.as_str() {
                "id" => {
                    has_id_attr = true;
                    if !val.is_empty() {
                        existing_id = Some(val);
                    }
                }
                "defaultMessage" => {
                    has_default_message_attr = true;
                    default_message = Some(val);
                }
                "description" => {
                    description = Some(val);
                }
                _ => unreachable!(),
            }
        }

        // Babel: `if (!descriptorPath.defaultMessage) return;` — JSX without a
        // `defaultMessage` attribute is assumed to come from a spread and is
        // extracted elsewhere. Match that behavior exactly.
        if !has_default_message_attr {
            return;
        }

        // Empty `defaultMessage=""` with no id: babel's `storeMessage` throws.
        if existing_id.is_none() && default_message.as_deref() == Some("") {
            fail(
                n.span,
                "<FormattedMessage> has empty `defaultMessage` and no `id` — cannot generate an id",
            );
        }

        let normalized_msg = default_message.as_ref().map(|m| {
            if self.config.preserve_whitespace {
                m.clone()
            } else {
                normalize_whitespace(m)
            }
        });

        let final_id = compute_id(
            existing_id.as_deref(),
            normalized_msg.as_deref(),
            description.as_deref(),
            self.config
                .id_interpolation_pattern
                .as_deref()
                .unwrap_or(DEFAULT_ID_INTERPOLATION_PATTERN),
        );

        if !final_id.is_empty() {
            if has_id_attr {
                replace_jsx_attr_value(n, "id", string_jsx_value(&final_id));
            } else {
                n.attrs.insert(
                    0,
                    JSXAttrOrSpread::JSXAttr(JSXAttr {
                        span: DUMMY_SP,
                        name: JSXAttrName::Ident(IdentName {
                            span: DUMMY_SP,
                            sym: "id".into(),
                        }),
                        value: Some(string_jsx_value(&final_id)),
                    }),
                );
            }
        }

        n.attrs.retain(|attr| {
            let JSXAttrOrSpread::JSXAttr(a) = attr else { return true };
            let JSXAttrName::Ident(id) = &a.name else { return true };
            id.sym != "description"
        });

        if self.config.remove_default_message {
            n.attrs.retain(|attr| {
                let JSXAttrOrSpread::JSXAttr(a) = attr else { return true };
                let JSXAttrName::Ident(id) = &a.name else { return true };
                id.sym != "defaultMessage"
            });
        } else if let Some(msg) = &normalized_msg {
            replace_jsx_attr_value(n, "defaultMessage", string_jsx_value(msg));
        }
    }
}

// =====================================================================
// Shared helpers
// =====================================================================

fn is_descriptor_key(key: &str) -> bool {
    matches!(key, "id" | "defaultMessage" | "description")
}

fn compute_id(
    existing_id: Option<&str>,
    default_message: Option<&str>,
    description: Option<&str>,
    pattern: &str,
) -> String {
    if let Some(id) = existing_id {
        if !id.is_empty() {
            return id.to_string();
        }
    }
    let Some(msg) = default_message else { return String::new() };
    if msg.is_empty() {
        return String::new();
    }
    let content = match description {
        Some(d) if !d.is_empty() => format!("{}#{}", msg, d),
        _ => msg.to_string(),
    };
    interpolate_pattern(pattern, &content)
}

fn callee_ident_name(e: &Expr) -> Option<(String, bool)> {
    match e {
        Expr::Ident(id) => Some((id.sym.to_string(), false)),
        Expr::Member(m) => match &m.prop {
            MemberProp::Ident(id) => Some((id.sym.to_string(), true)),
            // Computed `obj[expr](...)`. Babel's matcher is
            // `property.isIdentifier({name})` — it returns true ONLY when
            // the computed expression is a bare `Identifier` node whose
            // `.name` matches a configured function name. String literals
            // (`obj["formatMessage"]`) and complex expressions (`obj[fn()]`)
            // return false, and babel SILENTLY does not extract from them
            // (no error). Match that exactly.
            MemberProp::Computed(c) => match &*c.expr {
                Expr::Ident(id) => Some((id.sym.to_string(), true)),
                _ => None,
            },
            MemberProp::PrivateName(_) => None,
        },
        _ => None,
    }
}

fn require_static_key(name: &PropName) -> String {
    match name {
        PropName::Ident(id) => id.sym.to_string(),
        PropName::Str(s) => s.value.to_atom_lossy().to_string(),
        PropName::Computed(c) => fail(
            c.span,
            "computed property keys (`[expr]: value`) are not supported inside a message \
             descriptor — use a literal key (`id` / `defaultMessage` / `description`)",
        ),
        PropName::Num(n) => fail(
            n.span,
            "numeric property keys are not supported inside a message descriptor",
        ),
        PropName::BigInt(b) => fail(
            b.span,
            "bigint property keys are not supported inside a message descriptor",
        ),
    }
}

fn prop_name_as_str(name: &PropName) -> Option<String> {
    // Used only after `require_static_key` has already validated; the
    // Computed/Num/BigInt branches won't appear here in practice.
    match name {
        PropName::Ident(id) => Some(id.sym.to_string()),
        PropName::Str(s) => Some(s.value.to_atom_lossy().to_string()),
        _ => None,
    }
}

fn require_static_string(e: &Expr, key: &str) -> String {
    static_string(e).unwrap_or_else(|| {
        fail(
            e.span(),
            format!(
                "`{}` in a message descriptor must be a string literal — got a {}. \
                 Constant folding (e.g. `'a' + b`, identifier references, ternaries) is not \
                 supported by this Rust port; inline the literal or extend the plugin.",
                key,
                expr_kind(e)
            ),
        )
    })
}

fn unwrap_ts(e: &Expr) -> &Expr {
    let mut cur = e;
    loop {
        match cur {
            Expr::TsAs(t) => cur = &t.expr,
            Expr::TsTypeAssertion(t) => cur = &t.expr,
            Expr::TsNonNull(t) => cur = &t.expr,
            Expr::TsConstAssertion(t) => cur = &t.expr,
            Expr::TsSatisfies(t) => cur = &t.expr,
            _ => return cur,
        }
    }
}

fn unwrap_ts_mut(mut e: &mut Expr) -> &mut Expr {
    loop {
        let is_ts = matches!(
            e,
            Expr::TsAs(_)
                | Expr::TsTypeAssertion(_)
                | Expr::TsNonNull(_)
                | Expr::TsConstAssertion(_)
                | Expr::TsSatisfies(_)
        );
        if !is_ts {
            return e;
        }
        e = match e {
            Expr::TsAs(t) => &mut *t.expr,
            Expr::TsTypeAssertion(t) => &mut *t.expr,
            Expr::TsNonNull(t) => &mut *t.expr,
            Expr::TsConstAssertion(t) => &mut *t.expr,
            Expr::TsSatisfies(t) => &mut *t.expr,
            _ => unreachable!(),
        };
    }
}

fn static_string(e: &Expr) -> Option<String> {
    let e = unwrap_ts(e);
    match e {
        Expr::Lit(Lit::Str(s)) => Some(s.value.to_atom_lossy().to_string()),
        // No-substitution template literal: `foo bar` (no ${} interpolations).
        Expr::Tpl(t) if t.exprs.is_empty() && t.quasis.len() == 1 => t.quasis[0]
            .cooked
            .as_ref()
            .map(|c| c.to_atom_lossy().to_string()),
        _ => None,
    }
}

/// Human-friendly description of an expression's shape — used in error
/// messages so the developer can see exactly why their code wasn't accepted.
fn expr_kind(e: &Expr) -> &'static str {
    match e {
        Expr::Lit(Lit::Str(_)) => "string literal",
        Expr::Lit(Lit::Num(_)) => "number literal",
        Expr::Lit(Lit::Bool(_)) => "boolean literal",
        Expr::Lit(Lit::Null(_)) => "null",
        Expr::Lit(_) => "non-string literal",
        Expr::Ident(_) => "identifier reference",
        Expr::Bin(_) => "binary expression (e.g. `a + b`)",
        Expr::Tpl(_) => "template literal with `${...}` interpolation",
        Expr::Call(_) => "function call",
        Expr::Member(_) => "member access (e.g. `x.y`)",
        Expr::Cond(_) => "ternary expression",
        Expr::Object(_) => "object literal",
        Expr::Array(_) => "array literal",
        Expr::Paren(_) => "parenthesised expression",
        _ => "non-literal expression",
    }
}

fn string_expr(s: &str) -> Expr {
    Expr::Lit(Lit::Str(Str {
        span: DUMMY_SP,
        value: s.into(),
        raw: None,
    }))
}

fn string_jsx_value(s: &str) -> JSXAttrValue {
    JSXAttrValue::Str(Str {
        span: DUMMY_SP,
        value: s.into(),
        raw: None,
    })
}

fn replace_object_value(obj: &mut ObjectLit, key: &str, new_value: Expr) {
    for prop in obj.props.iter_mut() {
        let PropOrSpread::Prop(p) = prop else { continue };
        let Prop::KeyValue(kv) = p.as_mut() else { continue };
        if prop_name_as_str(&kv.key).as_deref() == Some(key) {
            kv.value = Box::new(new_value);
            return;
        }
    }
}

fn replace_jsx_attr_value(n: &mut JSXOpeningElement, key: &str, new_value: JSXAttrValue) {
    for attr in n.attrs.iter_mut() {
        let JSXAttrOrSpread::JSXAttr(a) = attr else { continue };
        let JSXAttrName::Ident(id) = &a.name else { continue };
        if id.sym == key {
            a.value = Some(new_value);
            return;
        }
    }
}

// =====================================================================
// WASI plugin entry point (only compiled with `--features plugin`).
// =====================================================================
#[cfg(feature = "plugin")]
mod plugin_entry {
    use super::*;
    use swc_core::plugin::metadata::TransformPluginMetadataContextKind;
    use swc_core::plugin::{metadata::TransformPluginProgramMetadata, plugin_transform};

    #[plugin_transform]
    pub fn process_transform(
        mut program: Program,
        metadata: TransformPluginProgramMetadata,
    ) -> Program {
        // Stash the source file path before parsing config or running the
        // visitor so any `fail(...)` we hit can include it in the message.
        // The host hands us a path string here (relative to the project
        // root) — we just forward it.
        let filename = metadata.get_context(&TransformPluginMetadataContextKind::Filename);
        set_current_file(filename);

        let raw = metadata
            .get_transform_plugin_config()
            .unwrap_or_else(|| "{}".to_string());
        let config: Config = match serde_json::from_str(&raw) {
            Ok(c) => c,
            Err(e) => fail(
                DUMMY_SP,
                format!("failed to parse plugin config: {}", e),
            ),
        };
        program.visit_mut_with(&mut FormatJsTransform::new(config));

        // Clear so a subsequent invocation in the same plugin instance
        // (host reuses instances) can't accidentally attribute its error
        // to the previous file.
        set_current_file(None);

        program
    }
}

#[cfg(not(feature = "plugin"))]
#[allow(dead_code)]
fn _force_program_used(_: &Program) {}
