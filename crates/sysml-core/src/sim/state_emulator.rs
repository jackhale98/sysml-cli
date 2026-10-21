//! Step-by-step emulation of a [`StateChart`], for interactive front ends.
//!
//! Where [`state_sim::simulate`](crate::sim::state_sim::simulate) runs a
//! flat machine through a queue of events, the emulator keeps a session the
//! caller drives one event at a time, and handles nested states and
//! parallel regions:
//!
//! - [`start`] enters the machine: the initial transition of each region,
//!   with entry actions, outermost first.
//! - [`fire`] takes one step. A named event fires the transitions it
//!   triggers (a signal, a time trigger by its text, or a change trigger
//!   by `when <condition>` while the condition holds). No event fires
//!   completion and guard-only transitions, and change triggers whose
//!   condition holds. At most one transition fires per active state, inner
//!   states first, so parallel regions each take their own step.
//! - [`enabled`] lists every transition leaving the active states, with its
//!   guard (and change condition) evaluated.
//!
//! Firing a transition runs the exit actions of the states it leaves
//! (innermost first), its effect, then the entry and `do` actions of the
//! states it enters. `assign` actions update the variables; other actions
//! are recorded in the trace only.

use std::collections::HashSet;

use serde::Serialize;

use crate::sim::expr::Env;
use crate::sim::state_chart::{ChartStateKind, ChartTransition, StateChart};
use crate::sim::state_machine::{ActionRef, Trigger};
use crate::sim::state_sim::{
    apply_action, change_condition, evaluate_guard_parts, seed_env, signal_matches, SimStatus,
    SimStep,
};

/// The state of an emulation run.
#[derive(Debug, Clone)]
pub struct ChartSession {
    /// Active leaf states (their ancestors are active too), in document
    /// order.
    pub current: Vec<String>,
    /// Number of transitions fired since the start.
    pub step: usize,
    pub env: Env,
    pub trace: Vec<SimStep>,
    pub status: SimStatus,
    /// What the last call did not do and why (an ignored event, an
    /// assignment the evaluator could not run).
    pub messages: Vec<String>,
}

/// A transition leaving an active state, as [`enabled`] reports it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnabledTransition {
    /// The transition's name, or `from -> to`.
    pub transition: String,
    /// Index into the chart's `transitions`.
    pub index: usize,
    pub from: String,
    pub to: String,
    /// The event to send to fire it; absent for completion and guard-only
    /// transitions, which fire on a step with no event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard: Option<String>,
    /// The guard's value now; absent when there is no guard or it could
    /// not be evaluated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard_result: Option<bool>,
    /// For a change trigger, whether its condition holds now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition_result: Option<bool>,
    /// Whether the transition would fire now on its trigger (or, without
    /// one, on a step with no event).
    pub ready: bool,
    /// Why it would not fire, when `ready` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Start the machine: seed variables from their defaults and `overrides`,
/// then enter the initial state of every top-level region.
pub fn start(chart: &StateChart, overrides: &Env) -> ChartSession {
    let mut session = ChartSession {
        current: Vec::new(),
        step: 0,
        env: seed_env(&chart.variables, overrides),
        trace: Vec::new(),
        status: SimStatus::Running,
        messages: Vec::new(),
    };
    let mut active = HashSet::new();
    let mut entered = Vec::new();
    if chart.parallel {
        for region in chart.children(None) {
            if region.kind == ChartStateKind::State {
                enter_state(chart, &region.name, &mut active, &mut entered);
            }
        }
    } else {
        enter_default(chart, None, &mut active, &mut entered);
    }
    let entry_actions = run_entries(chart, &entered, &mut session);
    let initial = chart
        .children(None)
        .find(|s| s.kind == ChartStateKind::Initial)
        .map(|s| s.name.clone())
        .unwrap_or_default();
    session.trace.push(SimStep {
        step: 0,
        from_state: initial,
        transition_name: None,
        trigger: None,
        guard_result: None,
        effect: None,
        to_state: entered.first().cloned().unwrap_or_default(),
        exit_action: None,
        entry_action: entry_actions,
    });
    session.current = leaves(chart, &active);
    session.status = status(chart, &active);
    session
}

