package FunctionAllocation {
    doc
    /*
     * Which component carries out each step of the delivery flight. The drone as a whole
     * performs the flight (see DeliveryDrone); `allocate` records which part is responsible
     * for each step, without changing either the flight or the parts. The Action Flow
     * view can draw these as swimlanes.
     */

    private import DroneArchitecture::drone;

    allocate drone.deliveryFlight.takeOff to drone.flightController;
    allocate drone.deliveryFlight.flyToDropPoint to drone.flightController;
    allocate drone.deliveryFlight.descend to drone.flightController;
    allocate drone.deliveryFlight.checkDropZone to drone.camera;
    allocate drone.deliveryFlight.releaseParcel to drone.payloadRelease;
    allocate drone.deliveryFlight.keepParcel to drone.flightController;
    allocate drone.deliveryFlight.returnHome to drone.flightController;
    allocate drone.deliveryFlight.land to drone.flightController;
}
