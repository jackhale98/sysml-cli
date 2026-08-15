package UncertaintyStackupFixture {
    doc /* Self-contained uncertainty-analysis fixture: minimal copies of
         * the Uncertainty/Tolerancing domain-library defs plus the
         * enclosure-gap chain (worst case 2.67..3.28, RSS Cp 2.79). */

    private import ScalarValues::*;

    enum def Distribution { enum normal; enum uniform; enum triangular; }
    attribute def UncertainValue {
        attribute nominal : Real;
        attribute plus : Real;
        attribute minus : Real;
        attribute distribution : Distribution default Distribution::normal;
    }
    attribute def LimitRange {
        attribute nominal : Real;
        attribute lower : Real;
        attribute upper : Real;
    }
    analysis def UncertaintyAnalysis {
        attribute target : LimitRange;
        attribute sigmaLevel : Real default 6.0;
        attribute meanShiftK : Real default 0.0;
        attribute iterations : Integer default 10000;
        attribute seed : Integer[0..1];
    }
    attribute def TolerancedDimension :> UncertainValue {
        attribute unit : String default "mm";
    }
    enum def Sense { enum positive; enum negative; }
    attribute def Contribution {
        attribute dim : TolerancedDimension;
        attribute sense : Sense default Sense::positive;
        attribute source : String[0..1];
    }
    analysis def ToleranceStackup :> UncertaintyAnalysis {
        attribute contributions : Contribution[1..*] ordered;
        attribute critical : Boolean default false;
    }

    part def Housing {
        attribute depth : TolerancedDimension {
            :>> nominal = 50.0;
            :>> plus = 0.1;
            :>> minus = 0.1;
        }
    }
    part def Cover {
        attribute height : TolerancedDimension {
            :>> nominal = 45.0;
            :>> plus = 0.08;
            :>> minus = 0.08;
        }
    }
    part def Gasket {
        attribute thickness : TolerancedDimension {
            :>> nominal = 2.0;
            :>> plus = 0.15;
            :>> minus = 0.10;
            :>> distribution = Distribution::uniform;
        }
    }

    part def Enclosure {
        part housing : Housing;
        part cover : Cover;
        part gasket : Gasket;

        analysis gapAnalysis : ToleranceStackup {
            attribute :>> target {
                :>> nominal = 3.0;
                :>> lower = 2.5;
                :>> upper = 3.5;
            }
            attribute :>> critical = true;
            attribute housingDepth :> contributions {
                :>> dim = housing.depth;
                :>> sense = Sense::positive;
                :>> source = "DWG-001 Rev A";
            }
            attribute coverHeight :> contributions {
                :>> dim = cover.height;
                :>> sense = Sense::negative;
            }
            attribute gasketThickness :> contributions {
                :>> dim = gasket.thickness;
                :>> sense = Sense::negative;
            }
        }

        analysis tightGap : ToleranceStackup {
            doc /* Same chain against limits the worst case cannot meet. */
            attribute :>> target {
                :>> nominal = 3.0;
                :>> lower = 2.9;
                :>> upper = 3.1;
            }
            attribute housingDepth :> contributions {
                :>> dim = housing.depth;
            }
            attribute coverHeight :> contributions {
                :>> dim = cover.height;
                :>> sense = Sense::negative;
            }
            attribute gasketThickness :> contributions {
                :>> dim = gasket.thickness;
                :>> sense = Sense::negative;
            }
        }
    }
}