/// Rebuild a session from its parts (a session that went through JSON):
/// `current` is normalised to the active leaves and the status recomputed.
pub fn resume(
    chart: &StateChart,
    current: Vec<String>,
    step: usize,
    env: Env,
    trace: Vec<SimStep>,
) -> ChartSession {
    let active = active_set(chart, &current);
    ChartSession {
        current: leaves(chart, &active),
        step,
        env,
        trace,
        status: status(chart, &active),
        messages: Vec::new(),
    }
}

/// Take one step on `event` (`None`: completion, guard-only and change
/// transitions). Returns the new session; if nothing fires, it is the old
/// one with a message saying why.
pub fn fire(chart: &StateChart, session: &ChartSession, event: Option<&str>) -> ChartSession {
    let mut next = session.clone();
    next.messages.clear();
    if session.status == SimStatus::Completed {
        next.messages
            .push("the machine has finished; restart it to run again".to_string());
        return next;
    }

    let mut active = active_set(chart, &session.current);
    let candidates = candidates(chart, &active);
    let mut exited: HashSet<String> = HashSet::new();
    let mut entered_now: HashSet<String> = HashSet::new();
    let mut fired = 0;

    for index in candidates {
        let t = &chart.transitions[index];
        if !active.contains(&t.from) || exited.contains(&t.from) || entered_now.contains(&t.from) {
            continue;
        }
        let check = check(chart, t, &active, &next.env);
        let accepts = match (&t.trigger, event) {
            (None | Some(Trigger::Completion), None) => true,
            (Some(Trigger::Change { .. }), None) => true,
            (Some(Trigger::Signal(s)), Some(e)) => signal_matches(s, e),
            (Some(trigger @ (Trigger::Time(_) | Trigger::Change { .. })), Some(e)) => {
                trigger.event_name() == e
            }
            _ => false,
        };
        if !accepts || !check.ready {
            continue;
        }
        let (left, entered) = take(chart, t, &mut active, &mut next);
        exited.extend(left);
        entered_now.extend(entered);
        fired += 1;
    }

    if fired == 0 {
        let where_ = session.current.join(", ");
        next.messages.push(match event {
            Some(e) => format!("`{e}` fires no transition from {where_}"),
            None => format!("no completion, guard-only or change transition is ready in {where_}"),
        });
    }
    next.current = leaves(chart, &active);
    next.status = status(chart, &active);
    next
}

/// Every transition leaving an active state, inner states first, with its
/// guard evaluated against the session's variables.
pub fn enabled(chart: &StateChart, session: &ChartSession) -> Vec<EnabledTransition> {
    let active = active_set(chart, &session.current);
    candidates(chart, &active)
        .into_iter()
        .map(|index| {
            let t = &chart.transitions[index];
            let check = check(chart, t, &active, &session.env);
            EnabledTransition {
                transition: t.label(),
                index,
                from: t.from.clone(),
                to: t.to.clone(),
                trigger: t.trigger.as_ref().and_then(|tr| match tr {
                    Trigger::Completion => None,
                    other => Some(other.event_name()),
                }),
                guard: t.guard.clone(),
                guard_result: check.guard_result,
                condition_result: check.condition_result,
                ready: check.ready,
                reason: check.reason,
            }
        })
        .collect()
}

struct Check {
    guard_result: Option<bool>,
    condition_result: Option<bool>,
    ready: bool,
    reason: Option<String>,
}

