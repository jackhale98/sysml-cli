package DronePerformance {
    doc
    /*
     * Engineering calculations with units. A `calc def` is a reusable formula; an
     * `analysis` applies formulas to a subject (here, our drone) to answer a question.
     * Units are checked: watt-hours divided by watts gives a duration.
     */

    private import ISQ::*;
    private import SI::*;
    private import DroneArchitecture::drone;

    calc def HoverEndurance {
        doc /* How long the battery lasts in hover: usable energy divided by the power needed
             * to hover. Hover is the worst case for a multicopter, so this is a safe estimate. */
        in usableEnergy :> energy;
        in hoverPower :> power;
        return endurance :> duration = usableEnergy / hoverPower;
    }

    analysis massBudget {
        doc /* Adds up the component masses and compares the take-off mass with its limit.
             * Expected result: 2.77 kg empty + 1 kg payload = 3.77 kg, a margin of 0.23 kg. */
        subject estimate = drone;
        objective {
            doc /* Show that 'Maximum take-off mass' (SYS-003) is met, and how much margin is left. */
        }
        attribute limit :> mass = 4 [kg];
        return margin :> mass = limit - estimate.takeOffMass;
    }

    analysis enduranceBudget {
        doc /* Estimates the endurance before the first flight test.
             * Expected result: 216 W*h x 0.85 / 400 W = 27.5 min, a margin of 2.5 min. */
        subject estimate = drone;
        objective {
            doc /* Show that 'Flight endurance' (SYS-001) can be met, and how much margin is left. */
        }
        attribute required :> duration = 25 [min];
        return margin :> duration = estimate.endurance - required;
    }
}
