// Simulation example: covers constraints, calculations, state machines, and actions.

// -- Constraints --

constraint def SpeedLimit {
    in speed : Real;
    speed <= 120
}

constraint def TemperatureRange {
    in temp : Real;
    temp >= -40 and temp <= 85
}

// -- Calculations --

calc def KineticEnergy {
    in mass : Real;
    in velocity : Real;
    return energy : Real;
    0.5 * mass * velocity ** 2
}

calc def BrakingDistance {
    in speed : Real;
    in friction : Real;
    return distance : Real;
    speed ** 2 / (2 * friction * 9.81)
}

// -- State Machines --

state def TrafficLight {
    entry; then red;

    state red;
    state yellow;
    state green;

    transition first red accept next then green;
    transition first green accept next then yellow;
    transition first yellow accept next then red;
}

state def DoorController {
    entry; then closed;

    state closed;
    state opening;
    state open;
    state closing;

    transition first closed accept openCmd then opening;
    transition first opening then open;
    transition first open accept closeCmd then closing;
    transition first closing then closed;
}

// -- Actions --

action def ProcessOrder {
    action validate;
    action checkInventory;
    action ship;
    action notifyCustomer;

    first validate then checkInventory;
    first checkInventory then ship;
    first ship then notifyCustomer;
}

action def ManufacturePart {
    action cut;
    action weld;
    action inspect;
    action paint;

    first cut then weld;
    first weld then inspect;
    first inspect then paint;
}
