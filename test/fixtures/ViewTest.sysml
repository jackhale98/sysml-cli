package ViewTestFixture {
    doc /* Self-contained view-rendering fixture: TableRendering spec,
         * @Fmea worksheet rows, pivot, and a stackup summary view that
         * pairs with UncertaintyStackup.sysml. */

    private import ScalarValues::*;

    metadata def TableRendering {
        attribute rows : String;
        attribute columns : String[0..1];
        attribute sortBy : String[0..1];
        attribute pivot : String[0..1];
        attribute where : String[0..1];
    }

    metadata def Fmea {
        attribute failureMode : String;
        attribute severity : Integer;
        attribute 'occurrence' : Integer;
        attribute detection : Integer;
    }

    view def Worksheet {
        @TableRendering {
            rows = "@Fmea";
            columns = "element; failureMode; severity; occurrence; detection; rpn = severity*occurrence*detection";
            sortBy = "-rpn";
        }
    }

    view def Matrix {
        @TableRendering {
            rows = "@Fmea";
            pivot = "severity; occurrence";
        }
    }

    view def StackupSummary {
        @TableRendering {
            rows = "uncertainty";
            columns = "case; nominal; wcMin; wcMax; margin; result";
            sortBy = "case";
        }
    }

    view def Stats {
        @TableRendering {
            rows = "kindcounts";
            columns = "kind; definitions; usages";
        }
    }

    part def Battery;
    part battery : Battery {
        @Fmea {
            failureMode = "Thermal runaway";
            severity = 9;
            'occurrence' = 3;
            detection = 4;
        }
        @Fmea {
            failureMode = "Capacity fade";
            severity = 5;
            'occurrence' = 6;
            detection = 3;
        }
    }
}
