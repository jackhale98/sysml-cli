//! The KerML and SysML metaclasses the textual notation can produce
//! (KerML 1.0 clause 8.3, SysML 2.0 clause 8.3), with the generalization
//! facts the semantic model needs.

use std::fmt;

macro_rules! metaclasses {
    ($($name:ident),* $(,)?) => {
        /// A concrete (or, for implied/abstract placeholders, abstract)
        /// metaclass of the KerML / SysML abstract syntax.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Metaclass {
            $($name),*
        }

        impl Metaclass {
            /// Every metaclass, in declaration order.
            pub const ALL: &'static [Metaclass] = &[$(Metaclass::$name),*];

            /// The metaclass name as used in the abstract syntax and in XMI
            /// (`xsi:type="sysml:<name>"`).
            pub fn name(self) -> &'static str {
                match self {
                    $(Metaclass::$name => stringify!($name)),*
                }
            }

            /// Inverse of [`Metaclass::name`].
            pub fn from_name(name: &str) -> Option<Metaclass> {
                match name {
                    $(stringify!($name) => Some(Metaclass::$name),)*
                    _ => None,
                }
            }
        }
    };
}

metaclasses! {
    // ---- Root (KerML 8.3.2)
    Namespace,
    Package,
    LibraryPackage,
    Comment,
    Documentation,
    TextualRepresentation,
    Membership,
    OwningMembership,
    MembershipImport,
    NamespaceImport,
    Dependency,
    Annotation,
    ElementFilterMembership,
    // ---- Core (KerML 8.3.3)
    Type,
    Classifier,
    Feature,
    Specialization,
    Subclassification,
    FeatureTyping,
    Subsetting,
    ReferenceSubsetting,
    CrossSubsetting,
    Redefinition,
    Conjugation,
    Disjoining,
    Unioning,
    Intersecting,
    Differencing,
    FeatureChaining,
    FeatureInverting,
    TypeFeaturing,
    FeatureMembership,
    EndFeatureMembership,
    Multiplicity,
    MultiplicityRange,
    // ---- Kernel (KerML 8.3.4)
    DataType,
    Class,
    Structure,
    Association,
    AssociationStructure,
    Connector,
    BindingConnector,
    Succession,
    Behavior,
    Step,
    Function,
    Predicate,
    Expression,
    BooleanExpression,
    Invariant,
    ParameterMembership,
    ReturnParameterMembership,
    ResultExpressionMembership,
    FeatureValue,
    NullExpression,
    LiteralBoolean,
    LiteralString,
    LiteralInteger,
    LiteralRational,
    LiteralInfinity,
    FeatureReferenceExpression,
    InvocationExpression,
    OperatorExpression,
    FeatureChainExpression,
    CollectExpression,
    SelectExpression,
    IndexExpression,
    ConstructorExpression,
    MetadataAccessExpression,
    Interaction,
    Flow,
    SuccessionFlow,
    PayloadFeature,
    FlowEnd,
    Metaclass,
    MetadataFeature,
    // ---- SysML (8.3)
    Definition,
    Usage,
    ReferenceUsage,
    VariantMembership,
    AttributeDefinition,
    AttributeUsage,
    EnumerationDefinition,
    EnumerationUsage,
    OccurrenceDefinition,
    OccurrenceUsage,
    EventOccurrenceUsage,
    ItemDefinition,
    ItemUsage,
    PartDefinition,
    PartUsage,
    PortDefinition,
    ConjugatedPortDefinition,
    PortConjugation,
    PortUsage,
    ConjugatedPortTyping,
    ConnectionDefinition,
    ConnectionUsage,
    BindingConnectorAsUsage,
    SuccessionAsUsage,
    InterfaceDefinition,
    InterfaceUsage,
    AllocationDefinition,
    AllocationUsage,
    FlowDefinition,
    FlowUsage,
    SuccessionFlowUsage,
    ActionDefinition,
    ActionUsage,
    PerformActionUsage,
    MergeNode,
    DecisionNode,
    JoinNode,
    ForkNode,
    SendActionUsage,
    AcceptActionUsage,
    TriggerInvocationExpression,
    AssignmentActionUsage,
    TerminateActionUsage,
    IfActionUsage,
    WhileLoopActionUsage,
    ForLoopActionUsage,
    StateDefinition,
    StateUsage,
    ExhibitStateUsage,
    StateSubactionMembership,
    TransitionUsage,
    TransitionFeatureMembership,
    CalculationDefinition,
    CalculationUsage,
    ConstraintDefinition,
    ConstraintUsage,
    AssertConstraintUsage,
    RequirementDefinition,
    RequirementUsage,
    SatisfyRequirementUsage,
    SubjectMembership,
    RequirementConstraintMembership,
    FramedConcernMembership,
    RequirementVerificationMembership,
    ActorMembership,
    StakeholderMembership,
    ConcernDefinition,
    ConcernUsage,
    CaseDefinition,
    CaseUsage,
    ObjectiveMembership,
    AnalysisCaseDefinition,
    AnalysisCaseUsage,
    VerificationCaseDefinition,
    VerificationCaseUsage,
    UseCaseDefinition,
    UseCaseUsage,
    IncludeUseCaseUsage,
    ViewDefinition,
    ViewUsage,
    ViewpointDefinition,
    ViewpointUsage,
    RenderingDefinition,
    RenderingUsage,
    ViewRenderingMembership,
    MembershipExpose,
    NamespaceExpose,
    MetadataDefinition,
    MetadataUsage,
}

