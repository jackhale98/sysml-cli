/// Trend analysis and escalation detection for NCRs.

use std::collections::BTreeMap;
use serde::Serialize;

use crate::enums::{NonconformanceCategory, SeverityClass};
use crate::ncr::Ncr;

/// A single data point for trend analysis.
#[derive(Debug, Clone, Serialize)]
pub struct TrendItem {
    pub category: NonconformanceCategory,
    pub severity: SeverityClass,
    pub count: usize,
    pub period: String,
}

/// Group NCRs and produce trend counts.
///
/// The `group_by` parameter selects the grouping dimension:
/// - `"category"` — group by [`NonconformanceCategory`]
/// - `"severity"` — group by [`SeverityClass`]
/// - `"part"` — group by `part_name`
/// - `"supplier"` — group by `supplier` (NCRs without a supplier are skipped)
pub fn trend_analysis(ncrs: &[Ncr], group_by: &str) -> Vec<TrendItem> {
    let mut groups: BTreeMap<String, (NonconformanceCategory, SeverityClass, usize)> =
        BTreeMap::new();

    for ncr in ncrs {
        let key = match group_by {
            "category" => ncr.category.id().to_string(),
            "severity" => ncr.severity.id().to_string(),
            "part" => ncr.part_name.clone(),
            "supplier" => match &ncr.supplier {
                Some(s) => s.clone(),
                None => continue,
            },
            _ => continue,
        };

        let entry = groups.entry(key).or_insert((ncr.category, ncr.severity, 0));
        entry.2 += 1;
    }

    groups
        .into_iter()
        .map(|(period, (category, severity, count))| TrendItem {
            category,
            severity,
            count,
            period,
        })
        .collect()
}

/// Check whether any failure pattern exceeds a threshold, indicating
/// a systemic issue that requires escalation.
///
/// Two NCRs are considered "same failure" if they share both `part_name`
/// and `category`. Returns warning messages for each escalation trigger.
pub fn check_escalation(
    ncrs: &[Ncr],
    same_failure_threshold: usize,
    time_window_days: u32,
) -> Vec<String> {
    if ncrs.is_empty() || same_failure_threshold == 0 {
        return Vec::new();
    }

    let mut groups: BTreeMap<(String, String), Vec<&Ncr>> = BTreeMap::new();
    for ncr in ncrs {
        let key = (ncr.part_name.clone(), ncr.category.id().to_string());
        groups.entry(key).or_default().push(ncr);
    }

    let mut warnings = Vec::new();

    for ((part, cat), group_ncrs) in &groups {
        let mut dates: Vec<&str> = group_ncrs
            .iter()
            .filter_map(|n| {
                if n.created.len() >= 10 {
                    Some(&n.created[..10])
                } else {
                    None
                }
            })
            .collect();
        dates.sort();

        if dates.is_empty() {
            continue;
        }

        let window_secs = time_window_days as i64 * 86400;

        for (i, &start_date) in dates.iter().enumerate() {
            let start_epoch = match date_to_epoch(start_date) {
                Some(e) => e,
                None => continue,
            };

            let mut count = 0usize;
            for &d in &dates[i..] {
                let epoch = match date_to_epoch(d) {
                    Some(e) => e,
                    None => continue,
                };
                if epoch - start_epoch <= window_secs {
                    count += 1;
                } else {
                    break;
                }
            }

            if count >= same_failure_threshold {
                warnings.push(format!(
                    "ESCALATION: {count} NCRs for part '{part}' category '{cat}' \
                     within {time_window_days} days (threshold: {same_failure_threshold})",
                ));
                break;
            }
        }
    }

    warnings
}

