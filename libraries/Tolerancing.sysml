package Tolerancing {
    doc
    /*
     * Dimensional tolerancing and GD&T vocabulary for SysML v2.
     *
     * Provides toleranced dimensions, geometric features, feature control
     * frames per ASME Y14.5, mates (fits), and tolerance stackup analyses.
     *
     * Design notes:
     * - Stackup contributors REFERENCE feature dimensions through feature
     *   chains, so there is a single source of truth: retolerancing a
     *   feature automatically retolerances every stackup that uses it.
     * - Stackups specialize Uncertainty::UncertaintyAnalysis, so any
     *   analyzer implementing the generic worst-case / RSS / Monte Carlo
     *   methods can evaluate them with no tolerancing-specific logic.
     * - Dimension values are unitless Reals interpreted in the unit named
     *   by the dimension's `unit` attribute (default "mm"). All dimensions
     *   in one stackup chain must share a unit; analyzers should check this.
     */

    private import ScalarValues::*;
    private import Uncertainty::*;

    attribute def ToleratedDimension :> UncertainValue {
        doc
        /*
         * A toleranced linear or diametral dimension:
         * nominal +plus/-minus, in `unit` (default millimetres).
         */
        attribute unit : String default "mm";
    }

    enum def FeatureNature {
        doc
        /*
         * Whether material surrounds the feature (internal, e.g. a hole or
         * slot) or the feature is surrounded by air (external, e.g. a shaft
         * or tab). Determines MMC/LMC interpretation and which side of a
         * fit the feature is on.
         */
        enum internal;
        enum external;
    }

    enum def FeatureForm {
        doc /* Common geometric feature forms. */
        enum hole;
        enum shaft;
        enum pin;
        enum slot;
        enum tab;
        enum plane;
        enum sphere;
        enum cone;
        enum surface;
    }

    enum def GeometricCharacteristic {
        doc
        /*
         * The 14 geometric characteristic symbols of ASME Y14.5.
         */
        enum straightness;
        enum flatness;
        enum circularity;
        enum cylindricity;
        enum lineProfile;
        enum surfaceProfile;
        enum perpendicularity;
        enum angularity;
        enum parallelism;
        enum position;
        enum concentricity;
        enum symmetry;
        enum circularRunout;
        enum totalRunout;
    }

    enum def MaterialCondition {
        doc /* Material condition modifiers per ASME Y14.5. */
        enum rfs;
        enum mmc;
        enum lmc;
    }

    attribute def FeatureControlFrame {
        doc
        /*
         * A GD&T feature control frame: geometric characteristic, tolerance
         * zone width (diametral for position), material condition modifier,
         * and up to three datum references in precedence order.
         */
        attribute characteristic : GeometricCharacteristic;
        attribute zone : Real;
        attribute materialCondition : MaterialCondition default MaterialCondition::rfs;
        attribute datumRefs : String[0..3] ordered;

        assert constraint zoneNonNegative { zone >= 0.0 }
    }

    attribute def Frame3D {
        doc
        /*
         * Local placement of a feature for 3D chain analysis:
         * origin point and axis direction in the owning part's
         * coordinate system. Reserved for future small-displacement
         * torsor (SDT) analysis; 1D stackups ignore it.
         */
        attribute origin : Real[3] ordered;
        attribute axis : Real[3] ordered;
    }

    item def GeometricFeature {
        doc
        /*
         * A geometric feature of a part: a hole, shaft, plane, slot...
         * Nest inside the owning part and add its toleranced dimensions
         * as ToleratedDimension attributes:
         *
         *   item bore : GeometricFeature {
         *       :>> form = FeatureForm::hole;
         *       :>> nature = FeatureNature::internal;
         *       attribute diameter : ToleratedDimension {
         *           :>> nominal = 10.0; :>> plus = 0.1; :>> minus = 0.05;
         *       }
         *   }
         *
         * Set `datum` (e.g. "A") when the feature serves as a datum.
         */
        attribute form : FeatureForm;
        attribute nature : FeatureNature default FeatureNature::external;
        attribute datum : String[0..1];
        attribute controls : FeatureControlFrame[0..*];
        attribute placement : Frame3D[0..1];
    }

    enum def FitType {
        doc /* Classification of a mate per ISO 286 fit categories. */
        enum clearance;
        enum transition;
        enum interference;
    }

    connection def Mate {
        doc
        /*
         * A mating relationship between two geometric features, typically
         * one internal (hole) and one external (shaft). `expectedFit`
         * states design intent; analyzers compute the actual worst-case
         * fit from the mated dimensions (see MinClearance / MaxClearance)
         * and flag disagreement with the stated intent.
         */
        end featureA : GeometricFeature;
        end featureB : GeometricFeature;
        attribute expectedFit : FitType[0..1];
    }

    calc def MinClearance {
        doc
        /*
         * Worst-case minimum clearance of a fit: smallest internal
         * dimension minus largest external dimension. Negative values
         * indicate interference.
         */
        in holeDim : UncertainValue;
        in shaftDim : UncertainValue;
        return : Real = (holeDim.nominal - holeDim.minus) - (shaftDim.nominal + shaftDim.plus);
    }

    calc def MaxClearance {
        doc
        /*
         * Worst-case maximum clearance of a fit: largest internal
         * dimension minus smallest external dimension.
         */
        in holeDim : UncertainValue;
        in shaftDim : UncertainValue;
        return : Real = (holeDim.nominal + holeDim.plus) - (shaftDim.nominal - shaftDim.minus);
    }

    enum def Sense {
        doc
        /*
         * Direction a dimension acts in a stackup chain: positive
         * contributions increase the closing dimension, negative
         * contributions decrease it.
         */
        enum positive;
        enum negative;
    }

    attribute def Contribution {
        doc
        /*
         * One dimension's participation in a stackup chain.
         *
         * Bind `dim` to a feature dimension through a feature chain
         * (e.g. `:>> dim = housing.bore.diameter;`) rather than restating
         * values, so the stackup always reflects the current tolerance.
         * `source` optionally records the drawing or specification the
         * dimension comes from.
         */
        attribute dim : ToleratedDimension;
        attribute sense : Sense default Sense::positive;
        attribute source : String[0..1];
    }

    analysis def ToleranceStackup :> UncertaintyAnalysis {
        doc
        /*
         * A 1D tolerance chain analysis. The measured closing dimension
         * (gap, clearance, engagement...) is the signed sum of the
         * contributions:
         *
         *   closing = sum over contributions of (+/- dim)
         *
         * with sign taken from each contribution's `sense`. The analysis
         * passes when the closing dimension stays within `target`
         * (inherited from UncertaintyAnalysis) under the chosen
         * evaluation method.
         *
         * Evaluate with a generic uncertainty analyzer, e.g.:
         *   sysml analyze <stackup> --method worst-case
         *   sysml analyze <stackup> --method rss
         *   sysml analyze <stackup> --method monte-carlo --iterations 50000
         *
         * Declare contributors by subsetting the inherited collection:
         *   attribute housingDepth :> contributions { :>> dim = ...; }
         * Analyzers must also accept plain Contribution-typed attributes
         * as contributors, in declaration order.
         *
         * Mark `critical` for dimensions on the critical-to-quality list;
         * reports and lint thresholds can key off it.
         */
        attribute contributions : Contribution[1..*] ordered;
        attribute critical : Boolean default false;
    }

    view def StackupSummary {
        doc
        /*
         * Tabular report: one row per ToleranceStackup usage with target
         * limits, worst-case range, RSS Cpk, and pass/fail status.
         * Rendered by tools implementing the generic view-rendering primitive
         * (e.g. `sysml view StackupSummary`).
         */
    }

    view def FitTable {
        doc
        /*
         * Tabular report: one row per Mate usage with mated dimensions,
         * worst-case min/max clearance, computed fit type, and expected
         * fit agreement.
         */
    }
}