use Metaclass as M;

impl fmt::Display for Metaclass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Metaclass {
    /// Whether this metaclass is a kind of `Relationship`.
    pub fn is_relationship(self) -> bool {
        self.is_membership()
            || self.is_import()
            || self.is_specialization()
            || matches!(
                self,
                M::Dependency
                    | M::Annotation
                    | M::Conjugation
                    | M::PortConjugation
                    | M::Disjoining
                    | M::Unioning
                    | M::Intersecting
                    | M::Differencing
                    | M::FeatureChaining
                    | M::FeatureInverting
                    | M::TypeFeaturing
            )
    }

    /// A kind of `Membership` (including `OwningMembership` and all its
    /// specializations, and `FeatureValue`).
    pub fn is_membership(self) -> bool {
        self == M::Membership || self.is_owning_membership()
    }

    /// A kind of `OwningMembership`.
    pub fn is_owning_membership(self) -> bool {
        matches!(
            self,
            M::OwningMembership
                | M::ElementFilterMembership
                | M::FeatureValue
                | M::VariantMembership
        ) || self.is_feature_membership()
    }

    /// A kind of `FeatureMembership`.
    pub fn is_feature_membership(self) -> bool {
        matches!(
            self,
            M::FeatureMembership
                | M::EndFeatureMembership
                | M::ParameterMembership
                | M::ReturnParameterMembership
                | M::ResultExpressionMembership
                | M::StateSubactionMembership
                | M::TransitionFeatureMembership
                | M::SubjectMembership
                | M::RequirementConstraintMembership
                | M::FramedConcernMembership
                | M::RequirementVerificationMembership
                | M::ActorMembership
                | M::StakeholderMembership
                | M::ObjectiveMembership
                | M::ViewRenderingMembership
        )
    }

    /// A kind of `ParameterMembership`.
    pub fn is_parameter_membership(self) -> bool {
        matches!(
            self,
            M::ParameterMembership
                | M::ReturnParameterMembership
                | M::SubjectMembership
                | M::ActorMembership
                | M::StakeholderMembership
        )
    }

    /// A kind of `Import` (SysML `Expose`s are imports).
    pub fn is_import(self) -> bool {
        matches!(
            self,
            M::MembershipImport | M::NamespaceImport | M::MembershipExpose | M::NamespaceExpose
        )
    }

