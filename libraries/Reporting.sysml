package Reporting {
    doc
    /*
     * The table-rendering convention for SysML v2 views.
     *
     * SysML v2's view machinery (`view def`, `expose`, `filter`,
     * `render as`) specifies what a view includes; it does not define a
     * vocabulary for tabular layout. @TableRendering fills that gap so
     * the entire report specification lives in the model: any tool
     * implementing this convention renders identical tables, and users
     * add reports by writing view defs, never tool code.
     *
     * Attach to a view def:
     *
     *   view def FmeaWorksheet {
     *       @TableRendering {
     *           rows = "@Fmea";
     *           columns = "element; failureMode; severity; occurrence;
     *                      detection; rpn = severity*occurrence*detection";
     *           sortBy = "-rpn";
     *       }
     *   }
     *
     * Lists are `;`-separated so column expressions may contain commas.
     *
     * Row providers (the `rows` value):
     *   "@Fmea"                 rows are annotations of a metadata type;
     *                           fields = element, file, line + the
     *                           annotation's values
     *   "type:Hazard"           rows are usages typed by a definition or
     *                           any of its specializations; fields =
     *                           element, type, kind, parent, file, line +
     *                           body values (`:>> description = ...`)
     *   "kind:port"             rows are usages of a syntactic kind;
     *                           fields = element, parent, type, direction
     *   "relation:allocation"   allocation pairs (source, target)
     *   "relation:satisfy"      satisfy relationships (requirement, by)
     *   "relation:verify"       verify relationships (requirement, by)
     *   "relation:connection"   connections (source, target)
     *   "trace"                 requirement coverage (requirement, id,
     *                           satisfied, verified)
     *   "kindcounts"            model statistics (kind, definitions,
     *                           usages)
     *   "uncertainty"           evaluated uncertainty analyses (case,
     *                           critical, target limits, worst-case
     *                           range, margin, cp, cpk, result)
     *
     * Column syntax: `name` copies a row field; `name = expression`
     * computes a value from the row's numeric fields with the standard
     * expression language (so derived values like RPN are never stored).
     * `where` filters rows with a boolean expression; `sortBy` orders by
     * a column (`-` prefix for descending); `pivot = "rowField; colField"`
     * renders a count grid instead of columns.
     */

    private import ScalarValues::*;

    metadata def TableRendering {
        attribute rows : String;
        attribute columns : String[0..1];
        attribute sortBy : String[0..1];
        attribute pivot : String[0..1];
        attribute where : String[0..1];
    }
}
