package DroneArchitecture {
    doc
    /*
     * Our delivery drone design.
     *   DeliveryDrone (a definition) says what any delivery drone offers: the properties
     *   the requirements talk about and the mission it performs.
     *   drone (a usage of DeliveryDrone) is our actual design: which catalogue components
     *   it is built from, how many of each, and how they are connected.
     */

    private import ISQ::*;
    private import SI::*;
    private import NumericalFunctions::sum;
    private import DroneComponents::*;
    private import DroneInterfaces::*;
    private import DeliveryMission::DeliveryFlight;
    private import DronePerformance::HoverEndurance;

    part def DeliveryDrone {
        doc /* Any drone that can do our delivery job. */
        attribute emptyMass :> mass;
        attribute payloadCapacity :> mass;
        attribute takeOffMass :> mass = emptyMass + payloadCapacity;
        attribute hoverPower :> power;
        attribute endurance :> duration;

        port commandLink : ~CommandLinkPort;
        perform action deliveryFlight : DeliveryFlight;
    }

    part drone : DeliveryDrone {
        doc /* Our design: a quadcopter built from catalogue components. */

        // The parts. [4] is a multiplicity: the drone has four identical rotors.
        part airframe : Airframe;
        part battery : Battery;
        part powerDistribution : PowerDistributionBoard;
        part rotors : Rotor[4];
        part flightController : FlightController;
        part gnss : GnssReceiver;
        part radio : TelemetryRadio;
        part camera : Camera;
        part payloadRelease : PayloadRelease;

        // Power: battery -> distribution board -> rotors and flight controller.
        interface batteryPower : PowerSupply connect battery.powerOut to powerDistribution.powerIn;
        interface rotorPower : PowerSupply connect powerDistribution.rotorPower to rotors.powerIn;
        interface avionicsPower : PowerSupply connect powerDistribution.avionicsPower to flightController.powerIn;

        // Commands and telemetry: the drone's link port is the radio's antenna side.
        bind commandLink = radio.airLink;
        interface radioCable : CommandLink connect radio.controllerLink to flightController.commandLink;

        // Sensor and actuator signals.
        flow of MotorCommand from flightController.motorControl.setPoint to rotors.control.setPoint;
        flow of PositionFix from gnss.navigation.fix to flightController.navigation.fix;
        flow of Image from camera.imageOut.image to flightController.imageIn.image;
        flow of ReleaseCommand from flightController.releaseControl.release to payloadRelease.releaseControl.release;

        // Design values. The empty mass is a roll-up of the component masses.
        attribute :>> payloadCapacity = 1 [kg];
        attribute :>> emptyMass = airframe.mass + battery.mass + powerDistribution.mass
            + sum(rotors.mass) + flightController.mass + gnss.mass + radio.mass
            + camera.mass + payloadRelease.mass;
        attribute :>> hoverPower = 400 [W] { doc /* Measured on the thrust stand at full take-off mass. */ }
        attribute :>> endurance = HoverEndurance(battery.usableEnergy, hoverPower);
    }
}
