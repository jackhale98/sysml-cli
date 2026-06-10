//! Interactive wizard framework for multi-step CLI flows.
//!
//! Defines the data model for wizards — the CLI layer provides the actual
//! terminal interaction using dialoguer. This module is deliberately free of
//! any I/O or terminal dependencies so it can be tested and used from any
//! frontend.

use serde::Serialize;
use std::collections::BTreeMap;

// ========================================================================
// Wizard step definitions
// ========================================================================

/// A single step in an interactive wizard.
#[derive(Debug, Clone, Serialize)]
pub struct WizardStep {
    /// Unique identifier used as the key in [`WizardResult`].
    pub id: String,
    /// The question presented to the user.
    pub prompt: String,
    /// Optional teaching text for non-software engineers.
    pub explanation: Option<String>,
    /// What kind of input this step expects.
    pub kind: PromptKind,
    /// Whether the user must provide a value (cannot skip).
    pub required: bool,
    /// Default value shown in the prompt.
    pub default: Option<String>,
}

/// The kind of input a wizard step expects.
#[derive(Debug, Clone, Serialize)]
pub enum PromptKind {
    /// Free-form text input.
    String,
    /// Select exactly one option from a list.
    Choice(Vec<ChoiceOption>),
    /// Yes/no confirmation.
    Confirm,
    /// Numeric input with optional bounds.
    Number { min: Option<f64>, max: Option<f64> },
    /// Select zero or more options from a list.
    MultiSelect(Vec<ChoiceOption>),
}

/// A single option within a [`PromptKind::Choice`] or [`PromptKind::MultiSelect`].
#[derive(Debug, Clone, Serialize)]
pub struct ChoiceOption {
    /// The programmatic value stored in the result.
    pub value: String,
    /// Human-readable label shown to the user.
    pub label: String,
    /// Optional description shown alongside the label.
    pub description: Option<String>,
}

impl ChoiceOption {
    /// Create a choice option with a value and label.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    /// Attach a description to this option (builder pattern).
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

// ========================================================================
// WizardStep convenience constructors
// ========================================================================

impl WizardStep {
    /// Create a free-form text prompt.
    pub fn string(id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            explanation: None,
            kind: PromptKind::String,
            required: true,
            default: None,
        }
    }

    /// Create a single-selection choice prompt from `(value, label)` pairs.
    pub fn choice(
        id: impl Into<String>,
        prompt: impl Into<String>,
        options: Vec<(&str, &str)>,
    ) -> Self {
        let choices = options
            .into_iter()
            .map(|(v, l)| ChoiceOption::new(v, l))
            .collect();
        Self {
            id: id.into(),
            prompt: prompt.into(),
            explanation: None,
            kind: PromptKind::Choice(choices),
            required: true,
            default: None,
        }
    }

    /// Create a yes/no confirmation prompt.
    pub fn confirm(id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            explanation: None,
            kind: PromptKind::Confirm,
            required: true,
            default: None,
        }
    }

    /// Create a numeric input prompt with no bounds.
    pub fn number(id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            explanation: None,
            kind: PromptKind::Number {
                min: None,
                max: None,
            },
            required: true,
            default: None,
        }
    }

    /// Create a multi-select prompt from `(value, label)` pairs.
    pub fn multi_select(
        id: impl Into<String>,
        prompt: impl Into<String>,
        options: Vec<(&str, &str)>,
    ) -> Self {
        let choices = options
            .into_iter()
            .map(|(v, l)| ChoiceOption::new(v, l))
            .collect();
        Self {
            id: id.into(),
            prompt: prompt.into(),
            explanation: None,
            kind: PromptKind::MultiSelect(choices),
            required: true,
            default: None,
        }
    }

    /// Attach an explanation for non-experts (builder pattern).
    pub fn with_explanation(mut self, text: &str) -> Self {
        self.explanation = Some(text.to_owned());
        self
    }

    /// Set the default value shown in the prompt (builder pattern).
    pub fn with_default(mut self, val: &str) -> Self {
        self.default = Some(val.to_owned());
        self
    }

    /// Mark this step as optional so the user can skip it (builder pattern).
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Set numeric bounds (only meaningful for [`PromptKind::Number`]).
    pub fn with_bounds(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        if let PromptKind::Number {
            min: ref mut m,
            max: ref mut x,
        } = self.kind
        {
            *m = min;
            *x = max;
        }
        self
    }
}

