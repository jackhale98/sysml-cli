/// FMI 3.0 export and validation for SysML v2 models.
///
/// Provides:
///   - Interface extraction (SysML ports → FMI variable contracts)
///   - Modelica partial model stub generation
///   - SSP (System Structure Description) XML generation
///   - Exportable part listing

pub mod fmi;
pub mod modelica;
pub mod ssp;
