//! Implied relationships (KerML 8.4.2, Tables 8-11; SysML 8.4.1, Tables
//! 31-33) that name resolution and element identity depend on:
//!
//! * default specializations — every definition subclassifies, and every
//!   usage/feature subsets, the library type its metaclass requires
//!   (`PartDefinition` ⊑ `Parts::Part`, `PartUsage` ⊑ `Parts::parts`,
//!   nested composite parts ⊑ `Items::Item::subparts`, ...), filtered by
//!   the redundancy rules of KerML 8.4.2 (an implied specialization is
//!   dropped when an owned or other implied specialization already
//!   specializes its target);
//! * redefinitions — parameters, result parameters and end features
//!   redefine the corresponding features of the owning type's supertypes;
//!   state entry/do/exit actions, objectives and view renderings redefine
//!   their library features.
//!
//! Implied relationships are added after the explicit ones in
//! `ownedRelationship` (as in the reference implementation's
//! `.xmi.implied` output), so explicit elements keep their paths.
//!
//! Not produced (they do not affect name resolution): `TypeFeaturing`s,
//! the `BindingConnector`s of feature values and expressions, and the
//! implied relationships inside expressions (operator typing, argument
//! redefinitions).

use crate::graph::{Direction, ElementRef, Flags, Role, Value};
use crate::metaclass::Metaclass as M;
use crate::metaclass::Metaclass;
use crate::resolve::{Resolver, TableKey};

