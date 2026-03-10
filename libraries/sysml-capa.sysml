package SysMLCAPA {
    doc /*
     * Domain library for quality management.
     *
     * Three distinct quality item types:
     * - NCR (Nonconformance Report) — documents observed nonconformances
     * - CAPA (Corrective/Preventive Action) — formal action programs
     * - Process Deviation — planned departures from standard processes
     *
     * Each type has its own lifecycle states, distinct from the others.
     */

    private import ScalarValues::*;

    // =====================================================================
    // Shared enums
    // =====================================================================

    enum def NonconformanceCategory {
        doc /* Classifies a nonconformance by the type of defect observed. */
        enum dimensional;
        enum material;
        enum cosmetic;
        enum functional;
        enum workmanship;
        enum documentation;
        enum labeling;
        enum packaging;
        enum contamination;
        enum software;
    }

    enum def SeverityClass {
        doc /* Rates the severity of a nonconformance for risk prioritization. */
        enum critical;
        enum major;
        enum minor;
        enum observation;
    }

    enum def Disposition {
        doc /* The material review board decision for a nonconforming item. */
        enum useAsIs;
        enum rework;
        enum repair;
        enum scrap;
        enum returnToVendor;
        enum sortAndScreen;
        enum deviate;
    }

    enum def CorrectiveActionType {
        doc /* Categorizes the type of corrective action taken. */
        enum designChange;
        enum processChange;
        enum supplierChange;
        enum toolingChange;
        enum trainingRetraining;
        enum procedureUpdate;
        enum inspectionEnhancement;
        enum containment;
        enum noActionRequired;
    }

    // =====================================================================
    // NCR lifecycle
    // =====================================================================

    enum def NcrStatus {
        doc /* Lifecycle states for a nonconformance report. */
        enum open;
        enum investigating;
        enum dispositioned;
        enum verified;
        enum closed;
        enum reopened;
    }

    // =====================================================================
    // CAPA lifecycle
    // =====================================================================

    enum def CapaStatus {
        doc /* Lifecycle states for a corrective/preventive action program. */
        enum initiated;
        enum rootCauseAnalysis;
        enum planningActions;
        enum implementing;
        enum verifyingEffectiveness;
        enum pendingClosure;
        enum closed;
        enum closedIneffective;
    }

    enum def CapaSource {
        doc /* What triggered the CAPA. */
        enum ncr;
        enum auditFinding;
        enum customerComplaint;
        enum processImprovement;
        enum regulatoryObservation;
        enum managementReview;
    }

    enum def CapaType {
        doc /* Whether the CAPA is corrective or preventive. */
        enum corrective;
        enum preventive;
    }

    // =====================================================================
    // Process Deviation lifecycle
    // =====================================================================

    enum def DeviationStatus {
        doc /* Lifecycle states for a process deviation. */
        enum requested;
        enum underReview;
        enum approved;
        enum denied;
        enum active;
        enum expired;
        enum closed;
    }

    enum def DeviationScope {
        doc /* Scope of a process deviation. */
        enum lot;
        enum processStep;
        enum productLine;
        enum temporary;
        enum permanent;
    }
}
