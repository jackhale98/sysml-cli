package DroneInterfaces {
    doc
    /*
     * What flows between the parts of the drone, and the connection points it flows through.
     *   item def      - a kind of thing that flows: energy, commands, data, the parcel itself
     *   port def      - a connection point on a part, saying what goes out and what comes in
     *   interface def - a reusable kind of connection between two matching ports
     * A port typed ~PortName is the conjugate (mirror image) of PortName: what goes
     * out on one side comes in on the other, so the two ends always fit together.
     */

    private import ScalarValues::Real;
    private import ISQ::*;

    // ---- Things that flow ----

    item def ElectricPower {
        doc /* Electrical energy delivered from the battery to the loads. */
        attribute voltage :> ISQ::voltage;
        attribute current :> electricCurrent;
    }

    item def FlightCommand {
        doc /* An instruction from the operator to the drone. Each kind of command below specializes it. */
    }
    item def ArmCommand :> FlightCommand { doc /* Start the motors on the ground, ready for take-off. */ }
    item def DisarmCommand :> FlightCommand { doc /* Stop the motors on the ground. */ }
    item def TakeOffCommand :> FlightCommand { doc /* Climb to cruise altitude and start the flight plan. */ }
    item def ReturnHomeCommand :> FlightCommand { doc /* Stop the mission and fly back to the depot. */ }
    item def FlightPlan :> FlightCommand {
        doc /* The route to the drop point, uploaded before take-off. */
        attribute cruiseAltitude :> length;
        attribute dropPointDistance :> distance;
    }

    item def Telemetry {
        doc /* The status the drone reports to the ground station several times a second. */
        attribute altitude :> length;
        attribute groundSpeed :> speed;
        attribute stateOfCharge : Real { doc /* Battery charge left: 0 is empty, 1 is full. */ }
    }

    item def MotorCommand { doc /* Speed set-point for a rotor. */ }
    item def PositionFix { doc /* Position and velocity from satellite navigation. */ }
    item def Image { doc /* A camera picture of the drop zone. */ }
    item def ReleaseCommand { doc /* Open the payload latch. */ }

    item def Parcel {
        doc /* The goods being delivered, packed in a standard box. */
        attribute mass :> ISQ::mass;
    }

    // ---- Ports ----

    port def PowerPort {
        doc /* Supplies electrical power. A load uses the conjugate ~PowerPort. */
        out item power : ElectricPower;
    }

    port def CommandLinkPort {
        doc /* The commanding side of a command link: commands go out, telemetry comes in.
             * The commanded side (the drone) uses ~CommandLinkPort. */
        out item command : FlightCommand;
        in item telemetry : Telemetry;
    }

    port def MotorControlPort { out item setPoint : MotorCommand; }
    port def NavigationPort { out item fix : PositionFix; }
    port def ImagePort { out item image : Image; }
    port def ReleaseControlPort { out item release : ReleaseCommand; }

    // ---- Interfaces ----

    interface def PowerSupply {
        doc /* A power cable: power flows from the supply to the load. */
        end supply : PowerPort;
        end load : ~PowerPort;
        flow of ElectricPower from supply.power to load.power;
    }

    interface def CommandLink {
        doc /* Commands one way, telemetry the other. Used both for the radio link between
             * ground station and drone, and for the cable from radio modem to flight controller. */
        end commander : CommandLinkPort;
        end commanded : ~CommandLinkPort;
        flow of FlightCommand from commander.command to commanded.command;
        flow of Telemetry from commanded.telemetry to commander.telemetry;
    }
}