    pub fn is_namespace_import(self) -> bool {
        matches!(self, M::NamespaceImport | M::NamespaceExpose)
    }

    /// A kind of `Specialization`.
    pub fn is_specialization(self) -> bool {
        matches!(
            self,
            M::Specialization
                | M::Subclassification
                | M::FeatureTyping
                | M::ConjugatedPortTyping
                | M::Subsetting
                | M::ReferenceSubsetting
                | M::CrossSubsetting
                | M::Redefinition
        )
    }

    /// A kind of `Subsetting`.
    pub fn is_subsetting(self) -> bool {
        matches!(
            self,
            M::Subsetting | M::ReferenceSubsetting | M::CrossSubsetting | M::Redefinition
        )
    }

    pub fn is_feature_typing(self) -> bool {
        matches!(self, M::FeatureTyping | M::ConjugatedPortTyping)
    }

    /// A kind of `AnnotatingElement`.
    pub fn is_annotating_element(self) -> bool {
        matches!(
            self,
            M::Comment
                | M::Documentation
                | M::TextualRepresentation
                | M::MetadataFeature
                | M::MetadataUsage
        )
    }

    /// A kind of `Namespace` (everything but relationships and the
    /// non-namespace annotating elements).
    pub fn is_namespace(self) -> bool {
        !self.is_relationship()
            && !matches!(
                self,
                M::Comment | M::Documentation | M::TextualRepresentation
            )
    }

    /// A kind of `Type`.
    pub fn is_type(self) -> bool {
        self.is_namespace() && !matches!(self, M::Namespace | M::Package | M::LibraryPackage)
    }

    /// A kind of `Feature`.
    pub fn is_feature(self) -> bool {
        self.is_type() && !self.is_classifier() && self != M::Type
    }

    /// A kind of `Classifier`.
    pub fn is_classifier(self) -> bool {
        matches!(
            self,
            M::Classifier
                | M::DataType
                | M::Class
                | M::Structure
                | M::Association
                | M::AssociationStructure
                | M::Behavior
                | M::Function
                | M::Predicate
                | M::Interaction
                | M::Metaclass
        ) || self.is_definition()
    }

    /// A kind of SysML `Definition`.
    pub fn is_definition(self) -> bool {
        matches!(
            self,
            M::Definition
                | M::AttributeDefinition
                | M::EnumerationDefinition
                | M::OccurrenceDefinition
                | M::ItemDefinition
                | M::PartDefinition
                | M::PortDefinition
                | M::ConjugatedPortDefinition
                | M::ConnectionDefinition
                | M::InterfaceDefinition
                | M::AllocationDefinition
                | M::FlowDefinition
                | M::ActionDefinition
                | M::StateDefinition
                | M::CalculationDefinition
                | M::ConstraintDefinition
                | M::RequirementDefinition
                | M::ConcernDefinition
                | M::CaseDefinition
                | M::AnalysisCaseDefinition
                | M::VerificationCaseDefinition
                | M::UseCaseDefinition
                | M::ViewDefinition
                | M::ViewpointDefinition
                | M::RenderingDefinition
                | M::MetadataDefinition
        )
    }

    /// A kind of SysML `Usage`.
    pub fn is_usage(self) -> bool {
        matches!(
            self,
            M::Usage
                | M::ReferenceUsage
                | M::AttributeUsage
                | M::EnumerationUsage
                | M::OccurrenceUsage
                | M::EventOccurrenceUsage
                | M::ItemUsage
                | M::PartUsage
                | M::PortUsage
                | M::ConnectionUsage
                | M::BindingConnectorAsUsage
                | M::SuccessionAsUsage
                | M::InterfaceUsage
                | M::AllocationUsage
                | M::FlowUsage
                | M::SuccessionFlowUsage
                | M::MetadataUsage
                | M::ViewUsage
                | M::RenderingUsage
        ) || self.is_action_usage()
            || self.is_constraint_usage()
    }

