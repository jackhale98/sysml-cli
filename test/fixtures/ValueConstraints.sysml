package ValueConstraintsFixture {
    private import ScalarValues::*;
    doc /* W017 fixture: assert constraints on types, violated by values. */

    metadata def Fmea {
        attribute severity : FmeaRating;
        attribute 'occurrence' : FmeaRating;
    }

    attribute def FmeaRating {
        assert constraint inRange { that >= 1 and that <= 10 }
    }

    attribute def LimitRange {
        attribute nominal : Real;
        attribute lower : Real;
        attribute upper : Real;
        assert constraint wellOrdered { lower <= nominal and nominal <= upper }
    }

    part def Battery;
    part battery : Battery {
        @Fmea { severity = 12; 'occurrence' = 3; }
    }

    part def Widget {
        attribute rating : FmeaRating = 0;
        attribute target : LimitRange {
            :>> nominal = 5.0;
            :>> lower = 6.0;
            :>> upper = 8.0;
        }
        attribute okTarget : LimitRange {
            :>> nominal = 7.0;
            :>> lower = 6.0;
            :>> upper = 8.0;
        }
    }
}
