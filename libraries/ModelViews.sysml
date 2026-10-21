library package ModelViews {
    doc
    /*
     * Standard tables over any SysML v2 model: requirements, coverage gaps,
     * traceability, interfaces and the bill of materials.
     *
     * Each table is an ordinary `view def` whose @TableRendering (from the
     * Reporting library) says which rows to collect and which columns to
     * show, so a tool renders it without knowing anything about it. To
     * look at only part of a model, type a view usage by one of these defs
     * and narrow it with `expose`:
     *
     *   view safetyRequirements : RequirementsTable {
     *       expose SafetyRequirements::*;
     *   }
     *
     * `expose P::*` keeps the members of P, `expose P::**` everything
     * nested in P, and `filter @SysML::PartUsage` keeps one kind of
     * element. A view without `expose` shows the whole model.
     */

    private import Reporting::*;

    view def RequirementsTable {
        doc /* Every requirement with its ID, statement, classification, status and trace links. */
        @TableRendering {
            rows = "requirements";
            columns = "id; title; text; kind; priority; verificationMethod; status; owner; satisfiedBy; verifiedBy";
            sortBy = "id";
        }
    }

    view def RequirementGaps {
        doc /* Requirements that nothing in the design satisfies yet, or that no verification case checks. */
        @TableRendering {
            rows = "requirements";
            columns = "id; title; satisfied; verified; satisfiedBy; verifiedBy; status; owner";
            where = "satisfied == 0 or verified == 0";
            sortBy = "id";
        }
    }

    view def TraceMatrix {
        doc /* Requirements down the side, the design elements that satisfy them across the top. */
        @TableRendering {
            rows = "relation:satisfy";
            pivot = "requirement; by";
            sortBy = "requirement";
        }
    }

    view def VerificationMatrix {
        doc /* Requirements down the side, the verification cases that check them across the top. */
        @TableRendering {
            rows = "relation:verify";
            pivot = "requirement; by";
            sortBy = "requirement";
        }
    }

    view def InterfaceTable {
        doc /* Every port: which part owns it, what flows through it, and what it is connected to. */
        @TableRendering {
            rows = "kind:port";
            columns = "owner; element; type; direction; connectedTo; via";
            sortBy = "owner";
        }
    }

    view def BillOfMaterials {
        doc /* What the system is made of: each part with how many are used, per assembly and in total. */
        @TableRendering {
            rows = "composition";
            columns = "path; type; quantity; extended; description = doc";
            sortBy = "path";
        }
    }
}
