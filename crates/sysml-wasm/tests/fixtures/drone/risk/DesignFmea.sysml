package DroneFmea {
    doc
    /*
     * Design FMEA (failure mode and effects analysis) of the delivery drone, using the
     * RiskAnalysis and HazardAnalysis libraries in libraries/.
     *   1. Hazards and the harm they can cause, linked by Causation.
     *   2. Risk controls: requirements saying what the design must do about each hazard.
     *   3. Mitigations: the work that implements a control (`satisfy`); a verification
     *      case proves it (`verify`, see DroneVerification).
     *   4. One @Fmea line per failure mode, attached to the component that fails.
     * The risk priority number (RPN = severity x likelihood x detection) is computed, never typed in.
     */

    private import DroneArchitecture::drone;
    private import HazardAnalysis::*;
    private import RiskAnalysis::*;

    // ---- 1. Hazards and harms ----

    occurrence batteryFire : Hazard {
        :>> description = "Fire or explosion of the battery pack, in flight or on the ground.";
    }
    occurrence uncontrolledDescent : Hazard {
        :>> description = "Loss of thrust or control, so that the drone falls.";
    }
    occurrence flyAway : Hazard {
        :>> description = "Loss of position or command link, so that the drone leaves its approved area.";
    }

    occurrence burnInjury : Harm {
        :>> description = "Burns to people and fire damage to property.";
        :>> severity = SeverityScale::critical;
    }
    occurrence impactInjury : Harm {
        :>> description = "A person on the ground is hit by the falling drone.";
        :>> severity = SeverityScale::critical;
    }
    occurrence thirdPartyDamage : Harm {
        :>> description = "Collision with property or other airspace users outside the approved area.";
        :>> severity = SeverityScale::serious;
    }

    connection : Causation connect batteryFire to burnInjury;
    connection : Causation connect uncontrolledDescent to impactInjury;
    connection : Causation connect flyAway to thirdPartyDamage;

    // ---- 2. Risk controls ----

    requirement <'RC-001'> 'Battery thermal protection' : RiskControl {
        doc /* The battery management system shall disconnect the pack and alert the operator
             * before any cell reaches its thermal limit. */
        :>> hierarchy = ControlHierarchy::protectiveMeasure;
        :>> rationale = "Stops a developing thermal event before it spreads to neighbouring cells.";
        ref :>> hazard = batteryFire;
    }

    requirement <'RC-002'> 'Rotor health monitoring' : RiskControl {
        doc /* The flight controller shall monitor the speed and current of every rotor and
             * land at once when one does not follow its command. */
        :>> hierarchy = ControlHierarchy::protectiveMeasure;
        :>> rationale = "A quadcopter cannot stay up on three rotors, so early detection limits the fall.";
        ref :>> hazard = uncontrolledDescent;
    }

    requirement <'RC-003'> 'Navigation integrity monitoring' : RiskControl {
        doc /* The flight controller shall compare the satellite position with its inertial
             * estimate and return home when they differ by more than 10 m. */
        :>> hierarchy = ControlHierarchy::protectiveMeasure;
        :>> rationale = "Detects jamming and spoofing, which the receiver cannot always detect itself.";
        ref :>> hazard = flyAway;
    }

    // ---- 3. Mitigations ----

    action batteryManagementSystem : Mitigation {
        doc /* Integrate a battery management system with cell monitoring and a power cut-off. */
        :>> kind = MitigationKind::prevention;
        :>> status = MitigationStatus::completed;
        :>> owner = "L. Moreau";
    }
    action rotorHealthMonitor : Mitigation {
        doc /* Add rotor speed and current checks to the flight controller software. */
        :>> kind = MitigationKind::detection;
        :>> status = MitigationStatus::inProgress;
        :>> owner = "R. Tanaka";
    }
    action navigationIntegrityMonitor : Mitigation {
        doc /* Add a satellite-versus-inertial consistency check to the navigation filter. */
        :>> kind = MitigationKind::detection;
        :>> status = MitigationStatus::proposed;
        :>> owner = "R. Tanaka";
    }

    satisfy 'Battery thermal protection' by batteryManagementSystem;
    satisfy 'Rotor health monitoring' by rotorHealthMonitor;
    satisfy 'Navigation integrity monitoring' by navigationIntegrityMonitor;

    // ---- 4. FMEA worksheet lines ----

    @Fmea about drone.battery {
        failureMode = "Thermal runaway";
        cause = "Internal cell short, overcharge or crash damage";
        effect = "Fire in the airframe; loss of the drone; burns to bystanders";
        category = RiskCategory::design;
        severity = 8; likelihood = 3; detection = 6;
        initialSeverity = 8; initialLikelihood = 5; initialDetection = 8;
        hazard = batteryFire;
    }

    @Fmea about drone.rotors.motor {
        failureMode = "Motor stops or loses torque";
        cause = "Bearing seizure, winding short or loose connector";
        effect = "One rotor loses thrust; the drone cannot hold altitude and falls";
        category = RiskCategory::design;
        severity = 8; likelihood = 4; detection = 3;
        hazard = uncontrolledDescent;
    }

    @Fmea about drone.rotors.propeller {
        failureMode = "Blade breaks or comes off";
        cause = "Hitting an obstacle, fatigue, or a badly tightened hub";
        effect = "Thrust imbalance and vibration; the drone falls";
        category = RiskCategory::design;
        severity = 8; likelihood = 3; detection = 4;
        hazard = uncontrolledDescent;
    }

    @Fmea about drone.flightController {
        failureMode = "Software lock-up";
        cause = "Unhandled fault or timing overrun in the control loop";
        effect = "Loss of attitude control; the drone falls";
        category = RiskCategory::software;
        severity = 8; likelihood = 2; detection = 5;
        hazard = uncontrolledDescent;
    }

    @Fmea about drone.gnss {
        failureMode = "Satellite position lost or wrong";
        cause = "Signal blocked by buildings, jamming or spoofing";
        effect = "Position drifts; the drone may leave the approved area";
        category = RiskCategory::design;
        severity = 6; likelihood = 4; detection = 3;
        hazard = flyAway;
    }

    @Fmea about drone.radio {
        failureMode = "Command link lost";
        cause = "Out of range, interference or a damaged antenna";
        effect = "The operator cannot command the drone; it relies on return-to-home";
        category = RiskCategory::design;
        severity = 6; likelihood = 5; detection = 2;
        hazard = flyAway;
    }
}
