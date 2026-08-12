package HazardAnalysis {
    doc
    /*
     * Hazard analysis vocabulary for SysML v2, aligned with the OMG RAAML
     * Core library and the ISO 14971 / MIL-STD-882E process.
     *
     * Hazard analysis is top-down: identify the harms the system can cause,
     * the hazards that produce them, and the hazardous situations that
     * expose people, property, or the environment to those hazards. Its
     * results DRIVE bottom-up analyses (see RiskAnalysis): a failure mode's
     * severity derives from the harm it can precipitate, connected through
     * Causation links, so severity is stated once - on the harm - and
     * checked everywhere.
     *
     * RAAML 1.0 is a UML/SysML v1 profile and cannot be imported into
     * SysML v2 models; this package re-expresses its Core concepts natively.
     * Name mapping (RAAML Core -> here):
     *   Harm                -> Harm
     *   Hazard              -> Hazard
     *   Situation           -> HazardousSituation
     *   causal relationship -> Causation
     *   ControllingMeasure  -> RiskControl
     * When OMG publishes RAAML for SysML v2, migration is a rename.
     */

    private import ScalarValues::*;

    abstract occurrence def RiskEvent {
        doc
        /*
         * Base for everything that can appear in a causal risk chain:
         * hazards, hazardous situations, harms, and (in RiskAnalysis)
         * failure modes. Causation connects RiskEvents.
         */
    }

    enum def SeverityScale :> Integer {
        doc
        /*
         * Harm severity classes with canonical anchor values on the FMEA
         * 1-10 severity scale, so top-down hazard analysis and bottom-up
         * FMEA share one number line:
         *   negligible   (2): inconvenience or temporary discomfort
         *   minor        (4): temporary injury not requiring intervention
         *   serious      (6): injury requiring professional intervention
         *   critical     (8): permanent impairment or major property loss
         *   catastrophic (10): loss of life
         * An @Fmea line linked to a Harm must use the harm's anchor as its
         * severity; conforming tools flag disagreement.
         */
        negligible = 2;
        minor = 4;
        serious = 6;
        critical = 8;
        catastrophic = 10;
    }

    attribute def ProbabilityLevel :> Real {
        doc /* A probability in the closed interval 0.0 to 1.0. */
        assert constraint unitInterval { that >= 0.0 and that <= 1.0 }
    }

    occurrence def Harm :> RiskEvent {
        doc
        /*
         * Physical injury or damage to health, property, or the
         * environment (ISO 14971). Carries the severity classification
         * that downstream analyses derive from.
         */
        attribute description : String[0..1];
        attribute severity : SeverityScale;
    }

    occurrence def Hazard :> RiskEvent {
        doc /* A potential source of harm (ISO 14971). */
        attribute description : String[0..1];
    }

    occurrence def HazardousSituation :> RiskEvent {
        doc
        /*
         * A circumstance in which people, property, or the environment
         * are exposed to one or more hazards (ISO 14971).
         *
         * p1: probability that the situation arises
         * p2: probability that the situation leads to the harm
         * Overall probability of harm = p1 * p2 (HarmProbability calc).
         */
        attribute description : String[0..1];
        attribute p1 : ProbabilityLevel[0..1];
        attribute p2 : ProbabilityLevel[0..1];
    }

    connection def Causation {
        doc
        /*
         * Directed causal link between risk events: the cause contributes
         * to bringing about the effect. Chains typically run
         *   FailureMode -> Hazard -> HazardousSituation -> Harm.
         * These links are also the substrate for future fault tree
         * analysis (an FTA is this causation graph plus gate semantics).
         */
        end cause : RiskEvent;
        end effect : RiskEvent;
    }

    calc def HarmProbability {
        doc /* Probability of harm for a hazardous situation: p1 * p2. */
        in p1 : ProbabilityLevel;
        in p2 : ProbabilityLevel;
        return : Real = p1 * p2;
    }

    enum def ControlHierarchy {
        doc
        /*
         * ISO 14971 risk-control option hierarchy, in order of
         * preference. Controls higher in the hierarchy are inherently
         * more reliable than those below.
         */
        enum inherentSafety;
        enum protectiveMeasure;
        enum informationForSafety;
    }

    requirement def RiskControl {
        doc
        /*
         * A risk-control obligation (RAAML: ControllingMeasure): a
         * requirement whose satisfaction reduces the probability or
         * severity of a harm. Specialize or instantiate it, `satisfy` it
         * by the design element implementing the control, and `verify` it
         * with the evidence. RiskAnalysis::Mitigation actions are the
         * work items that implement RiskControls.
         */
        attribute hierarchy : ControlHierarchy;
        attribute rationale : String[0..1];
    }

    view def HazardLog {
        doc
        /*
         * Tabular report: one row per Hazard with its causal chain -
         * contributing failure modes, hazardous situations (p1, p2,
         * probability of harm), harms with severity, and the status of
         * linked RiskControls. Rendered by tools implementing the generic
         * report primitive (e.g. `sysml view HazardLog`).
         */
    }
}
