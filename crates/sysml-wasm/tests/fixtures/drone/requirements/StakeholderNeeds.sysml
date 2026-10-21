package StakeholderNeeds {
    doc
    /*
     * What the people around the delivery drone need from it, in their own words.
     * A stakeholder need is written as a requirement: it has an ID, a statement
     * and the stakeholder who asked for it. The system requirements are derived
     * from these needs (see SystemRequirements).
     */

    private import EngineeringBasics::*;
    private import ModelingMetadata::*;
    private import DeliveryContext::*;

    requirement <'STK-001'> 'Parcel delivery' : EngineeringRequirement {
        doc /* The operator needs to deliver parcels of up to 1 kg to recipients
             * within 3 km of the depot, and get the drone back after each delivery. */
        stakeholder operator : Operator;
        :>> kind = RequirementKind::functional;
        :>> priority = Priority::high;
        :>> verificationMethod = VerificationMethod::demonstration;
        @StatusInfo { status = StatusKind::closed; owner = "M. Okafor"; originator = "Operations"; }
    }

    requirement <'STK-002'> 'Safe operation' : EngineeringRequirement {
        doc /* The aviation authority needs the drone not to endanger people on the ground,
             * other airspace users or property, including when something fails in flight. */
        stakeholder authority : AviationAuthority;
        :>> kind = RequirementKind::safety;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::analysis, VerificationMethod::test);
        @StatusInfo { status = StatusKind::open; owner = "S. Lindqvist"; originator = "Safety Office"; }
        @Rationale { text = "Flights over populated areas are only approved when the remaining risk is acceptable to the authority."; }
    }

    requirement <'STK-003'> 'Flying in typical weather' : EngineeringRequirement {
        doc /* The operator needs to fly deliveries in light rain and moderate wind,
             * so that most scheduled delivery days are flyable. */
        stakeholder operator : Operator;
        :>> kind = RequirementKind::environmental;
        :>> priority = Priority::medium;
        :>> verificationMethod = VerificationMethod::test;
        @StatusInfo { status = StatusKind::tbr; owner = "M. Okafor"; originator = "Operations"; }
        @Issue { text = "The target share of flyable days has not yet been agreed with Operations."; }
    }

    requirement <'STK-004'> 'Approved for operation' : EngineeringRequirement {
        doc /* The operator needs the drone to meet the product rules for its class of
             * unmanned aircraft, so that delivery flights can be approved. */
        stakeholder operator : Operator;
        stakeholder authority : AviationAuthority;
        :>> kind = RequirementKind::regulatory;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::inspection, VerificationMethod::analysis);
        @StatusInfo { status = StatusKind::done; owner = "A. Brandt"; originator = "Compliance"; }
    }

    requirement <'STK-005'> 'Quiet neighbour' : EngineeringRequirement {
        doc /* Residents along the delivery routes need the drone not to be a noise nuisance. */
        stakeholder residents : Resident;
        :>> kind = RequirementKind::environmental;
        :>> priority = Priority::low;
        :>> verificationMethod = VerificationMethod::test;
        @StatusInfo { status = StatusKind::open; owner = "A. Brandt"; originator = "Community Relations"; }
        @Rationale { text = "Community acceptance is a condition of the pilot-site agreement."; }
    }

    requirement <'STK-006'> 'Safe hand-over' : EngineeringRequirement {
        doc /* The recipient needs the parcel to arrive undamaged at the agreed drop point,
             * without having to go near the drone. */
        stakeholder recipient : Recipient;
        :>> kind = RequirementKind::functional;
        :>> priority = Priority::high;
        :>> verificationMethod = VerificationMethod::demonstration;
        @StatusInfo { status = StatusKind::open; owner = "K. Novak"; originator = "Customer Service"; }
    }
}
