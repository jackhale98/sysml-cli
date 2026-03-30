/// Checks for port type and direction compatibility on connections.

use std::collections::HashMap;

use crate::checks::Check;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::{simple_name, Direction, Model};

pub struct PortConnectionCheck;

impl Check for PortConnectionCheck {
    fn name(&self) -> &'static str {
        "port-types"
    }

    fn run(&self, model: &Model) -> Vec<Diagnostic> {
        // Build port name -> (type, direction, is_conjugated) mapping
        let mut port_types: HashMap<&str, &str> = HashMap::new();
        let mut port_dirs: HashMap<&str, Direction> = HashMap::new();

        for u in &model.usages {
            if u.kind == "port" {
                if let Some(ref t) = u.type_ref {
                    port_types.insert(u.name.as_str(), t.as_str());
                }
                if let Some(dir) = u.direction {
                    port_dirs.insert(u.name.as_str(), dir);
                }
            }
        }

        let mut diagnostics = Vec::new();

        for conn in &model.connections {
            let src_port = simple_name(&conn.source);
            let tgt_port = simple_name(&conn.target);

            // Type compatibility check
            let src_type = port_types.get(src_port);
            let tgt_type = port_types.get(tgt_port);

            if let (Some(&st), Some(&tt)) = (src_type, tgt_type) {
                let st_base = st.strip_prefix('~').unwrap_or(st);
                let tt_base = tt.strip_prefix('~').unwrap_or(tt);

                if simple_name(st_base) != simple_name(tt_base) {
                    diagnostics.push(Diagnostic::warning(
                        &model.file,
                        conn.span.clone(),
                        codes::PORT_TYPE_MISMATCH,
                        format!(
                            "connected ports have different types: `{}` is `{}` but `{}` is `{}`",
                            src_port, st, tgt_port, tt,
                        ),
                    ));
                }
            }

            // Direction compatibility check
            let src_dir = port_dirs.get(src_port);
            let tgt_dir = port_dirs.get(tgt_port);

            if let (Some(&sd), Some(&td)) = (src_dir, tgt_dir) {
                // Compatible: out→in, in→out, inout↔anything
                let compatible = match (sd, td) {
                    (Direction::Out, Direction::In) => true,
                    (Direction::In, Direction::Out) => true,
                    (Direction::InOut, _) | (_, Direction::InOut) => true,
                    (Direction::In, Direction::In) => false,   // both consuming
                    (Direction::Out, Direction::Out) => false,  // both producing
                };
                if !compatible {
                    diagnostics.push(Diagnostic::warning(
                        &model.file,
                        conn.span.clone(),
                        codes::PORT_DIRECTION_MISMATCH,
                        format!(
                            "connected ports have incompatible directions: `{}` is {} but `{}` is {}",
                            src_port, sd.label(), tgt_port, td.label(),
                        ),
                    ));
                }
            }
        }

        diagnostics
    }
}