fn check(chart: &StateChart, t: &ChartTransition, active: &HashSet<String>, env: &Env) -> Check {
    let mut reason = None;
    let condition_result = match &t.trigger {
        Some(Trigger::Change { text, expr }) => match change_condition(expr.as_ref(), env) {
            Ok(v) => {
                if !v {
                    reason = Some(format!("condition `{text}` is false"));
                }
                Some(v)
            }
            Err(e) => {
                reason = Some(format!("condition `{text}`: {e}"));
                None
            }
        },
        _ => None,
    };
    let guard_result = match evaluate_guard_parts(t.guard_expr.as_ref(), t.guard.as_deref(), env) {
        Ok(v) => {
            if !v && reason.is_none() {
                reason = Some(format!(
                    "guard `{}` is false",
                    t.guard.as_deref().unwrap_or_default()
                ));
            }
            t.guard.as_ref().map(|_| v)
        }
        Err(e) => {
            if reason.is_none() {
                reason = Some(format!(
                    "guard `{}`: {e}",
                    t.guard.as_deref().unwrap_or_default()
                ));
            }
            None
        }
    };
    // A completion transition out of a composite state waits until every
    // region inside it has reached its final state.
    let triggerless = matches!(t.trigger, None | Some(Trigger::Completion));
    if triggerless && reason.is_none() && chart.is_composite(&t.from) {
        let unfinished = active.iter().any(|s| {
            s != &t.from
                && chart.ancestry(s).contains(&t.from.as_str())
                && !chart.is_composite(s)
                && chart.state(s).map(|st| st.kind) != Some(ChartStateKind::Final)
        });
        if unfinished {
            reason = Some(format!("`{}` has not finished", t.from));
        }
    }
    Check {
        guard_result,
        condition_result,
        ready: reason.is_none(),
        reason,
    }
}

/// Transitions whose source is active: deeper sources first, then in
/// document order.
fn candidates(chart: &StateChart, active: &HashSet<String>) -> Vec<usize> {
    let mut out: Vec<usize> = (0..chart.transitions.len())
        .filter(|&i| {
            let t = &chart.transitions[i];
            active.contains(&t.from)
                && chart.state(&t.from).map(|s| s.kind) != Some(ChartStateKind::Initial)
        })
        .collect();
    out.sort_by_key(|&i| std::cmp::Reverse(chart.depth(&chart.transitions[i].from)));
    out
}

/// Fire `t`: exit, effect, enter. Returns the states left and entered.
fn take(
    chart: &StateChart,
    t: &ChartTransition,
    active: &mut HashSet<String>,
    session: &mut ChartSession,
) -> (Vec<String>, Vec<String>) {
    // The innermost state that contains both ends (and is neither of
    // them): the transition leaves and re-enters everything below it.
    let source_line = chart.ancestry(&t.from);
    let target_line = chart.ancestry(&t.to);
    let domain = source_line
        .iter()
        .skip(1)
        .find(|a| target_line.iter().skip(1).any(|b| b == *a))
        .map(|s| s.to_string());
    let branch_of = |line: &[&str]| -> String {
        let pos = match &domain {
            Some(d) => line.iter().position(|s| s == d).unwrap_or(line.len()),
            None => line.len(),
        };
        line[pos.saturating_sub(1)].to_string()
    };
    let source_branch = branch_of(&source_line);
    let target_branch = branch_of(&target_line);

    // Exit the source branch, innermost first.
    let mut left: Vec<String> = active
        .iter()
        .filter(|s| chart.ancestry(s).contains(&source_branch.as_str()))
        .cloned()
        .collect();
    left.sort_by_key(|s| std::cmp::Reverse(chart.depth(s)));
    let mut exit_texts = Vec::new();
    for s in &left {
        active.remove(s);
        if let Some(action) = chart.state(s).and_then(|st| st.exit.as_ref()) {
            exit_texts.push(action.to_string());
            run(action, session);
        }
    }

    if let Some(effect) = &t.effect {
        run(effect, session);
    }

    // Enter from the target branch down to the target, then its defaults.
    let mut entered = Vec::new();
    let mut path: Vec<&str> = target_line
        .iter()
        .take_while(|s| **s != target_branch)
        .copied()
        .collect();
    path.push(&target_branch);
    path.reverse();
    for (i, s) in path.iter().enumerate() {
        if i + 1 == path.len() {
            enter_state(chart, s, active, &mut entered);
        } else {
            mark_entered(s, active, &mut entered);
            // Entering a parallel state's region enters its sibling regions.
            if chart.state(s).is_some_and(|st| st.parallel) {
                for region in chart.children(Some(s)) {
                    if region.name != path[i + 1] && region.kind == ChartStateKind::State {
                        enter_state(chart, &region.name, active, &mut entered);
                    }
                }
            }
        }
    }
    let entry_text = run_entries(chart, &entered, session);

    session.trace.push(SimStep {
        step: session.step + 1,
        from_state: t.from.clone(),
        transition_name: t.name.clone(),
        trigger: t.trigger.as_ref().map(Trigger::event_name),
        // It fired, so its guard held.
        guard_result: t.guard.as_ref().map(|_| true),
        effect: t.effect.as_ref().map(|a| a.to_string()),
        to_state: t.to.clone(),
        exit_action: (!exit_texts.is_empty()).then(|| exit_texts.join("; ")),
        entry_action: entry_text,
    });
    session.step += 1;
    (left, entered)
}