impl Resolver<'_> {
    /// Compute and add the implied relationships of `e` (once): first the
    /// implied redefinitions (phase A, which naming and redefinition
    /// hiding need), then the implied specializations (phase B).
    ///
    /// A phase computed while a cycle was cut is not materialized; it is
    /// retried later (see `finish`), so implied relationships never depend
    /// on incomplete information.
    pub fn ensure_implied(&mut self, e: ElementRef) {
        self.ensure_implied_redefinitions(e);
        if self.implied_state(e) != 2 {
            return;
        }
        self.set_implied_state(e, 3);
        let cuts = self.cuts;
        let specializations = self.implied_specializations(e);
        if self.cuts != cuts {
            self.set_implied_state(e, 2);
            return;
        }
        for (kind, t) in specializations {
            self.add_implied(e, kind, t);
        }
        self.set_implied_state(e, 4);
    }

    /// Phase A of [`Self::ensure_implied`].
    pub fn ensure_implied_redefinitions(&mut self, e: ElementRef) {
        if self.is_lib(e) {
            return;
        }
        match self.implied_state(e) {
            0 => {}
            1 | 3 => {
                self.cuts += 1;
                return;
            }
            _ => return,
        }
        if !self.wants_implied(e) {
            self.set_implied_state(e, 4);
            return;
        }
        self.set_implied_state(e, 1);
        let cuts = self.cuts;
        let redefinitions = self.implied_redefinitions(e);
        if self.cuts != cuts {
            self.set_implied_state(e, 0);
            return;
        }
        for t in redefinitions {
            self.add_implied(e, M::Redefinition, t);
        }
        self.set_implied_state(e, 2);
    }

    /// Elements that get implied relationships: types that are not part
    /// of an expression tree and not feature chains.
    fn wants_implied(&self, e: ElementRef) -> bool {
        let kind = self.kind(e);
        if !kind.is_type() || kind.is_expression_node() {
            return false;
        }
        // Feature chains (owned by a relationship).
        if let Some(rel) = self.owner(e) {
            if !self.kind(rel).is_membership() {
                return false;
            }
        } else {
            return false;
        }
        // Anything inside an expression tree.
        let mut cur = e;
        while let Some(rel) = self.owner(cur) {
            let Some(o) = self.owner(rel) else { break };
            if self.kind(o).is_expression_node() {
                return false;
            }
            cur = o;
        }
        true
    }

    // ------------------------------------------------------------------
    // Redefinitions

    fn implied_redefinitions(&mut self, e: ElementRef) -> Vec<ElementRef> {
        let mut out = Vec::new();
        let kind = self.kind(e);
        if !kind.is_feature() {
            return out;
        }
        let Some(membership) = self.owner(e) else {
            return out;
        };
        let mk = self.kind(membership);
        let explicit: Vec<ElementRef> = {
            let rels = self.owned_rels(e, |k| k == M::Redefinition);
            let mut v = Vec::new();
            for r in rels {
                if let Some(g) = self.general(r) {
                    v.push(g);
                }
            }
            v
        };
        let owning_type = self.owning_type(e);

        // SysML state subactions (checkActionUsageStateActionRedefinition).
        if mk == M::StateSubactionMembership {
            let target = match self.el(membership).value {
                Value::Kind("entry") => "States::StateAction::entryAction",
                Value::Kind("do") => "States::StateAction::doAction",
                _ => "States::StateAction::exitAction",
            };
            if let Some(t) = self.library_element(target) {
                if !explicit.contains(&t) {
                    out.push(t);
                }
            }
            return out;
        }
        // View renderings (checkRenderingUsageRedefinition).
        if mk == M::ViewRenderingMembership {
            if let Some(t) = self.library_element("Views::View::viewRendering") {
                if !explicit.contains(&t) {
                    out.push(t);
                }
            }
            return out;
        }
        let Some(owning_type) = owning_type else {
            return out;
        };
        let generals: Vec<ElementRef> = self.supertypes(owning_type).to_vec();

        // Objectives (checkRequirementUsageObjectiveRedefinition).
        if mk == M::ObjectiveMembership {
            for g in &generals {
                if matches!(
                    self.kind(*g),
                    M::CaseDefinition
                        | M::AnalysisCaseDefinition
                        | M::VerificationCaseDefinition
                        | M::UseCaseDefinition
                        | M::CaseUsage
                        | M::AnalysisCaseUsage
                        | M::VerificationCaseUsage
                        | M::UseCaseUsage
                ) {
                    if let Some(obj) = self.feature_of_membership(*g, M::ObjectiveMembership) {
                        if !explicit.contains(&obj) && !out.contains(&obj) {
                            out.push(obj);
                        }
                    }
                }
            }
            return out;
        }

        // Result parameters (checkFeatureResultRedefinition).
        if mk == M::ReturnParameterMembership {
            let tk = self.kind(owning_type);
            if tk.is_function() || tk.is_expression() {
                for g in &generals {
                    let gk = self.kind(*g);
                    if gk.is_function() || gk.is_expression() {
                        if let Some(r) = self.result_parameter(*g) {
                            if r != e && !explicit.contains(&r) && !out.contains(&r) {
                                out.push(r);
                            }
                        }
                    }
                }
            }
            return out;
        }

        // End features (checkFeatureEndRedefinition).
        if self.el(e).flags.contains(Flags::END) && mk.is_feature_membership() {
            let ends = self.owned_end_features(owning_type);
            if let Some(i) = ends.iter().position(|x| *x == e) {
                for g in &generals {
                    let gends = self.end_features(*g);
                    if let Some(t) = gends.get(i) {
                        if *t != e && !explicit.contains(t) && !out.contains(t) {
                            out.push(*t);
                        }
                    }
                }
            }
            return out;
        }

        // Parameters (checkFeatureParameterRedefinition).
        let tk = self.kind(owning_type);
        if self.el(e).direction.is_some()
            && mk.is_feature_membership()
            && (tk.is_behavior() || tk.is_step())
            && !(tk == M::InvocationExpression && !explicit.is_empty())
        {
            let params = self.owned_parameters(owning_type);
            if let Some(i) = params.iter().position(|x| *x == e) {
                for g in &generals {
                    let gk = self.kind(*g);
                    if !(gk.is_behavior() || gk.is_step()) {
                        continue;
                    }
                    let gparams = self.parameters(*g);
                    if let Some(t) = gparams.get(i) {
                        if *t != e && !explicit.contains(t) && !out.contains(t) {
                            out.push(*t);
                        }
                    }
                }
            }
        }
        out
    }

    /// The member feature of `t`'s first membership of kind `mk` (own or
    /// inherited).
    fn feature_of_membership(&mut self, t: ElementRef, mk: Metaclass) -> Option<ElementRef> {
        let table = self.table(TableKey::All(t))?;
        table
            .entries
            .iter()
            .find(|e| self.kind(e.membership) == mk)
            .map(|e| e.element)
    }

    /// Owned features of `t` with a direction, excluding the result.
    pub fn owned_parameters(&mut self, t: ElementRef) -> Vec<ElementRef> {
        let owned = self.el(t).owned.clone();
        let mut out = Vec::new();
        for m in owned {
            let mk = self.kind(m);
            if !mk.is_feature_membership() || mk == M::ReturnParameterMembership {
                continue;
            }
            if let Some(f) = self.owned_member(m) {
                if self.el(f).direction.is_some() {
                    out.push(f);
                }
            }
        }
        out
    }

    /// The parameters of `t` (owned and inherited directed features,
    /// excluding the result parameter), in order.
    pub fn parameters(&mut self, t: ElementRef) -> Vec<ElementRef> {
        let Some(table) = self.table(TableKey::All(t)) else {
            return Vec::new();
        };
        let mut owned = Vec::new();
        let mut inherited = Vec::new();
        for entry in table.entries.iter() {
            let mk = self.kind(entry.membership);
            if !mk.is_feature_membership() || mk == M::ReturnParameterMembership {
                continue;
            }
            if self.direction_of(entry.element).is_none() {
                continue;
            }
            match entry.origin {
                crate::resolve::Origin::Owned => owned.push(entry.element),
                crate::resolve::Origin::Inherited => inherited.push(entry.element),
                _ => {}
            }
        }
        owned.extend(inherited);
        owned
    }

    /// The (declared or inherited through redefinition) direction of a feature.
    pub fn direction_of(&mut self, f: ElementRef) -> Option<Direction> {
        if let Some(d) = self.el(f).direction {
            return Some(d);
        }
        None
    }

    /// Owned end features of `t`.
    pub fn owned_end_features(&mut self, t: ElementRef) -> Vec<ElementRef> {
        let owned = self.el(t).owned.clone();
        let mut out = Vec::new();
        for m in owned {
            if !self.kind(m).is_feature_membership() {
                continue;
            }
            if let Some(f) = self.owned_member(m) {
                if self.el(f).flags.contains(Flags::END) {
                    out.push(f);
                }
            }
        }
        out
    }

    /// End features of `t` (owned and inherited).
    pub fn end_features(&mut self, t: ElementRef) -> Vec<ElementRef> {
        let Some(table) = self.table(TableKey::All(t)) else {
            return Vec::new();
        };
        let mut owned = Vec::new();
        let mut inherited = Vec::new();
        for entry in table.entries.iter() {
            if !self.kind(entry.membership).is_feature_membership() {
                continue;
            }
            if !self.el(entry.element).flags.contains(Flags::END) {
                continue;
            }
            match entry.origin {
                crate::resolve::Origin::Owned => owned.push(entry.element),
                crate::resolve::Origin::Inherited => inherited.push(entry.element),
                _ => {}
            }
        }
        owned.extend(inherited);
        owned
    }

    // ------------------------------------------------------------------
    // Specializations

    fn implied_specializations(&mut self, e: ElementRef) -> Vec<(Metaclass, ElementRef)> {
        let kind = self.kind(e);
        let names: Vec<&'static str> = if kind.is_classifier() {
            self.classifier_candidates(e)
        } else if kind.is_feature() {
            self.feature_candidates(e)
        } else {
            Vec::new()
        };
        let rel_kind = if kind.is_classifier() {
            M::Subclassification
        } else if kind.is_feature() {
            M::Subsetting
        } else {
            M::Specialization
        };
        // Resolve candidate targets.
        let mut candidates: Vec<ElementRef> = Vec::new();
        for n in names {
            if let Some(t) = self.library_element(n) {
                if t != e && !candidates.contains(&t) {
                    candidates.push(t);
                }
            }
        }
        if candidates.is_empty() {
            return Vec::new();
        }
        // Explicit generals.
        let explicit: Vec<ElementRef> = {
            let rels = self.owned_rels(e, |k| k.is_specialization());
            let mut v = Vec::new();
            for r in rels {
                if self.el(r).is_implied() {
                    continue;
                }
                if let Some(g) = self.general(r) {
                    v.push(g);
                }
            }
            v
        };
        // Implied redefinitions already added count as specializations.
        let implied_redefs: Vec<ElementRef> = {
            let rels = self.owned_rels(e, |k| k == M::Redefinition);
            let mut v = Vec::new();
            for r in rels {
                if !self.el(r).is_implied() {
                    continue;
                }
                if let Some(g) = self.general(r) {
                    v.push(g);
                }
            }
            v
        };
        let mut keep = Vec::new();
        'candidates: for (i, c) in candidates.iter().enumerate() {
            // Rule 1: an owned specialization with the same general, or any
            // owned/implied specialization whose general is a strict
            // subtype of the candidate.
            for g in explicit.iter().chain(implied_redefs.iter()) {
                if *g == *c || self.specializes(*g, *c) {
                    continue 'candidates;
                }
            }
            for (j, other) in candidates.iter().enumerate() {
                if i != j && *other != *c && self.specializes(*other, *c) {
                    continue 'candidates;
                }
            }
            keep.push((rel_kind, *c));
        }
        keep
    }

    fn is_binary(&mut self, t: ElementRef) -> bool {
        self.owned_end_features(t).len() == 2
    }

    /// Library types a classifier of this metaclass must specialize, most
    /// specific first.
    fn classifier_candidates(&mut self, e: ElementRef) -> Vec<&'static str> {
        let kind = self.kind(e);
        let binary = || false;
        let _ = binary;
        let mut v: Vec<&'static str> = match kind {
            M::AttributeDefinition | M::EnumerationDefinition | M::DataType => {
                vec!["Base::DataValue"]
            }
            M::OccurrenceDefinition => {
                if self.el(e).flags.contains(Flags::INDIVIDUAL) {
                    vec!["Occurrences::Life", "Occurrences::Occurrence"]
                } else {
                    vec!["Occurrences::Occurrence"]
                }
            }
            M::ItemDefinition => vec!["Items::Item"],
            M::PartDefinition => vec!["Parts::Part"],
            M::PortDefinition => vec!["Ports::Port"],
            M::ConjugatedPortDefinition => vec![],
            M::ConnectionDefinition => {
                if self.is_binary(e) {
                    vec!["Connections::BinaryConnection", "Connections::Connection"]
                } else {
                    vec!["Connections::Connection"]
                }
            }
            M::InterfaceDefinition => {
                if self.is_binary(e) {
                    vec!["Interfaces::BinaryInterface", "Interfaces::Interface"]
                } else {
                    vec!["Interfaces::Interface"]
                }
            }
            M::AllocationDefinition => vec!["Allocations::Allocation"],
            M::FlowDefinition => {
                if self.is_binary(e) {
                    vec!["Flows::Message", "Flows::MessageAction"]
                } else {
                    vec!["Flows::MessageAction"]
                }
            }
            M::ActionDefinition => vec!["Actions::Action"],
            M::StateDefinition => vec!["States::StateAction"],
            M::CalculationDefinition => vec!["Calculations::Calculation"],
            M::ConstraintDefinition => vec!["Constraints::ConstraintCheck"],
            M::RequirementDefinition => vec!["Requirements::RequirementCheck"],
            M::ConcernDefinition => vec!["Requirements::ConcernCheck"],
            M::CaseDefinition => vec!["Cases::Case"],
            M::AnalysisCaseDefinition => vec!["AnalysisCases::AnalysisCase"],
            M::VerificationCaseDefinition => vec!["VerificationCases::VerificationCase"],
            M::UseCaseDefinition => vec!["UseCases::UseCase"],
            M::ViewDefinition => vec!["Views::View"],
            M::ViewpointDefinition => vec!["Views::ViewpointCheck"],
            M::RenderingDefinition => vec!["Views::Rendering"],
            M::MetadataDefinition => vec!["Metadata::MetadataItem"],
            M::Class => vec!["Occurrences::Occurrence"],
            M::Structure => vec!["Objects::Object"],
            M::Association => {
                if self.is_binary(e) {
                    vec!["Links::BinaryLink", "Links::Link"]
                } else {
                    vec!["Links::Link"]
                }
            }
            M::AssociationStructure => {
                if self.is_binary(e) {
                    vec!["Objects::BinaryLinkObject", "Objects::LinkObject"]
                } else {
                    vec!["Objects::LinkObject"]
                }
            }
            M::Behavior => vec!["Performances::Performance"],
            M::Function => vec!["Performances::Evaluation"],
            M::Predicate => vec!["Performances::BooleanEvaluation"],
            M::Interaction => {
                if self.is_binary(e) {
                    vec!["Performances::Performance", "Links::BinaryLink"]
                } else {
                    vec!["Performances::Performance", "Links::Link"]
                }
            }
            M::Metaclass => vec!["Metaobjects::Metaobject"],
            _ => vec![],
        };
        if kind != M::ConjugatedPortDefinition {
            v.push("Base::Anything");
        }
        v
    }

    /// Whether a usage/feature is composite.
    pub fn is_composite(&self, e: ElementRef) -> bool {
        let el = self.el(e);
        let kind = el.kind;
        if kind.is_usage() {
            !el.flags.contains(Flags::REFERENCE)
                && !matches!(
                    kind,
                    M::ReferenceUsage
                        | M::AttributeUsage
                        | M::EnumerationUsage
                        | M::MetadataUsage
                        | M::BindingConnectorAsUsage
                        | M::SuccessionAsUsage
                )
                && el.direction.is_none()
                && !el.flags.contains(Flags::END)
                && self
                    .owner(e)
                    .map(|m| self.kind(m).is_feature_membership())
                    .unwrap_or(false)
        } else {
            el.flags.contains(Flags::COMPOSITE)
        }
    }

    /// Library features a feature of this metaclass must subset, most
    /// specific first.
    fn feature_candidates(&mut self, e: ElementRef) -> Vec<&'static str> {
        let kind = self.kind(e);
        let el_flags = self.el(e).flags;
        let membership_kind = self.owner(e).map(|m| self.kind(m));
        let owning_type = self.owning_type(e);
        let ot = owning_type.map(|t| self.kind(t));
        let composite = self.is_composite(e);
        let owner_is = |pred: fn(Metaclass) -> bool| ot.map(pred).unwrap_or(false);
        let mut v: Vec<&'static str> = Vec::new();

        // SysML usages.
        let item_owner = |k: Metaclass| {
            matches!(
                k,
                M::ItemDefinition
                    | M::PartDefinition
                    | M::ConnectionDefinition
                    | M::InterfaceDefinition
                    | M::AllocationDefinition
                    | M::RenderingDefinition
                    | M::ViewDefinition
                    | M::MetadataDefinition
                    | M::PortDefinition
            ) || k.is_item_usage()
        };
        let part_owner = |k: Metaclass| {
            matches!(
                k,
                M::PartDefinition
                    | M::ConnectionDefinition
                    | M::InterfaceDefinition
                    | M::AllocationDefinition
                    | M::RenderingDefinition
                    | M::ViewDefinition
            ) || k.is_part_usage()
        };
        let action_owner = |k: Metaclass| {
            matches!(
                k,
                M::ActionDefinition
                    | M::StateDefinition
                    | M::CalculationDefinition
                    | M::ConstraintDefinition
                    | M::RequirementDefinition
                    | M::ConcernDefinition
                    | M::CaseDefinition
                    | M::AnalysisCaseDefinition
                    | M::VerificationCaseDefinition
                    | M::UseCaseDefinition
                    | M::ViewpointDefinition
                    | M::FlowDefinition
            ) || k.is_action_usage()
                || matches!(k, M::FlowUsage | M::SuccessionFlowUsage)
        };
        let is_subaction = composite
            && owner_is(action_owner)
            && membership_kind != Some(M::StateSubactionMembership)
            && membership_kind != Some(M::TransitionFeatureMembership);
        match kind {
            M::AttributeUsage | M::EnumerationUsage => v.push("Base::dataValues"),
            M::OccurrenceUsage | M::EventOccurrenceUsage => {
                if kind == M::EventOccurrenceUsage
                    && owner_is(|k| k.is_occurrence_definition() || k.is_occurrence_usage())
                {
                    v.push("Occurrences::Occurrence::timeEnclosedOccurrences");
                }
            }
            M::ItemUsage => {
                if composite && owner_is(item_owner) {
                    v.push("Items::Item::subitems");
                }
                v.push("Items::items");
            }
            M::PartUsage => {
                match membership_kind {
                    Some(M::ActorMembership) => {
                        if owner_is(|k| matches!(k, M::RequirementDefinition | M::RequirementUsage))
                        {
                            v.push("Requirements::RequirementCheck::actors");
                        } else {
                            v.push("Cases::Case::actors");
                        }
                    }
                    Some(M::StakeholderMembership) => {
                        v.push("Requirements::RequirementCheck::stakeholders")
                    }
                    _ => {}
                }
                if composite && owner_is(item_owner) {
                    v.push("Items::Item::subparts");
                }
                v.push("Parts::parts");
            }
            M::PortUsage => {
                if composite
                    && owner_is(|k| {
                        matches!(
                            k,
                            M::PortDefinition | M::ConjugatedPortDefinition | M::PortUsage
                        )
                    })
                {
                    v.push("Ports::Port::subports");
                }
                if owner_is(part_owner) {
                    v.push("Parts::Part::ownedPorts");
                }
                v.push("Ports::ports");
            }
            M::ConnectionUsage => {
                if self.is_binary(e) {
                    v.push("Connections::binaryConnections");
                }
                v.push("Connections::connections");
                if composite && owner_is(item_owner) {
                    v.push("Items::Item::subparts");
                }
            }
            M::InterfaceUsage => {
                if self.is_binary(e) {
                    v.push("Interfaces::binaryInterfaces");
                }
                v.push("Interfaces::interfaces");
                if composite && owner_is(item_owner) {
                    v.push("Items::Item::subparts");
                }
            }
            M::AllocationUsage => {
                v.push("Allocations::allocations");
                if composite && owner_is(item_owner) {
                    v.push("Items::Item::subparts");
                }
            }
            M::FlowUsage => {
                if !self.owned_end_features(e).is_empty() {
                    v.push("Flows::flows");
                }
                v.push("Flows::messages");
            }
            M::SuccessionFlowUsage => v.push("Flows::successionFlows"),
            M::AcceptActionUsage => {
                if membership_kind == Some(M::TransitionFeatureMembership) {
                    v.push("Actions::TransitionAction::accepter");
                } else {
                    if is_subaction {
                        v.push("Actions::Action::acceptSubactions");
                    }
                    v.push("Actions::acceptActions");
                }
            }
            M::SendActionUsage => {
                if is_subaction {
                    v.push("Actions::Action::sendSubactions");
                }
                v.push("Actions::sendActions");
            }
            M::AssignmentActionUsage => {
                if is_subaction {
                    v.push("Actions::Action::assignments");
                }
                v.push("Actions::assignmentActions");
            }
            M::TerminateActionUsage => {
                if is_subaction {
                    v.push("Actions::Action::terminateSubactions");
                }
                v.push("Actions::terminateActions");
            }
            M::IfActionUsage => {
                if is_subaction {
                    v.push("Actions::Action::ifSubactions");
                }
                let params = self
                    .el(e)
                    .owned
                    .iter()
                    .filter(|m| self.kind(**m) == M::ParameterMembership)
                    .count();
                if params >= 3 {
                    v.push("Actions::ifThenElseActions");
                }
                v.push("Actions::ifThenActions");
            }
            M::WhileLoopActionUsage => {
                if is_subaction {
                    v.push("Actions::Action::whileLoops");
                }
                v.push("Actions::whileLoopActions");
            }
            M::ForLoopActionUsage => {
                if is_subaction {
                    v.push("Actions::Action::forLoops");
                }
                v.push("Actions::forLoopActions");
            }
            M::MergeNode => v.extend(["Actions::Action::merges", "Actions::Action::controls"]),
            M::DecisionNode => {
                v.extend(["Actions::Action::decisions", "Actions::Action::controls"])
            }
            M::JoinNode => v.extend(["Actions::Action::joins", "Actions::Action::controls"]),
            M::ForkNode => v.extend(["Actions::Action::forks", "Actions::Action::controls"]),
            M::PerformActionUsage => {
                if owner_is(part_owner) {
                    v.push("Parts::Part::performedActions");
                }
            }
            M::ExhibitStateUsage => {
                if owner_is(part_owner) {
                    v.push("Parts::Part::exhibitedStates");
                }
            }
            M::StateUsage => {
                if composite
                    && owner_is(|k| {
                        matches!(k, M::StateDefinition | M::StateUsage | M::ExhibitStateUsage)
                    })
                {
                    let parallel = owning_type
                        .map(|t| self.el(t).flags.contains(Flags::PARALLEL))
                        .unwrap_or(false);
                    if parallel {
                        v.push("States::StateAction::substates");
                    } else {
                        v.push("States::StateAction::exclusiveStates");
                    }
                }
                if composite && owner_is(part_owner) {
                    v.push("Parts::Part::ownedStates");
                }
                v.push("States::stateActions");
            }
            M::TransitionUsage => {
                if composite
                    && owner_is(|k| {
                        matches!(k, M::StateDefinition | M::StateUsage | M::ExhibitStateUsage)
                    })
                {
                    v.push("States::StateAction::stateTransitions");
                } else if composite && owner_is(action_owner) {
                    v.push("Actions::Action::decisionTransitions");
                }
                v.push("Actions::transitionActions");
            }
            M::CalculationUsage => {
                if owner_is(|k| matches!(k, M::CalculationDefinition | M::CalculationUsage)) {
                    v.push("Calculations::Calculation::subcalculations");
                }
                v.push("Calculations::calculations");
            }
            M::CaseUsage => {
                if composite && owner_is(|k| matches!(k, M::CaseDefinition | M::CaseUsage)) {
                    v.push("Cases::Case::subcases");
                }
                v.push("Cases::cases");
            }
            M::AnalysisCaseUsage => {
                if composite
                    && owner_is(|k| matches!(k, M::AnalysisCaseDefinition | M::AnalysisCaseUsage))
                {
                    v.push("AnalysisCases::AnalysisCase::subAnalysisCases");
                }
                v.push("AnalysisCases::analysisCases");
            }
            M::VerificationCaseUsage => {
                if composite
                    && owner_is(|k| {
                        matches!(k, M::VerificationCaseDefinition | M::VerificationCaseUsage)
                    })
                {
                    v.push("VerificationCases::VerificationCase::subVerificationCases");
                }
                v.push("VerificationCases::verificationCases");
            }
            M::UseCaseUsage => {
                if composite && owner_is(|k| matches!(k, M::UseCaseDefinition | M::UseCaseUsage)) {
                    v.push("UseCases::UseCase::subUseCases");
                }
                v.push("UseCases::useCases");
            }
            M::IncludeUseCaseUsage => {
                if owner_is(|k| matches!(k, M::UseCaseDefinition | M::UseCaseUsage)) {
                    v.push("UseCases::UseCase::includedUseCases");
                }
                v.push("UseCases::useCases");
            }
            M::AssertConstraintUsage => {
                if el_flags.contains(Flags::NEGATED) {
                    v.push("Constraints::negatedConstraintChecks");
                } else {
                    v.push("Constraints::assertedConstraintChecks");
                }
            }
            M::SatisfyRequirementUsage => {
                if el_flags.contains(Flags::NEGATED) {
                    v.push("Requirements::notSatisfiedRequirementChecks");
                } else {
                    v.push("Requirements::satisfiedRequirementChecks");
                }
            }
            M::RequirementUsage => {
                if membership_kind == Some(M::RequirementVerificationMembership) {
                    v.push("VerificationCases::VerificationCase::obj::requirementVerifications");
                }
                if composite
                    && owner_is(|k| matches!(k, M::RequirementDefinition | M::RequirementUsage))
                {
                    v.push("Requirements::RequirementCheck::subrequirements");
                }
                v.push("Requirements::requirementChecks");
            }
            M::ConcernUsage => {
                if membership_kind == Some(M::FramedConcernMembership) {
                    v.push("Requirements::RequirementCheck::concerns");
                }
                v.push("Requirements::concernChecks");
            }
            M::ViewpointUsage => {
                if composite && owner_is(|k| matches!(k, M::ViewDefinition | M::ViewUsage)) {
                    v.push("Views::View::viewpointSatisfactions");
                }
                v.push("Views::viewpoints");
            }
            M::ViewUsage => {
                if owner_is(|k| matches!(k, M::ViewDefinition | M::ViewUsage)) {
                    v.push("Views::View::subviews");
                }
                v.push("Views::views");
            }
            M::RenderingUsage => {
                if owner_is(|k| matches!(k, M::RenderingDefinition | M::RenderingUsage)) {
                    v.push("Views::Rendering::subrenderings");
                }
                v.push("Views::renderings");
            }
            M::MetadataUsage => v.push("Metadata::metadataItems"),
            M::BindingConnectorAsUsage => v.push("Links::selfLinks"),
            M::SuccessionAsUsage => v.push("Occurrences::happensBeforeLinks"),
            _ => {}
        }
        // ConstraintUsage family.
        if kind.is_constraint_usage() {
            if membership_kind == Some(M::RequirementConstraintMembership) {
                let assumption = self
                    .owner(e)
                    .map(|m| self.el(m).value == Value::Kind("assumption"))
                    .unwrap_or(false);
                if assumption {
                    v.push("Requirements::RequirementCheck::assumptions");
                } else {
                    v.push("Requirements::RequirementCheck::constraints");
                }
            }
            if owner_is(item_owner) {
                v.push("Items::Item::checkedConstraints");
            }
            v.push("Constraints::constraintChecks");
        }
        // ActionUsage family (generic action rules).
        if kind.is_action_usage() {
            if composite && owner_is(part_owner) {
                v.push("Parts::Part::ownedActions");
            }
            if is_subaction {
                v.push("Actions::Action::subactions");
            }
            v.push("Actions::actions");
        }
        // OccurrenceUsage family.
        if kind.is_occurrence_usage() {
            match self.el(e).value {
                Value::Portion(crate::graph::PortionKind::Snapshot) => {
                    v.push("Occurrences::Occurrence::snapshots")
                }
                Value::Portion(crate::graph::PortionKind::Timeslice) => {
                    v.push("Occurrences::Occurrence::timeSlices")
                }
                _ => {}
            }
            if composite
                && owner_is(|k| {
                    k.is_occurrence_definition()
                        || k.is_occurrence_usage()
                        || matches!(k, M::Class | M::Structure | M::Behavior | M::Function)
                })
            {
                v.push("Occurrences::Occurrence::suboccurrences");
            }
            v.push("Occurrences::occurrences");
        }

        // KerML features.
        match kind {
            M::Step => {
                if owner_is(|k| k.is_behavior() || k.is_step()) {
                    if self.el(e).flags.contains(Flags::COMPOSITE) {
                        v.push("Performances::Performance::subperformance");
                    }
                    v.push("Performances::Performance::enclosedPerformance");
                }
                v.push("Performances::performances");
            }
            M::Expression => {
                if owner_is(|k| k.is_behavior() || k.is_step()) {
                    v.push("Performances::Performance::enclosedPerformance");
                }
                v.push("Performances::evaluations");
            }
            M::BooleanExpression => v.push("Performances::booleanEvaluations"),
            M::Invariant => {
                if el_flags.contains(Flags::NEGATED) {
                    v.push("Performances::falseEvaluations");
                } else {
                    v.push("Performances::trueEvaluations");
                }
            }
            M::Connector => {
                if self.is_binary(e) {
                    v.push("Links::binaryLinks");
                }
                v.push("Links::links");
            }
            M::BindingConnector => v.push("Links::selfLinks"),
            M::Succession => v.push("Occurrences::happensBeforeLinks"),
            M::Flow => {
                if !self.owned_end_features(e).is_empty() {
                    v.push("Transfers::flowTransfers");
                }
                v.push("Transfers::transfers");
            }
            M::SuccessionFlow => v.push("Transfers::flowTransfersBefore"),
            M::MetadataFeature => v.push("Metaobjects::metaobjects"),
            M::Multiplicity | M::MultiplicityRange => v.push("Base::naturals"),
            _ => {}
        }
        if kind.is_step() && kind != M::Step && !kind.is_usage() {
            v.push("Performances::performances");
        }
        // Typed-by rules (checkFeatureDataValue/Occurrence/ObjectSpecialization).
        let types = self.owned_typing_types(e);
        if !matches!(kind, M::Multiplicity | M::MultiplicityRange) {
            let mut data = false;
            let mut class = false;
            let mut structure = false;
            for t in &types {
                let tk = self.kind(*t);
                if matches!(
                    tk,
                    M::DataType | M::AttributeDefinition | M::EnumerationDefinition
                ) {
                    data = true;
                }
                if is_structure(tk) {
                    structure = true;
                }
                if is_class(tk) {
                    class = true;
                }
            }
            if structure {
                if composite && owner_is(|k| is_structure(k) || k.is_feature()) {
                    v.push("Objects::Object::subobjects");
                }
                v.push("Objects::objects");
            }
            if class {
                if composite && owner_is(|k| is_class(k) || k.is_feature()) {
                    v.push("Occurrences::Occurrence::suboccurrences");
                }
                v.push("Occurrences::occurrences");
            }
            if data {
                v.push("Base::dataValues");
            }
        }
        // End features of associations and connectors.
        if el_flags.contains(Flags::END) && owner_is(|k| k.is_association() || k.is_connector()) {
            v.push("Links::Link::participant");
        }
        v.push("Base::things");
        v
    }

    /// Types of `e`'s owned FeatureTypings.
    pub fn owned_typing_types(&mut self, e: ElementRef) -> Vec<ElementRef> {
        let rels = self.owned_rels(e, |k| k.is_feature_typing());
        let mut out = Vec::new();
        for r in rels {
            if let Some(t) = self.general(r) {
                out.push(t);
            }
        }
        out
    }

    /// The end target of `rel` with role `role` (re-export for implied rules).
    #[allow(dead_code)]
    pub(crate) fn target_of(&mut self, rel: ElementRef) -> Option<ElementRef> {
        self.end_target(rel, Role::Target)
    }
}

/// Metaclasses that are kinds of KerML `Structure`.
fn is_structure(k: Metaclass) -> bool {
    matches!(
        k,
        M::Structure
            | M::AssociationStructure
            | M::ItemDefinition
            | M::PartDefinition
            | M::PortDefinition
            | M::ConjugatedPortDefinition
            | M::ConnectionDefinition
            | M::InterfaceDefinition
            | M::AllocationDefinition
            | M::RenderingDefinition
            | M::ViewDefinition
            | M::MetadataDefinition
            | M::Metaclass
    )
}

/// Metaclasses that are kinds of KerML `Class`.
fn is_class(k: Metaclass) -> bool {
    is_structure(k) || k.is_behavior() || k.is_occurrence_definition() || k == M::Class
}
