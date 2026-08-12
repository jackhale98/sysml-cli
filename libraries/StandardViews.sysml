package StandardViews {
    doc
    /*
     * General-purpose model views, rendered by any tool implementing the
     * Reporting::TableRendering convention (e.g. `sysml view PortTable`).
     * These replace what used to be hard-coded CLI report commands; being
     * models, they can be copied and customized per project.
     */

    private import Reporting::*;

    view def PortTable {
        doc /* Every port with its owner, type, and direction. */
        @TableRendering {
            rows = "kind:port";
            columns = "element; parent; type; direction";
            sortBy = "parent";
        }
    }

    view def AllocationMatrix {
        doc /* Allocation relationships, source to target. */
        @TableRendering {
            rows = "relation:allocation";
            columns = "source; target";
            sortBy = "source";
        }
    }

    view def RequirementsTraceMatrix {
        doc
        /*
         * One row per requirement definition with satisfy/verify status —
         * the requirements traceability matrix.
         */
        @TableRendering {
            rows = "trace";
            columns = "requirement; id; satisfied; verified";
            sortBy = "requirement";
        }
    }

    view def ModelStats {
        doc /* Element counts by kind: the model at a glance. */
        @TableRendering {
            rows = "kindcounts";
            columns = "kind; definitions; usages";
        }
    }

    view def ConnectionTable {
        doc /* Every connection, source to target. */
        @TableRendering {
            rows = "relation:connection";
            columns = "source; target";
            sortBy = "source";
        }
    }
}
