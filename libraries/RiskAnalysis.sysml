package RiskAnalysis {
    doc
    /*
     * FMEA and hazard analysis vocabulary for SysML v2.
     *
     * Supports two complementary styles:
     *
     * 1. Lightweight annotation - attach an @Fmea metadata line item to any
     *    element (part, action, process step). Ideal for FMEA worksheets.
     *
     * 2. Structural analysis - model FailureMode as a first-class
     *    occurrence wired into HazardAnalysis causal chains
     *    (FailureMode -> Hazard -> HazardousSituation -> Harm), for
     *    ISO 14971 / MIL-STD-882E style safety cases.
     *
     * Hazard analysis drives the FMEA: when a failure mode (or @Fmea line,
     * via hazardRef) links to a hazard, its severity derives top-down from
     * the harm at the end of that hazard's causal chain - see
     * HazardAnalysis::SeverityScale for the shared 1-10 anchors. Occurrence
     * and detection remain bottom-up properties of the failure mode.
     *
     * Ratings follow the AIAG/VDA and SAE J1739 ordinal 1-10 scales:
     *   Severity:   1 = negligible ... 10 = catastrophic, safety hazard
     *   Occurrence: 1 = very unlikely ... 10 = almost certain
     *   Detection:  1 = always detected ... 10 = cannot be detected
     *   RPN = S x O x D (1..1000), computed by the Rpn calc - never stored,
     *   so it can never disagree with its factors.
     *
     * Mitigations link to risks with the standard `satisfy` relationship
     * and to their proof with `verify`, so generic traceability tooling
     * answers "which high-RPN risks lack verified mitigations" without
     * any risk-specific support.
     */

    private import ScalarValues::*;
    private import HazardAnalysis::*;

    attribute def FmeaRating :> Integer {
        doc /* Ordinal FMEA rating on the AIAG/VDA 1-10 scale. */
        assert constraint inRange { that >= 1 and that <= 10 }
    }

    calc def Rpn {
        doc
        /*
         * Risk Priority Number: severity x occurrence x detection,
         * range 1..1000. Common action thresholds:
         *   1-50 low, 51-150 medium, 151-400 high, 401+ critical.
         */
        in severity : FmeaRating;
        in occurrence : FmeaRating;
        in detection : FmeaRating;
        return : Integer = severity * occurrence * detection;
    }

    enum def RiskCategory {
        doc /* FMEA domain per the source of the failure mode. */
        enum design;
        enum process;
        enum use;
        enum software;
    }

    enum def MitigationKind {
        doc
        /*
         * prevention reduces occurrence; detection improves the ability
         * to catch the failure before it causes harm.
         */
        enum prevention;
        enum detection;
    }

    enum def MitigationStatus {
        doc /* Lifecycle of a mitigation action. */
        enum proposed;
        enum inProgress;
        enum completed;
        enum verified;
    }

    metadata def Fmea {
        doc
        /*
         * One FMEA worksheet line item, attached to the element whose
         * failure is being analyzed:
         *
         *   part battery : Battery {
         *       @Fmea {
         *           failureMode = "Thermal runaway";
         *           cause = "Internal short from dendrite growth";
         *           effect = "Fire; loss of vehicle";
         *           severity = 9; occurrence = 3; detection = 4;
         *       }
         *   }
         *
         * RPN is derived via the Rpn calc; `initialSeverity` etc. optionally
         * record the pre-mitigation assessment so risk reduction is visible.
         */
        attribute failureMode : String;
        attribute cause : String[0..1];
        attribute effect : String[0..1];
        attribute category : RiskCategory default RiskCategory::design;
        attribute severity : FmeaRating;
        attribute occurrence : FmeaRating;
        attribute detection : FmeaRating;
        attribute initialSeverity : FmeaRating[0..1];
        attribute initialOccurrence : FmeaRating[0..1];
        attribute initialDetection : FmeaRating[0..1];
        attribute hazardRef : String[0..1] {
            doc
            /*
             * Qualified name of the HazardAnalysis::Hazard this failure
             * mode can precipitate. When set, conforming tools resolve the
             * hazard's causal chain to its Harm and flag this line if
             * `severity` disagrees with the harm's SeverityScale anchor.
             */
        }
    }

    occurrence def FailureMode :> RiskEvent {
        doc
        /*
         * A specific way an element can fail, with its cause, effect,
         * and FMEA ratings. Use alongside or instead of the @Fmea
         * annotation when the causal structure itself matters. As a
         * RiskEvent, a FailureMode connects into HazardAnalysis causal
         * chains with Causation (failure mode -> hazard); its severity
         * then derives from the harm at the end of the chain.
         */
        attribute cause : String[0..1];
        attribute effect : String[0..1];
        attribute severity : FmeaRating[0..1];
        attribute occurrence : FmeaRating[0..1];
        attribute detection : FmeaRating[0..1];
    }

    action def Mitigation {
        doc
        /*
         * An action taken to reduce a risk: the work item that implements
         * a HazardAnalysis::RiskControl obligation. Connect it to the risk
         * it addresses with `satisfy`, and to the evidence of its
         * effectiveness with `verify`.
         */
        attribute kind : MitigationKind;
        attribute status : MitigationStatus default MitigationStatus::proposed;
        attribute owner : String[0..1];
    }

    view def FmeaWorksheet {
        doc
        /*
         * Tabular report: one row per @Fmea annotation with the annotated
         * element, failure mode, cause, effect, S/O/D, and computed RPN,
         * sorted by RPN descending. Rendered by tools implementing the
         * generic view-rendering primitive (e.g. `sysml view FmeaWorksheet`).
         */
    }

    view def RiskMatrix {
        doc
        /*
         * Pivot report: severity x occurrence grid with the count (or IDs)
         * of @Fmea items in each cell.
         */
    }
}
