package Uncertainty {
    doc
    /*
     * Core conventions for scalar quantities with bounded uncertainty.
     *
     * This package is the contract between models and generic uncertainty
     * analyzers. A conforming analyzer evaluates any calculation or analysis
     * whose inputs are UncertainValues, using one of three methods:
     *
     *   worst-case   - interval arithmetic over [lower, upper] bounds
     *   rss          - linear (root-sum-square) variance propagation,
     *                  sigma per input = (plus + minus) / sigmaLevel
     *   monte-carlo  - random sampling from each input's distribution
     *
     * Domain vocabularies (e.g. Tolerancing) specialize these definitions;
     * the analyzer itself needs no domain knowledge.
     */

    private import ScalarValues::*;

    enum def Distribution {
        doc
        /*
         * Statistical distribution assumed for an uncertain quantity.
         * normal:     bell curve centered on nominal (stable processes)
         * uniform:    equal probability across [lower, upper] (vendor limits,
         *             unknown processes)
         * triangular: peak at nominal, tapering to the bounds (assembly by
         *             skilled operators)
         */
        enum normal;
        enum uniform;
        enum triangular;
    }

    attribute def UncertainValue {
        doc
        /*
         * A scalar with a nominal value and asymmetric bounds.
         *
         *   upper bound = nominal + plus
         *   lower bound = nominal - minus
         *
         * plus and minus are magnitudes and always non-negative
         * (10.0 +0.1/-0.05 has plus = 0.1, minus = 0.05).
         */
        attribute nominal : Real;
        attribute plus : Real;
        attribute minus : Real;
        attribute distribution : Distribution default Distribution::normal;

        assert constraint boundsNonNegative {
            plus >= 0.0 and minus >= 0.0
        }
    }

    attribute def SymmetricValue :> UncertainValue {
        doc
        /*
         * A symmetric uncertain value, nominal +/- tol.
         * plus and minus both equal tol.
         */
        attribute tol : Real;

        assert constraint symmetricBounds {
            plus == tol and minus == tol
        }
    }

    attribute def LimitRange {
        doc
        /*
         * A specification target: nominal value with lower and upper
         * specification limits (LSL / USL). Used as the pass/fail
         * criterion for uncertainty analyses.
         */
        attribute nominal : Real;
        attribute lower : Real;
        attribute upper : Real;

        assert constraint wellOrdered {
            lower <= nominal and nominal <= upper
        }
    }

    calc def UpperBound {
        doc /* Worst-case upper bound of an UncertainValue. */
        in v : UncertainValue;
        return : Real = v.nominal + v.plus;
    }

    calc def LowerBound {
        doc /* Worst-case lower bound of an UncertainValue. */
        in v : UncertainValue;
        return : Real = v.nominal - v.minus;
    }

    analysis def UncertaintyAnalysis {
        doc
        /*
         * Base definition for analyses evaluated by uncertainty propagation.
         *
         * target       pass/fail specification limits for the result
         * sigmaLevel   divisor mapping a tolerance band to one standard
         *              deviation: sigma = (plus + minus) / sigmaLevel.
         *              Default 6.0 treats the band as a +/-3-sigma process.
         * meanShiftK   Bender k-factor; shifts the predicted mean k * sigma
         *              toward the nearest specification limit to model
         *              long-term process drift (1.5 is the automotive
         *              convention; 0 disables the shift).
         * iterations   Monte Carlo sample count.
         * seed         Optional Monte Carlo seed. Fixing the seed makes runs
         *              bit-for-bit reproducible; analyzers must record the
         *              seed used in their output either way.
         */
        attribute target : LimitRange;
        attribute sigmaLevel : Real default 6.0;
        attribute meanShiftK : Real default 0.0;
        attribute iterations : Integer default 10000;
        attribute seed : Integer[0..1];
    }
}