fn mark_entered(name: &str, active: &mut HashSet<String>, entered: &mut Vec<String>) {
    if active.insert(name.to_string()) {
        entered.push(name.to_string());
    }
}

/// Enter `name` and, if it is composite, its default substates.
fn enter_state(
    chart: &StateChart,
    name: &str,
    active: &mut HashSet<String>,
    entered: &mut Vec<String>,
) {
    mark_entered(name, active, entered);
    let Some(state) = chart.state(name) else {
        return;
    };
    if state.parallel {
        for region in chart.children(Some(name)) {
            if region.kind == ChartStateKind::State {
                enter_state(chart, &region.name, active, entered);
            }
        }
    } else if chart.is_composite(name) {
        enter_default(chart, Some(name), active, entered);
    }
}

/// Enter the default state of the region directly inside `parent`: the
/// target of its initial transition, else its first state.
fn enter_default(
    chart: &StateChart,
    parent: Option<&str>,
    active: &mut HashSet<String>,
    entered: &mut Vec<String>,
) {
    let initial = chart
        .children(parent)
        .find(|s| s.kind == ChartStateKind::Initial)
        .and_then(|init| chart.transitions.iter().find(|t| t.from == init.name))
        .map(|t| t.to.clone());
    let target = initial.or_else(|| {
        chart
            .children(parent)
            .find(|s| s.kind == ChartStateKind::State)
            .map(|s| s.name.clone())
    });
    if let Some(target) = target {
        // The initial transition may point into a nested state.
        let line = chart.ancestry(&target);
        let mut path: Vec<&str> = line
            .iter()
            .take_while(|s| Some(**s) != parent)
            .copied()
            .collect();
        path.reverse();
        for (i, s) in path.iter().enumerate() {
            if i + 1 == path.len() {
                enter_state(chart, s, active, entered);
            } else {
                mark_entered(s, active, entered);
            }
        }
    }
}

/// Run the entry and `do` actions of `entered`, in order. Returns the entry
/// actions as text.
fn run_entries(
    chart: &StateChart,
    entered: &[String],
    session: &mut ChartSession,
) -> Option<String> {
    let mut texts = Vec::new();
    for s in entered {
        let Some(state) = chart.state(s) else {
            continue;
        };
        if let Some(action) = &state.entry {
            texts.push(action.to_string());
            run(action, session);
        }
        if let Some(action) = &state.do_action {
            run(action, session);
        }
    }
    (!texts.is_empty()).then(|| texts.join("; "))
}

fn run(action: &ActionRef, session: &mut ChartSession) {
    let errors = apply_action(action, &mut session.env);
    session.messages.extend(errors);
}

fn active_set(chart: &StateChart, current: &[String]) -> HashSet<String> {
    current
        .iter()
        .flat_map(|s| chart.ancestry(s))
        .map(str::to_string)
        .collect()
}

/// Active states with no active substate, in document order.
fn leaves(chart: &StateChart, active: &HashSet<String>) -> Vec<String> {
    chart
        .states
        .iter()
        .filter(|s| active.contains(&s.name))
        .filter(|s| {
            !chart
                .children(Some(&s.name))
                .any(|c| active.contains(&c.name))
        })
        .map(|s| s.name.clone())
        .collect()
}

