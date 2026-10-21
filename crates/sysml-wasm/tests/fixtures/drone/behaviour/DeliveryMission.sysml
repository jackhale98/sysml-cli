package DeliveryMission {
    doc
    /*
     * What the drone is for: the parcel delivery use case, and the delivery flight
     * that carries it out step by step.
     */

    private import ScalarValues::Boolean;
    private import DroneInterfaces::*;
    private import DroneArchitecture::DeliveryDrone;
    private import DeliveryContext::*;

    use case def DeliverParcel {
        doc /* The operator sends a parcel to a recipient by drone. */
        subject drone : DeliveryDrone;
        actor operator : Operator;
        actor recipient : Recipient;
        objective {
            doc /* The recipient has the parcel, undamaged, and the drone is back at the depot
                 * ready for its next flight. */
        }

        first start;
        then action prepareFlight {
            doc /* The operator loads the parcel and uploads the flight plan. */
        }
        then perform drone.deliveryFlight;
        then done;
    }

    action def DeliveryFlight {
        doc
        /*
         * One delivery flight, from take-off at the depot to landing back there.
         * Read it top to bottom: `first start` is where it begins and each `then` names
         * the next step. A `fork` starts steps that run at the same time and the `join`
         * waits for all of them; a `decide` takes exactly one of its branches and the
         * `merge` brings the branches back together.
         */
        in item flightPlan : FlightPlan;

        first start;
        then action takeOff { doc /* Climb vertically to cruise altitude. */ }
        then action flyToDropPoint {
            doc /* Follow the flight plan to the recipient's drop point. */
            in item plan : FlightPlan = flightPlan;
        }
        then fork;
            then descend;
            then checkDropZone;

        action descend { doc /* Descend to 3 m above the drop point and hover. */ }
        then atDropPoint;

        action checkDropZone {
            doc /* Look down with the camera to check that nobody is in the drop zone. */
            out attribute isClear : Boolean;
        }
        then atDropPoint;

        join atDropPoint;
        then decide;
            if checkDropZone.isClear then releaseParcel;
            else keepParcel;

        action releaseParcel { doc /* Open the latch and let the parcel down. */ }
        then readyToReturn;

        action keepParcel { doc /* Do not drop: keep the parcel on board; the operator reschedules the delivery. */ }
        then readyToReturn;

        merge readyToReturn;
        then action returnHome { doc /* Climb to cruise altitude and fly back to the depot. */ }
        then action land { doc /* Descend onto the landing pad and stop the motors. */ }
        then done;
    }
}