    /// A kind of `ActionUsage`.
    pub fn is_action_usage(self) -> bool {
        matches!(
            self,
            M::ActionUsage
                | M::PerformActionUsage
                | M::MergeNode
                | M::DecisionNode
                | M::JoinNode
                | M::ForkNode
                | M::SendActionUsage
                | M::AcceptActionUsage
                | M::AssignmentActionUsage
                | M::TerminateActionUsage
                | M::IfActionUsage
                | M::WhileLoopActionUsage
                | M::ForLoopActionUsage
                | M::StateUsage
                | M::ExhibitStateUsage
                | M::TransitionUsage
                | M::CalculationUsage
                | M::CaseUsage
                | M::AnalysisCaseUsage
                | M::VerificationCaseUsage
                | M::UseCaseUsage
                | M::IncludeUseCaseUsage
        )
    }

    /// A kind of `ConstraintUsage` (including requirements and their kin).
    pub fn is_constraint_usage(self) -> bool {
        matches!(
            self,
            M::ConstraintUsage
                | M::AssertConstraintUsage
                | M::RequirementUsage
                | M::SatisfyRequirementUsage
                | M::ConcernUsage
                | M::ViewpointUsage
        )
    }

    /// A kind of KerML `Expression` (incl. SysML calculations and
    /// constraints, which are expressions in the abstract syntax).
    pub fn is_expression(self) -> bool {
        matches!(
            self,
            M::Expression
                | M::BooleanExpression
                | M::Invariant
                | M::CalculationUsage
                | M::CaseUsage
                | M::AnalysisCaseUsage
                | M::VerificationCaseUsage
                | M::UseCaseUsage
                | M::IncludeUseCaseUsage
                | M::TriggerInvocationExpression
        ) || self.is_constraint_usage()
            || self.is_expression_node()
    }

    /// An expression produced by the expression sublanguage (operators,
    /// references, literals, invocations) rather than by a declaration.
    pub fn is_expression_node(self) -> bool {
        matches!(
            self,
            M::NullExpression
                | M::LiteralBoolean
                | M::LiteralString
                | M::LiteralInteger
                | M::LiteralRational
                | M::LiteralInfinity
                | M::FeatureReferenceExpression
                | M::InvocationExpression
                | M::OperatorExpression
                | M::FeatureChainExpression
                | M::CollectExpression
                | M::SelectExpression
                | M::IndexExpression
                | M::ConstructorExpression
                | M::MetadataAccessExpression
                | M::TriggerInvocationExpression
        )
    }

    /// Whether this is a kind of `Function` (calculation definitions and
    /// the like included).
    pub fn is_function(self) -> bool {
        matches!(
            self,
            M::Function
                | M::Predicate
                | M::CalculationDefinition
                | M::ConstraintDefinition
                | M::RequirementDefinition
                | M::ConcernDefinition
                | M::CaseDefinition
                | M::AnalysisCaseDefinition
                | M::VerificationCaseDefinition
                | M::UseCaseDefinition
                | M::ViewpointDefinition
        )
    }

    /// A kind of KerML `Behavior` (action definitions included).
    pub fn is_behavior(self) -> bool {
        matches!(
            self,
            M::Behavior
                | M::Interaction
                | M::ActionDefinition
                | M::StateDefinition
                | M::FlowDefinition
        ) || self.is_function()
    }

    /// A kind of KerML `Step` (action usages, expressions, flows).
    pub fn is_step(self) -> bool {
        matches!(
            self,
            M::Step | M::Flow | M::SuccessionFlow | M::FlowUsage | M::SuccessionFlowUsage
        ) || self.is_action_usage()
            || self.is_expression()
    }

    /// A kind of `Connector`.
    pub fn is_connector(self) -> bool {
        matches!(
            self,
            M::Connector
                | M::BindingConnector
                | M::Succession
                | M::Flow
                | M::SuccessionFlow
                | M::ConnectionUsage
                | M::BindingConnectorAsUsage
                | M::SuccessionAsUsage
                | M::InterfaceUsage
                | M::AllocationUsage
                | M::FlowUsage
                | M::SuccessionFlowUsage
        )
    }

