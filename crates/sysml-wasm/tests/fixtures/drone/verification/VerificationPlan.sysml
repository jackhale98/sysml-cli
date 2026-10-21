package DroneVerification {
    doc
    /*
     * How we will prove that the design meets its requirements. Each verification case
     * names its subject (the drone under test), the requirements it verifies (its
     * objective) and the method: test, analysis, inspection or demonstration.
     */

    private import ISQ::*;
    private import SI::*;
    private import VerificationCases::*;
    private import SystemRequirements::*;
    private import DroneFmea::*;
    private import DroneArchitecture::drone;

    verification <'VC-001'> enduranceFlightTest {
        doc /* Hover with a 1 kg payload in calm air until the battery reaches its landing
             * reserve, and time the flight. Passes at 25 minutes or more. */
        subject testDrone :> drone;
        objective { verify 'Flight endurance'; }
        @VerificationMethod { kind = VerificationMethodKind::test; }

        action hoverUntilReserve {
            out attribute flightTime :> duration;
        }
        return :>> verdict = PassIf(hoverUntilReserve.flightTime >= 25 [min]);
    }

    verification <'VC-002'> massAndPayloadCheck {
        doc /* Weigh the drone with the battery and the 1 kg reference parcel loaded, and check
             * the parcel box fits the payload bay. Supported by the mass budget analysis. */
        subject testDrone :> drone;
        objective {
            verify 'Payload capacity';
            verify 'Maximum take-off mass';
        }
        @VerificationMethod { kind = (VerificationMethodKind::test, VerificationMethodKind::inspect, VerificationMethodKind::analyze); }
    }

    verification <'VC-003'> windHoldTest {
        doc /* Hold position for 5 minutes in a 10 m/s wind with 14 m/s gusts and compare the
             * logged position error with the 2 m limit. */
        subject testDrone :> drone;
        objective { verify 'Wind tolerance'; }
        @VerificationMethod { kind = (VerificationMethodKind::test, VerificationMethodKind::analyze); }
    }

    verification <'VC-004'> failsafeFlightTest {
        doc /* Fly a route that crosses the geofence, then switch off the ground station radio.
             * The drone must stop at the boundary and, 3 s after losing the link, fly home and land. */
        subject testDrone :> drone;
        objective {
            verify 'Geofencing';
            verify 'Return to home';
        }
        @VerificationMethod { kind = VerificationMethodKind::test; }
    }

    verification <'VC-005'> batteryAbuseTest {
        doc /* On a test bench, apply over-current, over-temperature and deep discharge to the
             * battery and check that it disconnects within 1 s every time. */
        subject testDrone :> drone;
        objective {
            verify 'Battery protection';
            verify 'Battery thermal protection';
        }
        @VerificationMethod { kind = VerificationMethodKind::test; }
    }

    verification <'VC-006'> linkRangeTest {
        doc /* Measure signal strength and round-trip delay between the ground station and the
             * drone over a 5 km line-of-sight path. */
        subject testDrone :> drone;
        objective { verify 'Command and telemetry link'; }
        @VerificationMethod { kind = VerificationMethodKind::test; }
    }

    // Not yet planned: 'Ingress protection', 'Acoustic noise', 'Controlled parcel release',
    // and the risk controls 'Rotor health monitoring' and 'Navigation integrity monitoring'.
}
