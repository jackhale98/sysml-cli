library package EngineeringBasics {
    doc /* General-purpose vocabulary for engineering requirements: kind, priority and verification method. */

    enum def RequirementKind {
        doc /* The nature of a requirement. */
        enum functional;
        enum performance;
        enum interface;
        enum safety;
        enum regulatory;
        enum environmental;
    }

    enum def Priority {
        doc /* How important a requirement is to the success of the product. */
        enum high;
        enum medium;
        enum low;
    }

    enum def VerificationMethod {
        doc /* The way a requirement is shown to be met. */
        enum test;
        enum analysis;
        enum inspection;
        enum demonstration;
    }

    requirement def EngineeringRequirement {
        doc /* A requirement classified by kind, priority and planned verification method. */
        attribute kind : RequirementKind[0..1];
        attribute priority : Priority[0..1];
        attribute verificationMethod : VerificationMethod[0..*];
    }
}
