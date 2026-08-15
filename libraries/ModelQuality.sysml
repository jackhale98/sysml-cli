package ModelQuality {
    doc
    /*
     * Model-completeness scoring vocabulary.
     *
     * `sysml coverage` computes four metric percentages and looks for a
     * calc named QualityScore in the loaded model. When one is present
     * (import this package, or declare your own), its return expression
     * is evaluated for the overall score; otherwise the tool falls back
     * to equal weighting. The parameter vocabulary is fixed — the tool
     * binds these four names, each 0-100 — but the expression may use
     * any subset with any weighting, so scoring policy lives in the
     * model, not in tool configuration:
     *
     *   calc def QualityScore {
     *       in documented : Real;
     *       in typedUsages : Real;
     *       in reqSatisfied : Real;
     *       in reqVerified : Real;
     *       return : Real = documented * 0.1 + typedUsages * 0.1
     *                     + reqSatisfied * 0.4 + reqVerified * 0.4;
     *   }
     *
     * Gate CI on the result with:
     *   sysml coverage --check --min-score 80 -I libraries src/*.sysml
     */

    private import ScalarValues::*;

    calc def QualityScore {
        doc
        /*
         * Overall model quality score, 0-100: the default equal
         * weighting of documentation coverage, usage typing, and
         * requirement satisfaction/verification.
         */
        in documented : Real;
        in typedUsages : Real;
        in reqSatisfied : Real;
        in reqVerified : Real;
        return : Real = documented * 0.25 + typedUsages * 0.25
                      + reqSatisfied * 0.25 + reqVerified * 0.25;
    }

    constraint def QualityGate {
        doc
        /*
         * CI gate for `sysml coverage --check`. Inert as a definition;
         * declaring a constraint usage typed by it activates the gate:
         *
         *   constraint qualityGate : QualityGate { :>> minScore = 80.0; }
         *
         * The tool binds `score` (and the four metric percentages, all
         * 0-100) and fails the check when the expression is false.
         * Specialize or write your own gate def to constrain individual
         * metrics — e.g. `reqVerified >= 90.0 and score >= 70.0`.
         * With no gate declared, --check requires a perfect score.
         */
        in score : Real;
        in documented : Real;
        in typedUsages : Real;
        in reqSatisfied : Real;
        in reqVerified : Real;
        attribute minScore : Real default 100.0;
        score >= minScore
    }

    constraint def TraceGate {
        doc
        /*
         * CI gate for `sysml trace --check`, activated by a constraint
         * usage the same way as QualityGate:
         *
         *   constraint traceGate : TraceGate { :>> minVerified = 60.0; }
         *
         * The tool binds `satisfied`, `verified`, and `traced` (each
         * 0-100: percent of requirements satisfied, verified, and fully
         * traced). With no gate declared, --check requires every
         * requirement satisfied and verified.
         */
        in satisfied : Real;
        in verified : Real;
        in traced : Real;
        attribute minSatisfied : Real default 100.0;
        attribute minVerified : Real default 100.0;
        satisfied >= minSatisfied and verified >= minVerified
    }
}
