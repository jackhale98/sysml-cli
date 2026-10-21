package RequirementSatisfaction {
    doc
    /*
     * Which part of the design is responsible for meeting each system requirement.
     * `satisfy R by X` claims that X meets R; a verification case then proves it.
     * A requirement that appears nowhere here is a gap in the design.
     */

    private import SystemRequirements::*;
    private import DroneArchitecture::drone;

    satisfy 'Flight endurance' by drone;
    satisfy 'Payload capacity' by drone.payloadRelease;
    satisfy 'Maximum take-off mass' by drone;
    satisfy 'Wind tolerance' by drone.flightController;
    satisfy 'Geofencing' by drone.flightController;
    satisfy 'Return to home' by drone.flightController;
    satisfy 'Battery protection' by drone.battery;
    satisfy 'Command and telemetry link' by drone.radio;
    satisfy 'Controlled parcel release' by drone.payloadRelease;

    // Open gaps: nothing in the design answers 'Ingress protection' (SYS-005)
    // or 'Acoustic noise' (SYS-006) yet.
}
