use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike};
use chrono_tz::Tz;
use std::str::FromStr;
use std::sync::LazyLock;

static SIMPLE_DATE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid date regex"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl FromStr for SortOrder {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "asc" => Ok(Self::Asc),
            "desc" => Ok(Self::Desc),
            _ => Err(format!("Invalid sort order: {value}")),
        }
    }
}

/// Time bucket used as the aggregation key: `2024-08-04` or `2024-08-04T13`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Granularity {
    #[default]
    Day,
    Hour,
}

pub fn format_date(timestamp: &str, timezone: Option<&str>) -> Option<String> {
    let tz = match timezone {
        Some(tz_str) => Some(Tz::from_str(tz_str).ok()?),
        None => None,
    };
    format_date_with_tz(timestamp, tz, Granularity::Day)
}

pub fn format_date_with_tz(
    timestamp: &str,
    timezone: Option<Tz>,
    granularity: Granularity,
) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(timestamp).ok()?;
    Some(match timezone {
        Some(tz) => format_period(&parsed.with_timezone(&tz), granularity),
        None => format_period(&parsed.with_timezone(&Local), granularity),
    })
}

fn format_period<T: TimeZone>(datetime: &DateTime<T>, granularity: Granularity) -> String {
    let date = format!(
        "{:04}-{:02}-{:02}",
        datetime.year(),
        datetime.month(),
        datetime.day()
    );
    match granularity {
        Granularity::Day => date,
        Granularity::Hour => format!("{date}T{:02}", datetime.hour()),
    }
}

/// Formats an hourly key (`2024-08-04T13`) for the table's first column.
/// The key is already in the report timezone, so no conversion happens here.
pub fn format_hour_compact(period: &str) -> Option<String> {
    let (date, hour) = period.split_once('T')?;
    Some(format!("{date}\n{hour}:00"))
}

pub fn format_month(date_str: &str) -> Option<String> {
    if date_str.len() >= 7 {
        Some(date_str[..7].to_string())
    } else {
        None
    }
}

pub fn format_date_compact(date_str: &str, timezone: Option<&str>) -> Option<String> {
    let is_simple_date = SIMPLE_DATE_RE.is_match(date_str);

    let date = if is_simple_date {
        let naive = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
        match timezone {
            Some(tz_str) => {
                let tz = Tz::from_str(tz_str).ok()?;
                let utc_dt = chrono::Utc.from_utc_datetime(&naive.and_hms_opt(0, 0, 0)?);
                utc_dt.with_timezone(&tz).date_naive()
            }
            None => {
                let local_dt = Local
                    .from_local_datetime(&naive.and_hms_opt(0, 0, 0)?)
                    .single();
                local_dt?.date_naive()
            }
        }
    } else {
        let parsed = DateTime::parse_from_rfc3339(date_str).ok()?;
        match timezone {
            Some(tz_str) => {
                let tz = Tz::from_str(tz_str).ok()?;
                parsed.with_timezone(&tz).date_naive()
            }
            None => parsed.with_timezone(&Local).date_naive(),
        }
    };

    Some(format!(
        "{:04}\n{:02}-{:02}",
        date.year(),
        date.month(),
        date.day()
    ))
}

pub fn filter_by_date_range<T, F>(
    items: Vec<T>,
    get_date: F,
    since: Option<&str>,
    until: Option<&str>,
) -> Vec<T>
where
    F: Fn(&T) -> &str,
{
    if since.is_none() && until.is_none() {
        return items;
    }

    items
        .into_iter()
        .filter(|item| {
            let date_str = get_date(item).replace('-', "");
            // Hourly keys carry a `T13` suffix; since/until stay at day resolution.
            let date_str = date_str.get(..8).unwrap_or(date_str.as_str());
            if let Some(since) = since
                && date_str < since
            {
                return false;
            }
            if let Some(until) = until
                && date_str > until
            {
                return false;
            }
            true
        })
        .collect()
}

pub fn sort_by_date<T, F>(mut items: Vec<T>, get_date: F, order: SortOrder) -> Vec<T>
where
    F: Fn(&T) -> &str,
{
    items.sort_by(|a, b| match order {
        SortOrder::Asc => get_date(a).cmp(get_date(b)),
        SortOrder::Desc => get_date(b).cmp(get_date(a)),
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_date_to_yyyy_mm_dd() {
        let result = format_date("2024-08-04T12:00:00Z", None).unwrap();
        assert!(
            regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$")
                .unwrap()
                .is_match(&result)
        );
    }

    #[test]
    fn format_date_with_timezone() {
        let result = format_date("2024-08-04T12:00:00Z", Some("UTC")).unwrap();
        assert_eq!(result, "2024-08-04");
    }

    #[test]
    fn format_date_compact_formats_with_newline() {
        let result = format_date_compact("2024-08-04", None).unwrap();
        assert_eq!(result, "2024\n08-04");
    }

    #[test]
    fn format_date_compact_with_timezone() {
        let result = format_date_compact("2024-08-04T12:00:00Z", Some("UTC")).unwrap();
        assert_eq!(result, "2024\n08-04");
    }

    #[test]
    fn format_date_with_tz_supports_hourly_granularity() {
        let parsed = "2024-08-04T13:30:00Z";
        let tz = Some(Tz::from_str("UTC").unwrap());
        assert_eq!(
            format_date_with_tz(parsed, tz, Granularity::Hour).unwrap(),
            "2024-08-04T13"
        );
        assert_eq!(
            format_date_with_tz(parsed, tz, Granularity::Day).unwrap(),
            "2024-08-04"
        );
    }

    #[test]
    fn format_hour_compact_formats_with_newline() {
        assert_eq!(
            format_hour_compact("2024-08-04T13").unwrap(),
            "2024-08-04\n13:00"
        );
        assert!(format_hour_compact("2024-08-04").is_none());
    }

    #[test]
    fn filter_by_date_range_keeps_hourly_entries_on_boundary_days() {
        let items = vec!["2024-01-01T23", "2024-01-02T00", "2024-01-02T23"];
        let filtered = filter_by_date_range(items, |item| item, Some("20240102"), Some("20240102"));
        assert_eq!(filtered, vec!["2024-01-02T00", "2024-01-02T23"]);
    }

    #[test]
    fn filter_by_date_range_filters_items() {
        let items = vec!["2024-01-01", "2024-01-02", "2024-01-03", "2024-01-04"];
        let filtered = filter_by_date_range(items, |item| item, Some("20240102"), Some("20240103"));
        assert_eq!(filtered, vec!["2024-01-02", "2024-01-03"]);
    }
}
