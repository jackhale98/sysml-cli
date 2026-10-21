package DroneComponents {
    doc
    /*
     * The component catalogue. Each `part def` is a definition: it says what a kind of
     * component is - its properties and its ports - like a datasheet. A definition is not
     * part of any drone yet; DroneArchitecture uses these definitions to build one.
     */

    private import ScalarValues::Real;
    private import ISQ::*;
    private import SI::*;
    private import DroneInterfaces::*;
    private import FlightControl::FlightModes;

    abstract part def Component {
        doc /* Anything built into the drone. Every component has a mass, so the mass budget can add them up. */
        attribute mass :> ISQ::mass;
    }

    // Each definition below specializes Component (:>) and gives its mass a value (:>> redefines it).

    part def Airframe :> Component {
        doc /* Carbon-fibre body with four folding arms and the landing gear. */
        attribute :>> mass = 650 [g];
        attribute diagonal :> length = 650 [mm] { doc /* Motor-to-motor distance across the frame. */ }
    }

    part def Battery :> Component {
        doc /* Six-cell lithium-ion pack with a built-in battery management system. */
        attribute :>> mass = 1100 [g];
        attribute nominalVoltage :> voltage = 21.6 [V];
        attribute capacity :> energy = 216 [W*h];
        attribute usableFraction : Real = 0.85 { doc /* Share of the capacity used in flight; the rest is the landing reserve. */ }
        attribute usableEnergy :> energy = capacity * usableFraction;
        port powerOut : PowerPort;
    }

    part def PowerDistributionBoard :> Component {
        doc /* Splits battery power between the rotors and the avionics. */
        attribute :>> mass = 40 [g];
        port powerIn : ~PowerPort;
        port rotorPower : PowerPort;
        port avionicsPower : PowerPort;
    }

    part def Rotor :> Component {
        doc /* One propulsion unit: speed controller, motor and propeller, replaced as one unit.
             * Its mass is the sum of its own parts. */
        attribute :>> mass = esc.mass + motor.mass + propeller.mass;
        port powerIn : ~PowerPort;
        port control : ~MotorControlPort;

        part esc : SpeedController;
        part motor : Motor;
        part propeller : Propeller;

        bind powerIn = esc.powerIn;     // the rotor's ports are the speed controller's ports
        bind control = esc.control;
        connect esc to motor;           // three motor phase wires
        connect motor to propeller;     // shaft and hub
    }

    part def SpeedController :> Component {
        doc /* Turns battery power into motor current at the commanded speed. */
        attribute :>> mass = 35 [g];
        port powerIn : ~PowerPort;
        port control : ~MotorControlPort;
    }

    part def Motor :> Component {
        doc /* Brushless outrunner motor. */
        attribute :>> mass = 120 [g];
        attribute maxPower :> power = 450 [W];
    }

    part def Propeller :> Component {
        doc /* Two-blade carbon-fibre propeller with a low-noise tip. */
        attribute :>> mass = 25 [g];
        attribute diameter :> length = 380 [mm];
    }

    part def FlightController :> Component {
        doc /* The drone's computer: keeps it stable, navigates, and decides when to return home
             * or land. It also powers the small avionics (receiver, radio, camera, latch). */
        attribute :>> mass = 40 [g];
        port powerIn : ~PowerPort;
        port motorControl : MotorControlPort;
        port navigation : ~NavigationPort;
        port commandLink : ~CommandLinkPort;
        port imageIn : ~ImagePort;
        port releaseControl : ReleaseControlPort;

        exhibit state flightModes : FlightModes;
    }

    part def GnssReceiver :> Component {
        doc /* Satellite navigation receiver (GPS and Galileo) with a compass. */
        attribute :>> mass = 30 [g];
        port navigation : NavigationPort;
    }

    part def TelemetryRadio :> Component {
        doc /* Radio modem: passes commands from the ground station to the flight controller,
             * and telemetry back. */
        attribute :>> mass = 30 [g];
        attribute range :> distance = 8 [km];
        port airLink : ~CommandLinkPort;        // towards the ground station
        port controllerLink : CommandLinkPort;  // towards the flight controller
    }

    part def Camera :> Component {
        doc /* Downward-looking camera used to check that the drop zone is clear. */
        attribute :>> mass = 60 [g];
        port imageOut : ImagePort;
    }

    part def PayloadRelease :> Component {
        doc /* Motorised latch under the body that holds the parcel and lets it go on command. */
        attribute :>> mass = 100 [g];
        port releaseControl : ~ReleaseControlPort;
        item parcel : Parcel[0..1];  // empty on the way home
    }
}
