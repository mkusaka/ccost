use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone};
use chrono_tz::Tz;
use std::str::FromStr;

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

pub fn format_date(timestamp: &str, timezone: Option<&str>) -> Option<String> {
    let tz = match timezone {
        Some(tz_str) => Some(Tz::from_str(tz_str).ok()?),
        None => None,
    };
    format_date_with_tz(timestamp, tz)
}

pub fn format_date_with_tz(timestamp: &str, timezone: Option<Tz>) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(timestamp).ok()?;
    let date = match timezone {
        Some(tz) => parsed.with_timezone(&tz).date_naive(),
        None => parsed.with_timezone(&Local).date_naive(),
    };
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month(),
        date.day()
    ))
}

pub fn format_month(date_str: &str) -> Option<String> {
    if date_str.len() >= 7 {
        Some(date_str[..7].to_string())
    } else {
        None
    }
}

pub fn format_date_compact(date_str: &str, timezone: Option<&str>) -> Option<String> {
    let is_simple_date = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$")
        .map(|re| re.is_match(date_str))
        .unwrap_or(false);

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
            if let Some(since) = since
                && date_str.as_str() < since
            {
                return false;
            }
            if let Some(until) = until
                && date_str.as_str() > until
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
    fn filter_by_date_range_filters_items() {
        let items = vec!["2024-01-01", "2024-01-02", "2024-01-03", "2024-01-04"];
        let filtered = filter_by_date_range(items, |item| item, Some("20240102"), Some("20240103"));
        assert_eq!(filtered, vec!["2024-01-02", "2024-01-03"]);
    }
}
