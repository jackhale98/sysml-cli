/// Simulation engine for SysML v2 behavioral constructs.
///
/// Provides expression evaluation, constraint checking, calculation
/// evaluation, state machine simulation, and action flow execution.

pub mod action_exec;
pub mod analysis;
pub mod action_flow;
pub mod action_parser;
pub mod constraint_eval;
pub mod eval;
pub mod expr;
pub mod expr_parser;
pub mod state_machine;
pub mod state_parser;
pub mod resolve;
pub mod rollup;
pub mod state_sim;
pub mod units;
pub mod what_if;