    /// A kind of `Association`.
    pub fn is_association(self) -> bool {
        matches!(
            self,
            M::Association
                | M::AssociationStructure
                | M::Interaction
                | M::ConnectionDefinition
                | M::InterfaceDefinition
                | M::AllocationDefinition
                | M::FlowDefinition
        )
    }

    /// Metaclasses that are kinds of `Package`.
    pub fn is_package(self) -> bool {
        matches!(self, M::Package | M::LibraryPackage)
    }

    /// Metaclasses of `MetadataFeature` (`MetadataUsage` is one in SysML).
    pub fn is_metadata_feature(self) -> bool {
        matches!(self, M::MetadataFeature | M::MetadataUsage)
    }

    /// A kind of `OccurrenceDefinition`.
    pub fn is_occurrence_definition(self) -> bool {
        matches!(
            self,
            M::OccurrenceDefinition
                | M::ItemDefinition
                | M::PartDefinition
                | M::PortDefinition
                | M::ConjugatedPortDefinition
                | M::ConnectionDefinition
                | M::InterfaceDefinition
                | M::AllocationDefinition
                | M::FlowDefinition
                | M::ActionDefinition
                | M::StateDefinition
                | M::CalculationDefinition
                | M::ConstraintDefinition
                | M::RequirementDefinition
                | M::ConcernDefinition
                | M::CaseDefinition
                | M::AnalysisCaseDefinition
                | M::VerificationCaseDefinition
                | M::UseCaseDefinition
                | M::ViewDefinition
                | M::ViewpointDefinition
                | M::RenderingDefinition
                | M::MetadataDefinition
        )
    }

    /// A kind of `OccurrenceUsage`.
    pub fn is_occurrence_usage(self) -> bool {
        self.is_usage()
            && !matches!(
                self,
                M::Usage
                    | M::ReferenceUsage
                    | M::AttributeUsage
                    | M::EnumerationUsage
                    | M::BindingConnectorAsUsage
                    | M::SuccessionAsUsage
            )
    }

    /// A kind of `ItemUsage` (parts, ports are not items; connections are parts).
    pub fn is_item_usage(self) -> bool {
        matches!(
            self,
            M::ItemUsage
                | M::PartUsage
                | M::ConnectionUsage
                | M::InterfaceUsage
                | M::AllocationUsage
                | M::RenderingUsage
                | M::ViewUsage
                | M::MetadataUsage
        )
    }

    /// A kind of `PartUsage`.
    pub fn is_part_usage(self) -> bool {
        matches!(
            self,
            M::PartUsage
                | M::ConnectionUsage
                | M::InterfaceUsage
                | M::AllocationUsage
                | M::RenderingUsage
                | M::ViewUsage
        )
    }

    /// Whether the textual notation can produce an element of this
    /// metaclass that has no further structure (a bare relationship target).
    pub fn is_literal(self) -> bool {
        matches!(
            self,
            M::LiteralBoolean
                | M::LiteralString
                | M::LiteralInteger
                | M::LiteralRational
                | M::LiteralInfinity
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for &m in Metaclass::ALL {
            assert_eq!(Metaclass::from_name(m.name()), Some(m));
        }
    }

    #[test]
    fn classification() {
        assert!(M::PartDefinition.is_classifier());
        assert!(M::PartUsage.is_feature());
        assert!(!M::PartUsage.is_classifier());
        assert!(M::FeatureValue.is_owning_membership());
        assert!(M::ReturnParameterMembership.is_feature_membership());
        assert!(M::Redefinition.is_subsetting());
        assert!(M::NamespaceExpose.is_import());
        assert!(M::Package.is_namespace() && !M::Package.is_type());
        assert!(!M::Documentation.is_namespace());
        assert!(M::CalculationUsage.is_expression());
        assert!(M::MetadataUsage.is_feature());
    }
}
