# Domain libraries

Pure SysML v2 model libraries usable from any conformant tool. `sysml init`
automatically adds a project-local `libraries/` directory to the import
resolution path.

## Engineering analysis (canonical source: sysml-domain-libraries)

`Uncertainty.sysml`, `Tolerancing.sysml`, `RiskAnalysis.sysml`,
`HazardAnalysis.sysml`, `Reporting.sysml`, and `StandardViews.sysml` are
synced from
[sysml-domain-libraries](https://github.com/jackhale98/sysml-domain-libraries),
which is their canonical home (design rationale, examples, and validation
harness live there). They cover toleranced dimensions and GD&T, tolerance stackup
analyses, FMEA on the AIAG/VDA 1-10 scales, and RAAML-aligned hazard
analysis with causal chains. Do not edit these copies directly — change
them upstream and re-sync.

## Project sketches (legacy)

The remaining `sysml-*.sysml` files are earlier, shallower sketches
(BOM, CAPA, manufacturing, project management, quality, verification
extensions). They will be superseded by upstream packages
(`QualityManagement`, `ProjectMetadata`) as those mature.
