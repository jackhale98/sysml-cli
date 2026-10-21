package DeliveryContext {
    doc
    /*
     * The world around the drone: who uses it, who is affected by it, and what it talks to.
     * A good place to start reading the model.
     */

    private import DroneInterfaces::*;
    private import DeliveryMission::DeliverParcel;

    // ---- People and organisations: the stakeholders and actors ----

    part def Operator { doc /* The remote pilot who plans and supervises delivery flights at the depot. */ }
    part def Recipient { doc /* The customer who receives the parcel. */ }
    part def AviationAuthority { doc /* The authority that approves delivery flights. */ }
    part def Resident { doc /* People living along the delivery routes. */ }

    // ---- External systems ----

    part def GroundStation {
        doc /* Laptop with a radio modem at the depot, used to plan flights and watch the drone. */
        port commandLink : CommandLinkPort;
    }

    // ---- The delivery service ----

    part deliveryService {
        doc /* One depot running delivery flights. */

        part operator : Operator;
        part groundStation : GroundStation;
        part drone :> DroneArchitecture::drone;
        part recipient : Recipient;

        connect operator to groundStation;
        interface radioLink : CommandLink connect groundStation.commandLink to drone.commandLink;

        use case deliverParcel : DeliverParcel {
            subject :>> drone = deliveryService.drone;
            actor :>> operator = deliveryService.operator;
            actor :>> recipient = deliveryService.recipient;
        }
    }
}
