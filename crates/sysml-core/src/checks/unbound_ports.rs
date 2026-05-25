/// Flags port usages that exist inside a part definition but never appear
/// as an endpoint of any connection.  In a properly-wired model, every port
/// declared inside a `part def` should either be:
///   - connected via a `connect ... to ...` or `flow from ... to ...`
///   - explicitly marked as part of the public interface (no body)
///
/// Ports declared inside `port def`/`interface def` themselves (which define
/// the structure of a port, not instances of one) are exempt.

use std::collections::HashSet;

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{simple_name, DefKind, Model};

pub struct UnboundPortCheck;

impl Check for UnboundPortCheck {
    fn name(&self) -> &'static str {
        "unbound-port"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        // Collect all port endpoints from connections and flows.
        let mut bound: HashSet<String> = HashSet::new();
        for c in &model.connections {
            bound.insert(simple_name(&c.source).to_string());
            bound.insert(simple_name(&c.target).to_string());
        }
        for f in &model.flows {
            bound.insert(simple_name(&f.source).to_string());
            bound.insert(simple_name(&f.target).to_string());
        }

        let mut diagnostics = Vec::new();
        for u in &model.usages {
            if u.kind != "port" {
                continue;
            }
            // Skip ports declared inside port-def or interface-def bodies.
            if let Some(parent) = &u.parent_def {
                if let Some(p) = model.find_def(parent) {
                    if matches!(p.kind, DefKind::Port | DefKind::Interface) {
                        continue;
                    }
                }
            }
            // Only flag ports that have a part-def parent (i.e., instances).
            if u.parent_def.is_none() {
                continue;
            }
            if bound.contains(u.name.as_str()) {
                continue;
            }
            diagnostics.push(
                Diagnostic::warning(
                    &model.file,
                    u.span.clone(),
                    codes::UNBOUND_PORT,
                    format!(
                        "port `{}` (in `{}`) is declared but never connected",
                        u.name,
                        u.parent_def.as_deref().unwrap_or("?")
                    ),
                )
                .with_suggestion(format!(
                    "connect it (e.g., `connect {}.{} to <other>`) or remove it",
                    u.parent_def.as_deref().unwrap_or("?"),
                    u.name
                )),
            );
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn connected_port_does_not_warn() {
        let source = r#"
            port def Power;
            part def Vehicle {
                port p : Power;
                port q : Power;
                connect p to q;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let diags = UnboundPortCheck.run(&model);
        assert!(
            diags.is_empty(),
            "connected ports should not warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unconnected_port_warns() {
        let source = r#"
            port def Power;
            part def Vehicle {
                port unused : Power;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let diags = UnboundPortCheck.run(&model);
        assert!(
            diags.iter().any(|d| d.code == codes::UNBOUND_PORT
                && d.message.contains("unused")),
            "unconnected port should warn: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn port_inside_port_def_is_exempt() {
        let source = r#"
            port def MultiPort {
                port a : Power;
                port b : Power;
            }
            port def Power;
        "#;
        let model = parse_file("test.sysml", source);
        let diags = UnboundPortCheck.run(&model);
        assert!(
            diags.is_empty(),
            "ports inside port-def bodies should be exempt (they are structure)"
        );
    }
}
