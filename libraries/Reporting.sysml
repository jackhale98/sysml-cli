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
     *           columns = "element; failureMode; severity; likelihood;
     *                      detection; rpn = severity*likelihood*detection";
     *           sortBy = "-rpn";
     *       }
     *   }
     *
     * Lists are `;`-separated so column expressions may contain commas.
     *
     * A `view` usage typed by such a def renders the same table: it
     * inherits the def's @TableRendering (an annotation on the usage
     * overrides single fields, e.g. adds a `where`), and its clauses
     * narrow the rows:
     *
     *   view batteryFmea : FmeaWorksheet { expose Power::**; }
     *
     *   expose P        the element P itself
     *   expose P::*     the members of P
     *   expose P::**    everything nested in P
     *   filter @SysML::PartUsage   rows whose element is of that metaclass
     *   filter @Fmea               rows whose element carries @Fmea
     *
     * A row is exposed when its element, or the namespace it is written
     * in, matches (a satisfy row also through its requirement). Without
     * `expose` a view shows the whole model.
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
     *                           fields = element, owner (= parent), type
     *                           (`~T` when conjugated), direction (declared,
     *                           or implied by a port def's features and
     *                           conjugation: in, out, inout), conjugated
     *                           (1/0), connectedTo (far ends of the
     *                           connections ending on it), via (those
     *                           connections), doc, file, line
     *   "relation:allocation"   allocation pairs (source, target)
     *   "relation:satisfy"      satisfy relationships (requirement as
     *                           "ID title", id, title, target as written,
     *                           by, file, line)
     *   "relation:verify"       verify relationships (same fields)
     *   "relation:connection"   connections (connection, type, source,
     *                           target); `relation:connection:Mate` keeps
     *                           connections typed by Mate
     *   "relation:succession"   successions (succession, type, source,
     *                           target, parent)
     *   "composition"           the part/item tree from every top-level
     *   "composition:Root"      element (or from Root, a def or a usage),
     *                           following usage bodies and types: a bill
     *                           of materials. Fields = element, type,
     *                           parent, path (a.b.c), depth, quantity
     *                           (from multiplicity), extended (quantity
     *                           multiplied down the tree), doc, file, line
     *                           + attribute values of the usage and its
     *                           type (mass, cost, ...)
     *   "requirements"          one row per requirement usage, and per
     *                           requirement def that is not just the type
     *                           of requirement usages; fields = element,
     *                           qualifiedName, id (short name), title
     *                           (name), text (doc), type, every attribute
     *                           value set in the body (kind, priority,
     *                           verificationMethod, ...; enum values by
     *                           name, lists comma-separated), status and
     *                           owner (@StatusInfo), rationale
     *                           (@Rationale), issues (@Issue), satisfiedBy
     *                           and verifiedBy (comma lists, from every
     *                           file), satisfied and verified (1 or 0, so
     *                           `where = "verified == 0"` selects gaps),
     *                           file, line
     *   "trace"                 requirement coverage: the requirements
     *                           rows with requirement (= title) and
     *                           satisfied/verified as yes/no
     *   "kindcounts"            model statistics (kind, definitions,
     *                           usages)
     *   "uncertainty"           evaluated uncertainty analyses (case,
     *                           critical, target limits, worst-case
     *                           range, margin, cp, cpk, result)
     *
     * Column syntax: `name` copies a row field; `name = expression`
     * computes a value from the row's numeric fields with the standard
     * expression language (so derived values like RPN are never stored);
     * `name = field` renames a field. `where` filters rows with a boolean
     * expression; `sortBy` orders by a column (`-` prefix for
     * descending); `pivot = "rowField; colField"` renders a count grid
     * instead of columns (rows ordered by `sortBy` when it names the row
     * field, otherwise descending).
     */

    private import ScalarValues::*;

    metadata def TableRendering {
        attribute rows : String;
        attribute columns : String[0..1];
        attribute sortBy : String[0..1];
        attribute pivot : String[0..1];
        attribute where : String[0..1];
    }

    // --- Domain-neutral views usable in any model ---------------------

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
         * One row per requirement with satisfy/verify status — the
         * requirements traceability matrix.
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
