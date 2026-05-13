use biome_analyze::{
    Ast, Rule, RuleDiagnostic, RuleSource, context::RuleContext, declare_lint_rule,
};
use biome_js_syntax::JsModule;
use biome_rule_options::no_duplicate_code::NoDuplicateCodeOptions;

declare_lint_rule! {
    /// Disallow duplicate code blocks.
    ///
    /// This rule detects duplicate code blocks across files and reports them as violations.
    /// Code duplication can lead to maintenance issues and bugs that are hard to track down.
    ///
    /// ## Examples
    ///
    /// ### Invalid
    ///
    /// ```ts,expect_diagnostic
    /// function a() {
    ///   const x = 1;
    ///   const y = 2;
    ///   return x + y;
    /// }
    ///
    /// function b() {
    ///   const x = 1;
    ///   const y = 2;
    ///   return x + y;
    /// }
    /// ```
    ///
    /// ### Valid
    ///
    /// ```ts
    /// function add(a: number, b: number) {
    ///   return a + b;
    /// }
    ///
    /// function multiply(a: number, b: number) {
    ///   return a * b;
    /// }
    /// ```
    ///
    pub NoDuplicateCode {
        version: "next",
        name: "noDuplicateCode",
        language: "js",
        recommended: false,
        sources: &[RuleSource::EslintUnicorn("duplicate-code").same()],
    }
}

impl Rule for NoDuplicateCode {
    type Query = Ast<JsModule>;
    type State = ();
    type Signals = Option<Self::State>;
    type Options = NoDuplicateCodeOptions;

    fn run(_ctx: &RuleContext<Self>) -> Self::Signals {
        // Per-file tokenization is handled by the crawler in process_file.
        // Cross-file analysis is performed in the finalizer.
        // This rule does not emit diagnostics at the individual file level.
        None
    }

    fn diagnostic(_ctx: &RuleContext<Self>, _state: &Self::State) -> Option<RuleDiagnostic> {
        None
    }
}