/// Parse a `YYYY-MM-DD` date string into seconds since Unix epoch.
fn date_to_epoch(date: &str) -> Option<i64> {
    if date.len() < 10 {
        return None;
    }
    let year: i64 = date[0..4].parse().ok()?;
    let month: i64 = date[5..7].parse().ok()?;
    let day: i64 = date[8..10].parse().ok()?;

    let m = if month <= 2 { month + 9 } else { month - 3 };
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Some(days * 86400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::NcrStatus;

    fn sample_ncr() -> Ncr {
        Ncr {
            id: "NCR-001".into(),
            part_name: "BrakeRotor".into(),
            lot_id: Some("LOT-2026-03".into()),
            supplier: Some("AcmeCasting".into()),
            category: NonconformanceCategory::Dimensional,
            severity: SeverityClass::Major,
            description: "OD out of tolerance by 0.5mm".into(),
            disposition: None,
            status: NcrStatus::Open,
            created: "2026-03-01T10:00:00Z".into(),
            owner: "alice".into(),
            linked_capas: Vec::new(),
        }
    }

    #[test]
    fn trend_by_category() {
        let ncrs = vec![
            sample_ncr(),
            { let mut n = sample_ncr(); n.id = "NCR-002".into(); n },
            {
                let mut n = sample_ncr();
                n.id = "NCR-003".into();
                n.category = NonconformanceCategory::Functional;
                n
            },
        ];

        let trends = trend_analysis(&ncrs, "category");
        assert_eq!(trends.len(), 2);
        let dim = trends.iter().find(|t| t.period == "dimensional").unwrap();
        assert_eq!(dim.count, 2);
        let func = trends.iter().find(|t| t.period == "functional").unwrap();
        assert_eq!(func.count, 1);
    }

    #[test]
    fn trend_by_severity() {
        let ncrs = vec![
            sample_ncr(),
            {
                let mut n = sample_ncr();
                n.id = "NCR-002".into();
                n.severity = SeverityClass::Critical;
                n
            },
        ];

        let trends = trend_analysis(&ncrs, "severity");
        assert_eq!(trends.len(), 2);
    }

    #[test]
    fn trend_by_part() {
        let ncrs = vec![
            sample_ncr(),
            {
                let mut n = sample_ncr();
                n.id = "NCR-002".into();
                n.part_name = "CalliperHousing".into();
                n
            },
        ];

        let trends = trend_analysis(&ncrs, "part");
        assert_eq!(trends.len(), 2);
    }

    #[test]
    fn trend_by_supplier_skips_none() {
        let ncrs = vec![
            sample_ncr(),
            {
                let mut n = sample_ncr();
                n.id = "NCR-002".into();
                n.supplier = None;
                n
            },
        ];

        let trends = trend_analysis(&ncrs, "supplier");
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].period, "AcmeCasting");
    }

    #[test]
    fn trend_empty_input() {
        assert!(trend_analysis(&[], "category").is_empty());
    }

    #[test]
    fn trend_unknown_group_by() {
        let ncrs = vec![sample_ncr()];
        assert!(trend_analysis(&ncrs, "nonexistent").is_empty());
    }

    #[test]
    fn escalation_triggers_on_threshold() {
        let ncrs = vec![
            { let mut n = sample_ncr(); n.created = "2026-03-01T10:00:00Z".into(); n },
            { let mut n = sample_ncr(); n.id = "NCR-002".into(); n.created = "2026-03-05T10:00:00Z".into(); n },
            { let mut n = sample_ncr(); n.id = "NCR-003".into(); n.created = "2026-03-10T10:00:00Z".into(); n },
        ];

        let warnings = check_escalation(&ncrs, 3, 30);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("ESCALATION"));
        assert!(warnings[0].contains("3 NCRs"));
    }

    #[test]
    fn escalation_no_trigger_below_threshold() {
        let ncrs = vec![
            sample_ncr(),
            { let mut n = sample_ncr(); n.id = "NCR-002".into(); n.created = "2026-03-05T10:00:00Z".into(); n },
        ];
        assert!(check_escalation(&ncrs, 3, 30).is_empty());
    }

    #[test]
    fn escalation_respects_time_window() {
        let ncrs = vec![
            { let mut n = sample_ncr(); n.created = "2026-01-01T10:00:00Z".into(); n },
            { let mut n = sample_ncr(); n.id = "NCR-002".into(); n.created = "2026-06-01T10:00:00Z".into(); n },
            { let mut n = sample_ncr(); n.id = "NCR-003".into(); n.created = "2026-12-01T10:00:00Z".into(); n },
        ];
        assert!(check_escalation(&ncrs, 3, 30).is_empty());
    }

    #[test]
    fn escalation_empty_input() {
        assert!(check_escalation(&[], 3, 30).is_empty());
    }

    #[test]
    fn escalation_zero_threshold() {
        assert!(check_escalation(&[sample_ncr()], 0, 30).is_empty());
    }

    #[test]
    fn escalation_different_parts_no_trigger() {
        let ncrs = vec![
            { let mut n = sample_ncr(); n.created = "2026-03-01T10:00:00Z".into(); n },
            {
                let mut n = sample_ncr();
                n.id = "NCR-002".into();
                n.part_name = "OtherPart".into();
                n.created = "2026-03-02T10:00:00Z".into();
                n
            },
        ];
        assert!(check_escalation(&ncrs, 2, 30).is_empty());
    }

    #[test]
    fn date_epoch_known_dates() {
        assert_eq!(date_to_epoch("1970-01-01"), Some(0));
        assert_eq!(date_to_epoch("2000-01-01"), Some(10957 * 86400));
    }

    #[test]
    fn date_epoch_malformed() {
        assert_eq!(date_to_epoch("bad"), None);
        assert_eq!(date_to_epoch(""), None);
    }
}
