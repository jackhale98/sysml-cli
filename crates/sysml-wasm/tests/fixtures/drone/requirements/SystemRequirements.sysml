package SystemRequirements {
    doc
    /*
     * What the drone shall do and how well, derived from the stakeholder needs.
     * Each requirement has an ID (the short name in <'...'>), a title, a statement
     * (its doc text) and classifying attributes. Where a requirement can be checked
     * by a number, a `require constraint` states it formally, with units.
     */

    private import SI::*;
    private import EngineeringBasics::*;
    private import ModelingMetadata::*;
    private import RequirementDerivation::*;
    private import DroneArchitecture::DeliveryDrone;

    requirement def DroneRequirement :> EngineeringRequirement {
        doc /* A requirement on the delivery drone. Specializing EngineeringRequirement adds a
             * subject: every requirement of this kind is about a DeliveryDrone. */
        subject drone : DeliveryDrone;
    }

    // ---- Performance ----

    requirement <'SYS-001'> 'Flight endurance' : DroneRequirement {
        doc /* The drone shall fly for at least 25 minutes with a 1 kg payload in still air
             * before it reaches its landing reserve. */
        :>> kind = RequirementKind::performance;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::analysis, VerificationMethod::test);
        @StatusInfo { status = StatusKind::done; owner = "J. Hale"; }
        @Rationale { text = "A 3 km delivery and return takes about 10 minutes; the rest covers hover at the drop point, headwind and battery ageing."; }
        require constraint { drone.endurance >= 25 [min] }
    }

    requirement <'SYS-002'> 'Payload capacity' : DroneRequirement {
        doc /* The drone shall carry a parcel of up to 1 kg in a box of 250 mm x 200 mm x 150 mm. */
        :>> kind = RequirementKind::performance;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::test, VerificationMethod::inspection);
        @StatusInfo { status = StatusKind::closed; owner = "J. Hale"; }
        require constraint { drone.payloadCapacity >= 1 [kg] }
    }

    requirement <'SYS-003'> 'Maximum take-off mass' : DroneRequirement {
        doc /* The take-off mass, including battery and payload, shall not exceed 4 kg. */
        :>> kind = RequirementKind::regulatory;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::analysis, VerificationMethod::test);
        @StatusInfo { status = StatusKind::done; owner = "J. Hale"; }
        @Rationale { text = "4 kg is the upper mass limit of the drone class the operator is approved for."; }
        require constraint { drone.takeOffMass <= 4 [kg] }
    }

    // ---- Environment ----

    requirement <'SYS-004'> 'Wind tolerance' : DroneRequirement {
        doc /* The drone shall hold its position to within 2 m in a steady wind of 10 m/s
             * with gusts of up to 14 m/s. */
        :>> kind = RequirementKind::environmental;
        :>> priority = Priority::medium;
        :>> verificationMethod = (VerificationMethod::analysis, VerificationMethod::test);
        @StatusInfo { status = StatusKind::open; owner = "R. Tanaka"; }
    }

    requirement <'SYS-005'> 'Ingress protection' : DroneRequirement {
        doc /* The drone shall be protected against dust and splashing water to at least IP54 (IEC 60529). */
        :>> kind = RequirementKind::environmental;
        :>> priority = Priority::medium;
        :>> verificationMethod = VerificationMethod::test;
        @StatusInfo { status = StatusKind::tbc; owner = "L. Moreau"; }
        @Issue { text = "The motor cooling openings may not allow IP54. Nobody owns this in the design yet."; }
    }

    requirement <'SYS-006'> 'Acoustic noise' : DroneRequirement {
        doc /* The sound pressure level in hover shall not exceed 70 dB(A) at a distance of 10 m. */
        :>> kind = RequirementKind::environmental;
        :>> priority = Priority::low;
        :>> verificationMethod = VerificationMethod::test;
        @StatusInfo { status = StatusKind::tbd; owner = "A. Brandt"; }
        @Issue { text = "First estimate; the pilot-site agreement may ask for a lower limit."; }
    }

    // ---- Safety ----

    requirement <'SYS-007'> 'Geofencing' : DroneRequirement {
        doc /* The drone shall not leave the operator-defined geofence, and shall stop at the
             * boundary when its route would cross it. */
        :>> kind = RequirementKind::safety;
        :>> priority = Priority::high;
        :>> verificationMethod = VerificationMethod::test;
        @StatusInfo { status = StatusKind::done; owner = "R. Tanaka"; }
    }

    requirement <'SYS-008'> 'Return to home' : DroneRequirement {
        doc /* The drone shall return to the depot and land on its own when the command link
             * is lost for more than 3 s, or when the battery only has enough energy left
             * to fly home plus a 20 % reserve. */
        :>> kind = RequirementKind::safety;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::test, VerificationMethod::demonstration);
        @StatusInfo { status = StatusKind::tbr; owner = "R. Tanaka"; }
        @Rationale { text = "A lost link must never leave the drone flying without a defined outcome."; }
    }

    requirement <'SYS-009'> 'Battery protection' : DroneRequirement {
        doc /* The battery shall disconnect its output within 1 s of an over-current,
             * over-temperature or deep-discharge condition. */
        :>> kind = RequirementKind::safety;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::test, VerificationMethod::analysis);
        @StatusInfo { status = StatusKind::open; owner = "L. Moreau"; }
        @Rationale { text = "Battery fire is the most severe hazard of the design; see the design FMEA."; }
    }

    // ---- Functions and interfaces ----

    requirement <'SYS-010'> 'Command and telemetry link' : DroneRequirement {
        doc /* The drone shall exchange commands and telemetry with the ground station up to
             * 5 km away in line of sight, with a round-trip delay below 200 ms. */
        :>> kind = RequirementKind::interface;
        :>> priority = Priority::high;
        :>> verificationMethod = VerificationMethod::test;
        @StatusInfo { status = StatusKind::done; owner = "K. Novak"; }
    }

    requirement <'SYS-011'> 'Controlled parcel release' : DroneRequirement {
        doc /* The drone shall release the parcel only when it is less than 5 m above the
             * drop point, moving slower than 1 m/s, and the drop zone is clear of people. */
        :>> kind = RequirementKind::functional;
        :>> priority = Priority::high;
        :>> verificationMethod = VerificationMethod::demonstration;
        @StatusInfo { status = StatusKind::open; owner = "K. Novak"; }
    }

    // ---- Derivation: which need each requirement comes from ----

    #derivation connection {
        end #original ::> StakeholderNeeds::'Parcel delivery';
        end #derive ::> 'Flight endurance';
        end #derive ::> 'Payload capacity';
        end #derive ::> 'Command and telemetry link';
    }

    #derivation connection {
        end #original ::> StakeholderNeeds::'Safe operation';
        end #derive ::> 'Geofencing';
        end #derive ::> 'Return to home';
        end #derive ::> 'Battery protection';
    }

    #derivation connection {
        end #original ::> StakeholderNeeds::'Flying in typical weather';
        end #derive ::> 'Wind tolerance';
        end #derive ::> 'Ingress protection';
    }

    #derivation connection {
        end #original ::> StakeholderNeeds::'Approved for operation';
        end #derive ::> 'Maximum take-off mass';
    }

    #derivation connection {
        end #original ::> StakeholderNeeds::'Quiet neighbour';
        end #derive ::> 'Acoustic noise';
    }

    #derivation connection {
        end #original ::> StakeholderNeeds::'Safe hand-over';
        end #derive ::> 'Controlled parcel release';
    }
}
