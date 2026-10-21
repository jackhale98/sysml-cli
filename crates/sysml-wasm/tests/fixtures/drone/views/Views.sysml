package DroneViews {
    doc
    /*
     * The saved diagrams and tables of the model. A view is part of the model too:
     * `expose` picks the elements to start from (::* for their members, ::** for everything
     * nested inside), `filter` keeps only certain kinds of element, and the view's
     * definition (GeneralView, InterconnectionView, ...) says what kind of diagram it is.
     * Only the positions of boxes on the canvas are stored outside the model.
     */

    private import StandardViewDefinitions::*;
    private import Views::asTreeDiagram;
    private import Reporting::*;
    private import RiskAnalysis::FmeaWorksheet;

    view serviceContext : InterconnectionView {
        doc /* The drone in its world: operator, ground station and recipient. */
        expose DeliveryContext::deliveryService::*;
    }

    view componentCatalogue : GeneralView {
        doc /* The component definitions and how each specializes Component. */
        expose DroneComponents::*;
        filter @SysML::PartDefinition;
    }

    view droneBreakdown : GeneralView {
        doc /* The parts tree of our drone design: what it is made of, and how many. */
        expose DroneArchitecture::drone::**;
        filter @SysML::PartUsage;
        render asTreeDiagram;
    }

    view droneInterconnection : InterconnectionView {
        doc /* The drone's parts with their ports, and the power, command and signal connections between them. */
        expose DroneArchitecture::drone::*;
    }

    view requirementTree : GeneralView {
        doc /* Stakeholder needs and the system requirements derived from them. */
        expose StakeholderNeeds::*;
        expose SystemRequirements::*;
        filter @SysML::RequirementUsage or @SysML::ConnectionUsage;
    }

    view deliveryFlightFlow : ActionFlowView {
        doc /* The steps of one delivery flight, with the fork, join, decision and merge. */
        expose DeliveryMission::DeliveryFlight;
    }

    view flightModeStates : StateTransitionView {
        doc /* The flight modes and what makes the drone switch between them. */
        expose FlightControl::FlightModes;
    }

    view requirementCoverage : RequirementsTraceMatrix {
        doc /* Which requirements are satisfied by the design and verified, and which are not yet. */
    }

    view fmeaWorksheet : FmeaWorksheet {
        doc /* The design FMEA as a table, highest risk priority first. */
    }
}
