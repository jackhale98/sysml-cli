/// CLI wizard runner — implements WizardRunner using dialoguer.

use sysml_core::interactive::*;

/// A wizard runner that uses dialoguer for terminal interaction.
pub struct CliWizardRunner {
    interactive: bool,
}

impl CliWizardRunner {
    /// Create a new CLI wizard runner.
    pub fn new() -> Self {
        use std::io::IsTerminal;
        Self {
            interactive: std::io::stderr().is_terminal(),
        }
    }
}

impl WizardRunner for CliWizardRunner {
    fn run_step(&self, step: &WizardStep) -> Option<WizardAnswer> {
        use dialoguer::{Confirm, FuzzySelect, Input, MultiSelect};

        if !self.interactive {
            eprintln!("error: wizard requires an interactive terminal");
            return None;
        }

        // Show explanation if present
        if let Some(ref explanation) = step.explanation {
            eprintln!("  {}", explanation);
        }

        match &step.kind {
            PromptKind::String => {
                let mut input = Input::<String>::new().with_prompt(&step.prompt);
                if let Some(ref default) = step.default {
                    input = input.default(default.clone());
                }
                if !step.required {
                    input = input.allow_empty(true);
                }
                match input.interact_text() {
                    Ok(val) if val.is_empty() && !step.required => Some(WizardAnswer::Skipped),
                    Ok(val) => Some(WizardAnswer::String(val)),
                    Err(_) => None,
                }
            }
            PromptKind::Choice(options) => {
                let labels: Vec<String> = options
                    .iter()
                    .map(|o| {
                        if let Some(ref desc) = o.description {
                            format!("{} — {}", o.label, desc)
                        } else {
                            o.label.clone()
                        }
                    })
                    .collect();
                match FuzzySelect::new()
                    .with_prompt(&step.prompt)
                    .items(&labels)
                    .default(0)
                    .interact_opt()
                {
                    Ok(Some(idx)) => Some(WizardAnswer::String(options[idx].value.clone())),
                    _ => None,
                }
            }
            PromptKind::Confirm => {
                let default = step
                    .default
                    .as_ref()
                    .map(|d| d == "true" || d == "yes")
                    .unwrap_or(true);
                match Confirm::new()
                    .with_prompt(&step.prompt)
                    .default(default)
                    .interact_opt()
                {
                    Ok(Some(val)) => Some(WizardAnswer::Bool(val)),
                    _ => None,
                }
            }
            PromptKind::Number { min, max } => {
                let mut input = Input::<String>::new().with_prompt(&step.prompt);
                if let Some(ref default) = step.default {
                    input = input.default(default.clone());
                }
                match input.interact_text() {
                    Ok(val) => match val.parse::<f64>() {
                        Ok(n) => {
                            if let Some(lo) = min {
                                if n < *lo {
                                    eprintln!("  value must be >= {}", lo);
                                    return self.run_step(step);
                                }
                            }
                            if let Some(hi) = max {
                                if n > *hi {
                                    eprintln!("  value must be <= {}", hi);
                                    return self.run_step(step);
                                }
                            }
                            Some(WizardAnswer::Number(n))
                        }
                        Err(_) => {
                            eprintln!("  please enter a number");
                            self.run_step(step)
                        }
                    },
                    Err(_) => None,
                }
            }
            PromptKind::MultiSelect(options) => {
                let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
                match MultiSelect::new()
                    .with_prompt(&step.prompt)
                    .items(&labels)
                    .interact_opt()
                {
                    Ok(Some(indices)) => {
                        let selected: Vec<String> = indices
                            .iter()
                            .map(|&i| options[i].value.clone())
                            .collect();
                        Some(WizardAnswer::Selected(selected))
                    }
                    _ => None,
                }
            }
        }
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }
}
