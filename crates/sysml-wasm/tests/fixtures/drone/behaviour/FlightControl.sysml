package FlightControl {
    doc
    /*
     * The flight modes of the drone, as a state machine run by the flight controller.
     * The drone is always in exactly one state. A transition names the state it leaves
     * (`first`), what triggers it (`accept`), an optional guard (`if`), and the state it
     * enters (`then`).
     */

    private import ScalarValues::*;
    private import DroneInterfaces::*;

    // Events the flight controller raises itself; operator commands are in DroneInterfaces.
    item def LinkLost { doc /* No command has arrived from the ground station for 3 s. */ }
    item def FaultDetected { doc /* A rotor, a sensor or the battery reported a fault. */ }
    item def ArrivedHome { doc /* The drone is above the depot landing pad. */ }
    item def TouchDown { doc /* The landing gear has touched the ground. */ }

    state def FlightModes {
        doc /* Flight modes of the delivery drone. */
        in attribute preflightChecksPassed : Boolean;
        in attribute stateOfCharge : Real { doc /* Battery charge left: 0 is empty, 1 is full. */ }
        attribute returnThreshold : Real = 0.3 { doc /* Charge needed to fly home plus a 20 % reserve. */ }

        entry; then idle;

        state idle { doc /* On the ground with the motors off. Safe to handle. */ }
        state armed { doc /* On the ground with the motors idling, ready to take off. */ }
        state flying {
            doc /* Flying the mission under automatic control. */
            do action followFlightPlan;
        }
        state returningHome { doc /* Flying back to the depot on its own. */ }
        state landing { doc /* Descending onto the landing pad. */ }
        state failsafe {
            doc /* Something is wrong: land at once, where the drone is. */
            entry action alertOperator;
        }

        transition arm first idle accept ArmCommand if preflightChecksPassed then armed;
        transition disarm first armed accept DisarmCommand then idle;
        transition takeOff first armed accept TakeOffCommand then flying;

        transition commandedReturn first flying accept ReturnHomeCommand then returningHome;
        transition linkLost first flying accept LinkLost then returningHome;
        transition lowBattery first flying accept when stateOfCharge < returnThreshold then returningHome;
        transition overDepot first returningHome accept ArrivedHome then landing;
        transition landed first landing accept TouchDown then idle;

        transition faultInFlight first flying accept FaultDetected then failsafe;
        transition faultOnReturn first returningHome accept FaultDetected then failsafe;
        transition emergencyLanded first failsafe accept TouchDown then idle;
    }
}