// ========================================================================
// Wizard answers and results
// ========================================================================

/// A single answer captured from the user.
#[derive(Debug, Clone, Serialize)]
pub enum WizardAnswer {
    /// Free-form text.
    String(String),
    /// Boolean confirmation.
    Bool(bool),
    /// Numeric value.
    Number(f64),
    /// One or more selected values from a choice/multi-select.
    Selected(Vec<String>),
    /// The step was skipped (only valid for optional steps).
    Skipped,
}

/// The collected answers from running a wizard, keyed by step ID.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WizardResult {
    answers: BTreeMap<String, WizardAnswer>,
}

impl WizardResult {
    /// Create an empty result set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store an answer for the given step ID.
    pub fn set(&mut self, id: impl Into<String>, answer: WizardAnswer) {
        self.answers.insert(id.into(), answer);
    }

    /// Check whether an answer exists (and is not [`WizardAnswer::Skipped`]).
    pub fn has(&self, id: &str) -> bool {
        matches!(
            self.answers.get(id),
            Some(answer) if !matches!(answer, WizardAnswer::Skipped)
        )
    }

    /// Retrieve a string answer, returning `None` if missing or wrong type.
    pub fn get_string(&self, id: &str) -> Option<&str> {
        match self.answers.get(id) {
            Some(WizardAnswer::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Retrieve a boolean answer, returning `None` if missing or wrong type.
    pub fn get_bool(&self, id: &str) -> Option<bool> {
        match self.answers.get(id) {
            Some(WizardAnswer::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// Retrieve a numeric answer, returning `None` if missing or wrong type.
    pub fn get_number(&self, id: &str) -> Option<f64> {
        match self.answers.get(id) {
            Some(WizardAnswer::Number(n)) => Some(*n),
            _ => None,
        }
    }

    /// Retrieve selected values, returning `None` if missing or wrong type.
    pub fn get_selected(&self, id: &str) -> Option<&[String]> {
        match self.answers.get(id) {
            Some(WizardAnswer::Selected(v)) => Some(v.as_slice()),
            _ => None,
        }
    }

    /// Return all step IDs that have answers.
    pub fn answered_ids(&self) -> Vec<&str> {
        self.answers.keys().map(|k| k.as_str()).collect()
    }

    /// Return the number of non-skipped answers.
    pub fn count(&self) -> usize {
        self.answers
            .values()
            .filter(|a| !matches!(a, WizardAnswer::Skipped))
            .count()
    }

    /// Consume the result and return the underlying map.
    pub fn into_map(self) -> BTreeMap<String, WizardAnswer> {
        self.answers
    }
}

// ========================================================================
// Runner trait — implemented by the CLI layer
// ========================================================================

/// Trait for executing wizard steps against a user interface.
///
/// The core library defines wizard flows as sequences of [`WizardStep`]s.
/// The CLI layer implements this trait to present them via dialoguer, while
/// tests can provide a mock implementation.
pub trait WizardRunner {
    /// Present a single step to the user and collect an answer.
    ///
    /// Returns `None` if the user cancels (e.g. Ctrl-C).
    fn run_step(&self, step: &WizardStep) -> Option<WizardAnswer>;

    /// Whether this runner is connected to a real interactive terminal.
    fn is_interactive(&self) -> bool;
}

/// Run a full sequence of wizard steps, collecting all answers.
///
/// Returns `None` if the user cancels at any point.
pub fn run_wizard(runner: &dyn WizardRunner, steps: &[WizardStep]) -> Option<WizardResult> {
    let mut result = WizardResult::new();
    for step in steps {
        match runner.run_step(step) {
            Some(answer) => result.set(&step.id, answer),
            None => return None,
        }
    }
    Some(result)
}

// ========================================================================
// Tests
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- WizardStep builder tests ------------------------------------------

    #[test]
    fn string_step_defaults() {
        let step = WizardStep::string("name", "What is the part name?");
        assert_eq!(step.id, "name");
        assert_eq!(step.prompt, "What is the part name?");
        assert!(step.required);
        assert!(step.default.is_none());
        assert!(step.explanation.is_none());
        assert!(matches!(step.kind, PromptKind::String));
    }

    #[test]
    fn choice_step_builds_options() {
        let step = WizardStep::choice(
            "kind",
            "Select element kind",
            vec![("part", "Part"), ("port", "Port"), ("action", "Action")],
        );
        assert_eq!(step.id, "kind");
        assert!(step.required);
        if let PromptKind::Choice(opts) = &step.kind {
            assert_eq!(opts.len(), 3);
            assert_eq!(opts[0].value, "part");
            assert_eq!(opts[0].label, "Part");
            assert!(opts[0].description.is_none());
        } else {
            panic!("expected Choice variant");
        }
    }

    #[test]
    fn confirm_step() {
        let step = WizardStep::confirm("proceed", "Continue?");
        assert!(matches!(step.kind, PromptKind::Confirm));
    }

    #[test]
    fn number_step() {
        let step = WizardStep::number("count", "How many?");
        assert!(matches!(
            step.kind,
            PromptKind::Number {
                min: None,
                max: None
            }
        ));
    }

    #[test]
    fn multi_select_step() {
        let step = WizardStep::multi_select(
            "features",
            "Select features",
            vec![("a", "Alpha"), ("b", "Beta")],
        );
        if let PromptKind::MultiSelect(opts) = &step.kind {
            assert_eq!(opts.len(), 2);
        } else {
            panic!("expected MultiSelect variant");
        }
    }

    #[test]
    fn builder_chains() {
        let step = WizardStep::string("name", "Name?")
            .with_explanation("The identifier for this element.")
            .with_default("Untitled")
            .optional();

        assert!(!step.required);
        assert_eq!(step.default.as_deref(), Some("Untitled"));
        assert_eq!(
            step.explanation.as_deref(),
            Some("The identifier for this element.")
        );
    }

    #[test]
    fn number_bounds() {
        let step =
            WizardStep::number("weight", "Weight (kg)?").with_bounds(Some(0.0), Some(1000.0));
        if let PromptKind::Number { min, max } = step.kind {
            assert_eq!(min, Some(0.0));
            assert_eq!(max, Some(1000.0));
        } else {
            panic!("expected Number variant");
        }
    }

    #[test]
    fn bounds_ignored_on_non_number() {
        let step = WizardStep::string("x", "X?").with_bounds(Some(1.0), Some(2.0));
        // Should remain String, bounds silently ignored
        assert!(matches!(step.kind, PromptKind::String));
    }

    // -- ChoiceOption tests ------------------------------------------------

    #[test]
    fn choice_option_with_description() {
        let opt = ChoiceOption::new("val", "Label").with_description("Some help text");
        assert_eq!(opt.value, "val");
        assert_eq!(opt.label, "Label");
        assert_eq!(opt.description.as_deref(), Some("Some help text"));
    }

    // -- WizardResult tests ------------------------------------------------

    #[test]
    fn result_starts_empty() {
        let r = WizardResult::new();
        assert_eq!(r.count(), 0);
        assert!(!r.has("anything"));
    }

    #[test]
    fn set_and_get_string() {
        let mut r = WizardResult::new();
        r.set("name", WizardAnswer::String("Vehicle".into()));
        assert!(r.has("name"));
        assert_eq!(r.get_string("name"), Some("Vehicle"));
        // Wrong accessor returns None
        assert_eq!(r.get_bool("name"), None);
        assert_eq!(r.get_number("name"), None);
        assert_eq!(r.get_selected("name"), None);
    }

    #[test]
    fn set_and_get_bool() {
        let mut r = WizardResult::new();
        r.set("confirm", WizardAnswer::Bool(true));
        assert!(r.has("confirm"));
        assert_eq!(r.get_bool("confirm"), Some(true));
        assert_eq!(r.get_string("confirm"), None);
    }

    #[test]
    fn set_and_get_number() {
        let mut r = WizardResult::new();
        r.set("mass", WizardAnswer::Number(42.5));
        assert!(r.has("mass"));
        assert_eq!(r.get_number("mass"), Some(42.5));
    }

    #[test]
    fn set_and_get_selected() {
        let mut r = WizardResult::new();
        r.set(
            "features",
            WizardAnswer::Selected(vec!["a".into(), "b".into()]),
        );
        assert!(r.has("features"));
        assert_eq!(
            r.get_selected("features"),
            Some(["a".to_owned(), "b".to_owned()].as_slice())
        );
    }

    #[test]
    fn skipped_not_counted_by_has() {
        let mut r = WizardResult::new();
        r.set("opt", WizardAnswer::Skipped);
        assert!(!r.has("opt"));
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn missing_key_returns_none() {
        let r = WizardResult::new();
        assert_eq!(r.get_string("nope"), None);
        assert_eq!(r.get_bool("nope"), None);
        assert_eq!(r.get_number("nope"), None);
        assert_eq!(r.get_selected("nope"), None);
    }

    #[test]
    fn answered_ids_in_order() {
        let mut r = WizardResult::new();
        r.set("z_last", WizardAnswer::Bool(true));
        r.set("a_first", WizardAnswer::String("hi".into()));
        // BTreeMap is sorted, so a_first comes first
        assert_eq!(r.answered_ids(), vec!["a_first", "z_last"]);
    }

    #[test]
    fn count_excludes_skipped() {
        let mut r = WizardResult::new();
        r.set("a", WizardAnswer::String("yes".into()));
        r.set("b", WizardAnswer::Skipped);
        r.set("c", WizardAnswer::Bool(false));
        assert_eq!(r.count(), 2);
    }

    #[test]
    fn into_map_ownership() {
        let mut r = WizardResult::new();
        r.set("x", WizardAnswer::Number(1.0));
        let map = r.into_map();
        assert!(map.contains_key("x"));
    }

    // -- WizardRunner + run_wizard tests -----------------------------------

    /// A mock runner that returns pre-loaded answers in order.
    struct MockRunner {
        answers: Vec<Option<WizardAnswer>>,
        interactive: bool,
    }

    impl MockRunner {
        fn new(answers: Vec<Option<WizardAnswer>>) -> Self {
            Self {
                answers,
                interactive: true,
            }
        }

        fn non_interactive() -> Self {
            Self {
                answers: vec![],
                interactive: false,
            }
        }
    }

    impl WizardRunner for MockRunner {
        fn run_step(&self, step: &WizardStep) -> Option<WizardAnswer> {
            // Find the answer by position — use the step id to index into our
            // answers by finding which step number this is. For simplicity we
            // use a cell to track the call count.
            use std::sync::atomic::{AtomicUsize, Ordering};
            static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
            // Reset on first step (id contains "0" or we just count calls)
            let idx = if step.id.ends_with("_0") {
                CALL_COUNT.store(1, Ordering::SeqCst);
                0
            } else {
                CALL_COUNT.fetch_add(1, Ordering::SeqCst)
            };
            self.answers
                .get(idx)
                .cloned()
                .unwrap_or(Some(WizardAnswer::Skipped))
        }

        fn is_interactive(&self) -> bool {
            self.interactive
        }
    }

    #[test]
    fn run_wizard_collects_answers() {
        let steps = vec![
            WizardStep::string("step_0", "Name?"),
            WizardStep::confirm("step_1", "OK?"),
        ];
        let runner = MockRunner::new(vec![
            Some(WizardAnswer::String("Test".into())),
            Some(WizardAnswer::Bool(true)),
        ]);
        let result = run_wizard(&runner, &steps).expect("should succeed");
        assert_eq!(result.get_string("step_0"), Some("Test"));
        assert_eq!(result.get_bool("step_1"), Some(true));
    }

    #[test]
    fn run_wizard_cancellation() {
        let steps = vec![
            WizardStep::string("step_0", "Name?"),
            WizardStep::confirm("step_1", "OK?"),
        ];
        let runner = MockRunner::new(vec![
            Some(WizardAnswer::String("Test".into())),
            None, // user cancels
        ]);
        assert!(run_wizard(&runner, &steps).is_none());
    }

    #[test]
    fn is_interactive_flag() {
        let runner = MockRunner::non_interactive();
        assert!(!runner.is_interactive());
    }
}