fn status(chart: &StateChart, active: &HashSet<String>) -> SimStatus {
    let finished = chart
        .children(None)
        .any(|s| s.kind == ChartStateKind::Final && active.contains(&s.name));
    if finished {
        return SimStatus::Completed;
    }
    let any_exit = chart.transitions.iter().any(|t| {
        active.contains(&t.from)
            && chart.state(&t.from).map(|s| s.kind) != Some(ChartStateKind::Initial)
    });
    if any_exit {
        SimStatus::Running
    } else {
        SimStatus::Deadlocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::expr::Value;
    use crate::sim::state_chart::extract_state_charts;

    fn chart(source: &str) -> StateChart {
        extract_state_charts("t.sysml", source).remove(0)
    }

    const COUNTER: &str = r#"
        state def Counter {
            attribute count : Integer = 0;
            attribute limit : Integer = 2;
            entry; then idle;
            state idle { entry assign count := count + 1; }
            state busy;
            transition start first idle accept Go if count < limit then busy;
            transition finish first busy then idle;
            transition overflow first idle accept when count >= limit then done;
        }
    "#;

    #[test]
    fn start_enters_initial_state_and_runs_entry() {
        let c = chart(COUNTER);
        let s = start(&c, &Env::new());
        assert_eq!(s.current, vec!["idle"]);
        assert_eq!(s.env.get("count"), Some(&Value::Number(1.0)));
        assert_eq!(s.trace.len(), 1);
        assert_eq!(s.trace[0].to_state, "idle");
        assert_eq!(s.status, SimStatus::Running);
    }

    #[test]
    fn signals_guards_completion_and_change_triggers() {
        let c = chart(COUNTER);
        let s = start(&c, &Env::new());
        // Unknown event: nothing happens, with a message.
        let s = fire(&c, &s, Some("Nope"));
        assert_eq!(s.current, vec!["idle"]);
        assert_eq!(s.messages.len(), 1);
        let s = fire(&c, &s, Some("Go"));
        assert_eq!(s.current, vec!["busy"]);
        // Completion transition back; entry increments count to 2.
        let s = fire(&c, &s, None);
        assert_eq!(s.current, vec!["idle"]);
        assert_eq!(s.env.get("count"), Some(&Value::Number(2.0)));
        // The guard now blocks Go, and the change trigger is ready.
        let e = enabled(&c, &s);
        let go = e.iter().find(|t| t.transition == "start").unwrap();
        assert_eq!(go.guard_result, Some(false));
        assert!(!go.ready);
        assert!(go.reason.as_deref().unwrap().contains("count < limit"));
        let overflow = e.iter().find(|t| t.transition == "overflow").unwrap();
        assert_eq!(overflow.condition_result, Some(true));
        assert_eq!(overflow.trigger.as_deref(), Some("when count >= limit"));
        let blocked = fire(&c, &s, Some("Go"));
        assert_eq!(blocked.current, vec!["idle"]);
        let done = fire(&c, &s, Some("when count >= limit"));
        assert_eq!(done.current, vec!["done"]);
        assert_eq!(done.status, SimStatus::Completed);
    }

    #[test]
    fn nested_and_parallel_states() {
        let c = chart(
            r#"state def Drone {
                attribute log : Integer = 0;
                entry; then off;
                state off;
                state on parallel {
                    exit action { assign log := log + 100; }
                    state motors {
                        entry; then idle;
                        state idle;
                        state spinning { entry assign log := log + 1; }
                        transition first idle accept Spin then spinning;
                    }
                    state camera {
                        entry; then standby;
                        state standby;
                        state recording;
                        transition first standby accept Spin then recording;
                    }
                }
                transition first off accept Start then on;
                transition first on accept Stop then off;
            }"#,
        );
        let s = start(&c, &Env::new());
        let s = fire(&c, &s, Some("Start"));
        assert_eq!(s.current, vec!["on.motors.idle", "on.camera.standby"]);
        // One event, both regions step.
        let s = fire(&c, &s, Some("Spin"));
        assert_eq!(s.current, vec!["on.motors.spinning", "on.camera.recording"]);
        assert_eq!(s.env.get("log"), Some(&Value::Number(1.0)));
        // Leaving the parallel state exits every region.
        let s = fire(&c, &s, Some("Stop"));
        assert_eq!(s.current, vec!["off"]);
        assert_eq!(s.trace.last().unwrap().from_state, "on");
        assert_eq!(s.env.get("log"), Some(&Value::Number(101.0)));
    }

    #[test]
    fn unparsable_guard_blocks() {
        let mut c = chart(
            "state def M { entry; then a; state a; state b; transition first a if x > 1 then b; }",
        );
        c.transitions[1].guard_expr = None;
        let s = start(&c, &Env::new());
        let e = enabled(&c, &s);
        assert!(!e[0].ready);
        assert_eq!(fire(&c, &s, None).current, vec!["a"]);
    }
}
