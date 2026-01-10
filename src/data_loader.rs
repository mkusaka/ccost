use crate::pricing::{CostMode, PricingFetcher, UsageTokens};
use crate::time_utils::{SortOrder, filter_by_date_range, format_date, format_month, sort_by_date};
use crate::token_utils::{AggregatedTokenCounts, get_total_tokens_from_aggregated};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use jwalk::WalkDir;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const CLAUDE_CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
const CLAUDE_PROJECTS_DIR_NAME: &str = "projects";
const DEFAULT_CLAUDE_CODE_PATH: &str = ".claude";

fn default_claude_config_path() -> PathBuf {
    if let Some(dir) = dirs::config_dir() {
        return dir.join("claude");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/claude");
    }
    PathBuf::from(".config/claude")
}

#[derive(Debug, Clone, Deserialize)]
struct UsageMessageUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct UsageMessage {
    usage: Option<UsageMessageUsage>,
    model: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UsageRequest {
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UsageData {
    timestamp: Option<String>,
    message: Option<UsageMessage>,
    #[serde(rename = "costUSD")]
    cost_usd: Option<f64>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    request: Option<UsageRequest>,
}

#[derive(Debug, Clone)]
pub struct ModelBreakdown {
    pub model_name: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone)]
pub struct DailyUsage {
    pub date: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_cost: f64,
    pub models_used: Vec<String>,
    pub model_breakdowns: Vec<ModelBreakdown>,
    pub project: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MonthlyUsage {
    pub month: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_cost: f64,
    pub models_used: Vec<String>,
    pub model_breakdowns: Vec<ModelBreakdown>,
    pub project: Option<String>,
}

#[derive(Debug, Clone)]
struct TokenStats {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    cost: f64,
}

impl Default for TokenStats {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            cost: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
struct Aggregate {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    total_cost: f64,
    models_used: Vec<String>,
    models_used_seen: HashSet<String>,
    model_breakdowns: HashMap<String, TokenStats>,
}

impl Default for Aggregate {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            total_cost: 0.0,
            models_used: Vec::new(),
            models_used_seen: HashSet::new(),
            model_breakdowns: HashMap::new(),
        }
    }
}

impl Aggregate {
    fn push_model(&mut self, model: &str) {
        let owned = model.to_string();
        if self.models_used_seen.insert(owned.clone()) {
            self.models_used.push(owned);
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadOptions {
    pub claude_path: Option<PathBuf>,
    pub mode: CostMode,
    pub order: SortOrder,
    pub offline: bool,
    pub group_by_project: bool,
    pub project: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub timezone: Option<String>,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            claude_path: None,
            mode: CostMode::Auto,
            order: SortOrder::Desc,
            offline: false,
            group_by_project: false,
            project: None,
            since: None,
            until: None,
            timezone: None,
        }
    }
}

pub struct GlobResult {
    pub file: PathBuf,
    pub base_dir: PathBuf,
}

struct ParsedRecord {
    unique_hash: Option<String>,
    date: String,
    project: String,
    model: Option<String>,
    tokens: UsageTokens,
    cost: f64,
}

pub fn get_claude_paths() -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(env_paths) = std::env::var(CLAUDE_CONFIG_DIR_ENV) {
        let env_paths = env_paths.trim();
        if !env_paths.is_empty() {
            for raw in env_paths.split(',') {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let base = PathBuf::from(trimmed);
                if base.is_dir() && base.join(CLAUDE_PROJECTS_DIR_NAME).is_dir() {
                    let normalized = base.canonicalize().unwrap_or(base.clone());
                    if seen.insert(normalized.clone()) {
                        paths.push(normalized);
                    }
                }
            }
            if !paths.is_empty() {
                return Ok(paths);
            }
            return Err(anyhow!(
                "No valid Claude data directories found in CLAUDE_CONFIG_DIR"
            ));
        }
    }

    let defaults = vec![
        default_claude_config_path(),
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(DEFAULT_CLAUDE_CODE_PATH),
    ];

    for base in defaults {
        if base.is_dir() && base.join(CLAUDE_PROJECTS_DIR_NAME).is_dir() {
            let normalized = base.canonicalize().unwrap_or(base.clone());
            if seen.insert(normalized.clone()) {
                paths.push(normalized);
            }
        }
    }

    if paths.is_empty() {
        return Err(anyhow!("No valid Claude data directories found"));
    }

    Ok(paths)
}

fn parse_file_records(
    file: &Path,
    project: &str,
    options: &LoadOptions,
    pricing: Option<&PricingFetcher>,
) -> Result<Vec<ParsedRecord>> {
    let mut records = Vec::new();
    process_jsonl_file_by_line(file, |line, _| {
        let parsed: UsageData = match sonic_rs::from_str(line) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(()),
        };

        let message = match parsed.message.as_ref() {
            Some(message) => message,
            None => return Ok(()),
        };
        let tokens = match extract_usage_tokens(message) {
            Some(tokens) => tokens,
            None => return Ok(()),
        };
        let timestamp = match parsed.timestamp.as_deref() {
            Some(ts) => ts,
            None => return Ok(()),
        };

        let date = match format_date(timestamp, options.timezone.as_deref()) {
            Some(date) => date,
            None => return Ok(()),
        };

        let cost = calculate_cost_for_entry(&parsed, options.mode, pricing);
        let unique_hash = create_unique_hash(&parsed);

        records.push(ParsedRecord {
            unique_hash,
            date,
            project: project.to_string(),
            model: message.model.clone(),
            tokens,
            cost,
        });

        Ok(())
    })?;
    Ok(records)
}

pub fn extract_project_from_path(path: &Path) -> String {
    let mut found_projects = false;
    for component in path.components() {
        let value = component.as_os_str().to_string_lossy();
        if found_projects {
            return if value.trim().is_empty() {
                "unknown".to_string()
            } else {
                value.to_string()
            };
        }
        if value == CLAUDE_PROJECTS_DIR_NAME {
            found_projects = true;
        }
    }
    "unknown".to_string()
}

pub fn process_jsonl_file_by_line<F>(file_path: &Path, mut process_line: F) -> Result<()>
where
    F: FnMut(&str, usize) -> Result<()> + Send,
{
    let file = File::open(file_path)?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut line = String::new();
    let mut line_number = 0;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        process_line(trimmed, line_number)?;
    }
    Ok(())
}

pub fn get_earliest_timestamp(file_path: &Path) -> Option<DateTime<Utc>> {
    let file = File::open(file_path).ok()?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut earliest: Option<DateTime<Utc>> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = match reader.read_line(&mut line) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if bytes == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed: Result<UsageData, _> = sonic_rs::from_str(trimmed);
        if let Ok(parsed) = parsed {
            if let Some(ts) = parsed.timestamp.as_deref() {
                if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                    let utc = dt.with_timezone(&Utc);
                    earliest = match earliest {
                        Some(existing) if existing <= utc => Some(existing),
                        _ => Some(utc),
                    };
                }
            }
        }
    }
    earliest
}

pub fn sort_files_by_timestamp(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files_with_ts: Vec<(PathBuf, Option<DateTime<Utc>>)> = files
        .into_par_iter()
        .map(|file| {
            let ts = get_earliest_timestamp(&file);
            (file, ts)
        })
        .collect();

    files_with_ts.sort_by(|a, b| match (&a.1, &b.1) {
        (Some(a_ts), Some(b_ts)) => a_ts.cmp(b_ts),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    files_with_ts.into_iter().map(|(file, _)| file).collect()
}

pub fn glob_usage_files(claude_paths: &[PathBuf]) -> Vec<GlobResult> {
    let mut results = Vec::new();
    for base in claude_paths {
        let projects_dir = base.join(CLAUDE_PROJECTS_DIR_NAME);
        if !projects_dir.is_dir() {
            continue;
        }
        let entries = WalkDir::new(&projects_dir)
            .parallelism(jwalk::Parallelism::RayonNewPool(0))
            .follow_links(true)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext == "jsonl")
                    .unwrap_or(false)
            })
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        for file in entries {
            results.push(GlobResult {
                file,
                base_dir: projects_dir.clone(),
            });
        }
    }
    results
}

fn create_unique_hash(data: &UsageData) -> Option<String> {
    let message_id = data.message.as_ref()?.id.as_ref()?;
    let request_id = data
        .request_id
        .as_ref()
        .or_else(|| data.request.as_ref().and_then(|r| r.id.as_ref()))?;
    Some(format!("{message_id}:{request_id}"))
}

fn extract_usage_tokens(message: &UsageMessage) -> Option<UsageTokens> {
    let usage = message.usage.as_ref()?;
    let input = usage.input_tokens?;
    let output = usage.output_tokens?;
    Some(UsageTokens {
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
        cache_read_input_tokens: usage.cache_read_input_tokens.unwrap_or(0),
    })
}

fn update_model_breakdowns(
    breakdowns: &mut HashMap<String, TokenStats>,
    model_name: &str,
    tokens: &UsageTokens,
    cost: f64,
) {
    let entry = breakdowns.entry(model_name.to_string()).or_default();
    entry.input_tokens += tokens.input_tokens;
    entry.output_tokens += tokens.output_tokens;
    entry.cache_creation_tokens += tokens.cache_creation_input_tokens;
    entry.cache_read_tokens += tokens.cache_read_input_tokens;
    entry.cost += cost;
}

fn calculate_cost_for_entry(
    data: &UsageData,
    mode: CostMode,
    pricing: Option<&PricingFetcher>,
) -> f64 {
    match mode {
        CostMode::Display => data.cost_usd.unwrap_or(0.0),
        CostMode::Calculate => {
            let message = match &data.message {
                Some(message) => message,
                None => return 0.0,
            };
            let tokens = match extract_usage_tokens(message) {
                Some(tokens) => tokens,
                None => return 0.0,
            };
            let model = message.model.as_deref();
            pricing
                .map(|fetcher| fetcher.calculate_cost_from_tokens(&tokens, model))
                .unwrap_or(0.0)
        }
        CostMode::Auto => {
            if let Some(cost) = data.cost_usd {
                return cost;
            }
            let message = match &data.message {
                Some(message) => message,
                None => return 0.0,
            };
            let tokens = match extract_usage_tokens(message) {
                Some(tokens) => tokens,
                None => return 0.0,
            };
            let model = message.model.as_deref();
            pricing
                .map(|fetcher| fetcher.calculate_cost_from_tokens(&tokens, model))
                .unwrap_or(0.0)
        }
    }
}

pub fn load_daily_usage_data(options: LoadOptions) -> Result<Vec<DailyUsage>> {
    let claude_paths = if let Some(path) = &options.claude_path {
        vec![path.clone()]
    } else {
        get_claude_paths()?
    };

    let all_files = glob_usage_files(&claude_paths);
    if all_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut file_list = all_files.into_iter().map(|f| f.file).collect::<Vec<_>>();

    if let Some(project) = &options.project {
        file_list.retain(|file| extract_project_from_path(file) == *project);
    }

    if file_list.is_empty() {
        return Ok(Vec::new());
    }

    let sorted_files = sort_files_by_timestamp(file_list);
    let pricing = if matches!(options.mode, CostMode::Display) {
        None
    } else {
        Some(PricingFetcher::new())
    };

    let mut processed_hashes = HashSet::new();
    let mut aggregates: HashMap<String, Aggregate> = HashMap::new();

    let needs_project_grouping = options.group_by_project || options.project.is_some();

    let pricing_ref = pricing.as_ref();
    let file_entries = sorted_files
        .into_iter()
        .map(|file| {
            let project = extract_project_from_path(&file);
            (file, project)
        })
        .collect::<Vec<_>>();

    let batch_size = (rayon::current_num_threads() * 2).max(1);

    for chunk in file_entries.chunks(batch_size) {
        let parsed_chunks = chunk
            .par_iter()
            .map(|(file, project)| parse_file_records(file, project, &options, pricing_ref))
            .collect::<Vec<_>>();

        for records in parsed_chunks {
            let records = records?;
            for record in records {
                if let Some(hash) = &record.unique_hash {
                    if processed_hashes.contains(hash) {
                        continue;
                    }
                    processed_hashes.insert(hash.clone());
                }

                let key = if needs_project_grouping {
                    format!(
                        "{date}\u{0}{project}",
                        date = record.date,
                        project = record.project
                    )
                } else {
                    record.date.clone()
                };

                let entry = aggregates.entry(key).or_default();
                entry.input_tokens += record.tokens.input_tokens;
                entry.output_tokens += record.tokens.output_tokens;
                entry.cache_creation_tokens += record.tokens.cache_creation_input_tokens;
                entry.cache_read_tokens += record.tokens.cache_read_input_tokens;
                entry.total_cost += record.cost;

                if let Some(model) = record.model.as_deref() {
                    if model != "<synthetic>" {
                        entry.push_model(model);
                        update_model_breakdowns(
                            &mut entry.model_breakdowns,
                            model,
                            &record.tokens,
                            record.cost,
                        );
                    }
                } else {
                    update_model_breakdowns(
                        &mut entry.model_breakdowns,
                        "unknown",
                        &record.tokens,
                        record.cost,
                    );
                }
            }
        }
    }

    let mut results = Vec::new();
    for (group_key, aggregate) in aggregates {
        let (date, project) = if let Some((date, project)) = group_key.split_once('\u{0}') {
            (date.to_string(), Some(project.to_string()))
        } else {
            (group_key, None)
        };

        let mut model_breakdowns = aggregate
            .model_breakdowns
            .into_iter()
            .filter(|(name, _)| name != "<synthetic>")
            .map(|(model_name, stats)| ModelBreakdown {
                model_name,
                input_tokens: stats.input_tokens,
                output_tokens: stats.output_tokens,
                cache_creation_tokens: stats.cache_creation_tokens,
                cache_read_tokens: stats.cache_read_tokens,
                cost: stats.cost,
            })
            .collect::<Vec<_>>();
        model_breakdowns.sort_by(|a, b| {
            b.cost
                .partial_cmp(&a.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let models_used = aggregate.models_used;

        results.push(DailyUsage {
            date,
            input_tokens: aggregate.input_tokens,
            output_tokens: aggregate.output_tokens,
            cache_creation_tokens: aggregate.cache_creation_tokens,
            cache_read_tokens: aggregate.cache_read_tokens,
            total_cost: aggregate.total_cost,
            models_used,
            model_breakdowns,
            project,
        });
    }

    let filtered = filter_by_date_range(
        results,
        |item| item.date.as_str(),
        options.since.as_deref(),
        options.until.as_deref(),
    );

    let mut final_results = if let Some(project) = &options.project {
        filtered
            .into_iter()
            .filter(|item| item.project.as_deref() == Some(project))
            .collect::<Vec<_>>()
    } else {
        filtered
    };

    final_results = sort_by_date(final_results, |item| item.date.as_str(), options.order);

    Ok(final_results)
}

pub fn load_monthly_usage_data(options: LoadOptions) -> Result<Vec<MonthlyUsage>> {
    let daily = load_daily_usage_data(options.clone())?;
    if daily.is_empty() {
        return Ok(Vec::new());
    }

    let mut aggregates: HashMap<String, Aggregate> = HashMap::new();
    let needs_project_grouping = options.group_by_project || options.project.is_some();

    for entry in daily {
        let month = match format_month(&entry.date) {
            Some(month) => month,
            None => continue,
        };
        let key = if needs_project_grouping {
            format!(
                "{month}\u{0}{}",
                entry
                    .project
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string())
            )
        } else {
            month.clone()
        };

        let aggregate = aggregates.entry(key).or_default();
        aggregate.input_tokens += entry.input_tokens;
        aggregate.output_tokens += entry.output_tokens;
        aggregate.cache_creation_tokens += entry.cache_creation_tokens;
        aggregate.cache_read_tokens += entry.cache_read_tokens;
        aggregate.total_cost += entry.total_cost;
        for model in entry.models_used {
            aggregate.push_model(&model);
        }
        for breakdown in entry.model_breakdowns {
            update_model_breakdowns(
                &mut aggregate.model_breakdowns,
                &breakdown.model_name,
                &UsageTokens {
                    input_tokens: breakdown.input_tokens,
                    output_tokens: breakdown.output_tokens,
                    cache_creation_input_tokens: breakdown.cache_creation_tokens,
                    cache_read_input_tokens: breakdown.cache_read_tokens,
                },
                breakdown.cost,
            );
        }
    }

    let mut results = Vec::new();
    for (group_key, aggregate) in aggregates {
        let (month, project) = if let Some((month, project)) = group_key.split_once('\u{0}') {
            (month.to_string(), Some(project.to_string()))
        } else {
            (group_key, None)
        };

        let mut model_breakdowns = aggregate
            .model_breakdowns
            .into_iter()
            .filter(|(name, _)| name != "<synthetic>")
            .map(|(model_name, stats)| ModelBreakdown {
                model_name,
                input_tokens: stats.input_tokens,
                output_tokens: stats.output_tokens,
                cache_creation_tokens: stats.cache_creation_tokens,
                cache_read_tokens: stats.cache_read_tokens,
                cost: stats.cost,
            })
            .collect::<Vec<_>>();
        model_breakdowns.sort_by(|a, b| {
            b.cost
                .partial_cmp(&a.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let models_used = aggregate.models_used;

        results.push(MonthlyUsage {
            month,
            input_tokens: aggregate.input_tokens,
            output_tokens: aggregate.output_tokens,
            cache_creation_tokens: aggregate.cache_creation_tokens,
            cache_read_tokens: aggregate.cache_read_tokens,
            total_cost: aggregate.total_cost,
            models_used,
            model_breakdowns,
            project,
        });
    }

    let results = sort_by_date(results, |item| item.month.as_str(), options.order);

    Ok(results)
}

pub fn calculate_totals_daily(data: &[DailyUsage]) -> UsageTotals {
    let mut totals = UsageTotals::default();
    for item in data {
        totals.input_tokens += item.input_tokens;
        totals.output_tokens += item.output_tokens;
        totals.cache_creation_tokens += item.cache_creation_tokens;
        totals.cache_read_tokens += item.cache_read_tokens;
        totals.total_cost += item.total_cost;
    }
    totals
}

pub fn calculate_totals_monthly(data: &[MonthlyUsage]) -> UsageTotals {
    let mut totals = UsageTotals::default();
    for item in data {
        totals.input_tokens += item.input_tokens;
        totals.output_tokens += item.output_tokens;
        totals.cache_creation_tokens += item.cache_creation_tokens;
        totals.cache_read_tokens += item.cache_read_tokens;
        totals.total_cost += item.total_cost;
    }
    totals
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct UsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_cost: f64,
}

impl UsageTotals {
    pub fn total_tokens(&self) -> u64 {
        get_total_tokens_from_aggregated(AggregatedTokenCounts {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_creation_tokens: self.cache_creation_tokens,
            cache_read_tokens: self.cache_read_tokens,
        })
    }
}

pub fn group_daily_by_project(data: &[DailyUsage]) -> HashMap<String, Vec<DailyUsage>> {
    let mut projects: HashMap<String, Vec<DailyUsage>> = HashMap::new();
    for item in data {
        let project = item
            .project
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        projects.entry(project).or_default().push(item.clone());
    }
    projects
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn write_file(base: &Path, rel: &str, content: &str) {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn create_fixture() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn load_daily_usage_returns_empty_when_no_files() {
        let fixture = create_fixture();
        write_file(fixture.path(), "projects", "");
        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            timezone: Some("UTC".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn load_daily_usage_aggregates_data() {
        let fixture = create_fixture();
        let data1 = vec![
            json!({
                "timestamp": "2024-01-01T10:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100 } },
                "costUSD": 0.02
            }),
        ];
        let data2 = json!({
            "timestamp": "2024-01-01T18:00:00Z",
            "message": { "usage": { "input_tokens": 300, "output_tokens": 150 } },
            "costUSD": 0.03
        });
        write_file(
            fixture.path(),
            "projects/project1/session1/file1.jsonl",
            &data1
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        write_file(
            fixture.path(),
            "projects/project1/session2/file2.jsonl",
            &data2.to_string(),
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            timezone: Some("UTC".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].date, "2024-01-01");
        assert_eq!(result[0].input_tokens, 600);
        assert_eq!(result[0].output_tokens, 300);
        assert_eq!(result[0].total_cost, 0.06);
    }

    #[test]
    fn load_daily_usage_handles_cache_tokens() {
        let fixture = create_fixture();
        let data = json!({
            "timestamp": "2024-01-01T12:00:00Z",
            "message": { "usage": { "input_tokens": 100, "output_tokens": 50, "cache_creation_input_tokens": 25, "cache_read_input_tokens": 10 } },
            "costUSD": 0.01
        });
        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data.to_string(),
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            timezone: Some("UTC".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result[0].cache_creation_tokens, 25);
        assert_eq!(result[0].cache_read_tokens, 10);
    }

    #[test]
    fn load_daily_usage_filters_by_date_range() {
        let fixture = create_fixture();
        let data = vec![
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-01-15T12:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100 } },
                "costUSD": 0.02
            }),
            json!({
                "timestamp": "2024-01-31T12:00:00Z",
                "message": { "usage": { "input_tokens": 300, "output_tokens": 150 } },
                "costUSD": 0.03
            }),
        ];
        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            since: Some("20240110".to_string()),
            until: Some("20240125".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].date, "2024-01-15");
        assert_eq!(result[0].input_tokens, 200);
    }

    #[test]
    fn load_daily_usage_sorting_default_desc() {
        let fixture = create_fixture();
        let data = vec![
            json!({
                "timestamp": "2024-01-15T12:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100 } },
                "costUSD": 0.02
            }),
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-01-31T12:00:00Z",
                "message": { "usage": { "input_tokens": 300, "output_tokens": 150 } },
                "costUSD": 0.03
            }),
        ];
        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            timezone: Some("UTC".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result[0].date, "2024-01-31");
        assert_eq!(result[1].date, "2024-01-15");
        assert_eq!(result[2].date, "2024-01-01");
    }

    #[test]
    fn load_daily_usage_sorting_asc() {
        let fixture = create_fixture();
        let data = vec![
            json!({
                "timestamp": "2024-01-15T12:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100 } },
                "costUSD": 0.02
            }),
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-01-31T12:00:00Z",
                "message": { "usage": { "input_tokens": 300, "output_tokens": 150 } },
                "costUSD": 0.03
            }),
        ];
        write_file(
            fixture.path(),
            "projects/project1/session1/usage.jsonl",
            &data
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            order: SortOrder::Asc,
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result[0].date, "2024-01-01");
        assert_eq!(result[1].date, "2024-01-15");
        assert_eq!(result[2].date, "2024-01-31");
    }

    #[test]
    fn load_daily_usage_handles_invalid_json_lines() {
        let fixture = create_fixture();
        let data = [
            r#"{"timestamp":"2024-01-01T12:00:00Z","message":{"usage":{"input_tokens":100,"output_tokens":50}},"costUSD":0.01}"#,
            "invalid json line",
            r#"{"timestamp":"2024-01-01T12:00:00Z","message":{"usage":{"input_tokens":200,"output_tokens":100}},"costUSD":0.02}"#,
            "{ broken json",
            r#"{"timestamp":"2024-01-01T18:00:00Z","message":{"usage":{"input_tokens":300,"output_tokens":150}},"costUSD":0.03}"#,
        ]
        .join("\n");

        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data,
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            timezone: Some("UTC".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].input_tokens, 600);
        assert_eq!(result[0].total_cost, 0.06);
    }

    #[test]
    fn load_daily_usage_skips_missing_required_fields() {
        let fixture = create_fixture();
        let data = [
            r#"{"timestamp":"2024-01-01T12:00:00Z","message":{"usage":{"input_tokens":100,"output_tokens":50}},"costUSD":0.01}"#,
            r#"{"timestamp":"2024-01-01T14:00:00Z","message":{"usage":{}}}"#,
            r#"{"timestamp":"2024-01-01T18:00:00Z","message":{}}"#,
            r#"{"timestamp":"2024-01-01T20:00:00Z"}"#,
            r#"{"message":{"usage":{"input_tokens":200,"output_tokens":100}}}"#,
            r#"{"timestamp":"2024-01-01T22:00:00Z","message":{"usage":{"input_tokens":300,"output_tokens":150}},"costUSD":0.03}"#,
        ]
        .join("\n");

        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data,
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            timezone: Some("UTC".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].input_tokens, 400);
        assert_eq!(result[0].total_cost, 0.04);
    }

    #[test]
    fn load_monthly_usage_aggregates_by_month() {
        let fixture = create_fixture();
        let data = vec![
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-01-15T12:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100 } },
                "costUSD": 0.02
            }),
            json!({
                "timestamp": "2024-02-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 150, "output_tokens": 75 } },
                "costUSD": 0.015
            }),
        ];

        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let result = load_monthly_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].month, "2024-02");
        assert_eq!(result[0].input_tokens, 150);
        assert_eq!(result[1].month, "2024-01");
        assert_eq!(result[1].input_tokens, 300);
    }

    #[test]
    fn load_monthly_usage_handles_empty_data() {
        let fixture = create_fixture();
        write_file(fixture.path(), "projects", "");
        let result = load_monthly_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            ..LoadOptions::default()
        })
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn load_monthly_usage_sorts_asc_desc() {
        let fixture = create_fixture();
        let data = vec![
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-03-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-02-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2023-12-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
        ];

        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let desc = load_monthly_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            ..LoadOptions::default()
        })
        .unwrap();
        let desc_months = desc.iter().map(|r| r.month.clone()).collect::<Vec<_>>();
        assert_eq!(
            desc_months,
            vec!["2024-03", "2024-02", "2024-01", "2023-12"]
        );

        let asc = load_monthly_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            order: SortOrder::Asc,
            ..LoadOptions::default()
        })
        .unwrap();
        let asc_months = asc.iter().map(|r| r.month.clone()).collect::<Vec<_>>();
        assert_eq!(asc_months, vec!["2023-12", "2024-01", "2024-02", "2024-03"]);
    }

    #[test]
    fn load_monthly_usage_respects_date_filters() {
        let fixture = create_fixture();
        let data = vec![
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-02-15T12:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100 } },
                "costUSD": 0.02
            }),
            json!({
                "timestamp": "2024-03-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 150, "output_tokens": 75 } },
                "costUSD": 0.015
            }),
        ];
        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let result = load_monthly_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            since: Some("20240110".to_string()),
            until: Some("20240225".to_string()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].month, "2024-02");
        assert_eq!(result[0].input_tokens, 200);
    }

    #[test]
    fn load_monthly_usage_handles_cache_tokens() {
        let fixture = create_fixture();
        let data = vec![
            json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50, "cache_creation_input_tokens": 25, "cache_read_input_tokens": 10 } },
                "costUSD": 0.01
            }),
            json!({
                "timestamp": "2024-01-15T12:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100, "cache_creation_input_tokens": 50, "cache_read_input_tokens": 20 } },
                "costUSD": 0.02
            }),
        ];
        write_file(
            fixture.path(),
            "projects/project1/session1/file.jsonl",
            &data
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let result = load_monthly_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cache_creation_tokens, 75);
        assert_eq!(result[0].cache_read_tokens, 30);
    }

    #[test]
    fn cost_modes_auto_calculate_display() {
        let fixture = create_fixture();
        let data1 = json!({
            "timestamp": "2024-01-01T10:00:00Z",
            "message": { "usage": { "input_tokens": 1000, "output_tokens": 500 } },
            "costUSD": 0.05
        });
        let data2 = json!({
            "timestamp": "2024-01-01T11:00:00Z",
            "message": { "usage": { "input_tokens": 2000, "output_tokens": 1000 }, "model": "claude-4-sonnet-20250514" }
        });
        write_file(
            fixture.path(),
            "projects/test-project/session/usage.jsonl",
            &format!("{}\n{}", data1, data2),
        );

        let auto_result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            mode: CostMode::Auto,
            ..LoadOptions::default()
        })
        .unwrap();
        assert!(auto_result[0].total_cost > 0.05);

        let calculate_result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            mode: CostMode::Calculate,
            ..LoadOptions::default()
        })
        .unwrap();
        assert!(calculate_result[0].total_cost < 1.0);

        let display_result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            mode: CostMode::Display,
            ..LoadOptions::default()
        })
        .unwrap();
        assert_eq!(display_result[0].total_cost, 0.05);
    }

    #[test]
    fn calculate_cost_for_entry_display_mode() {
        let data = UsageData {
            timestamp: Some("2024-01-01T10:00:00Z".to_string()),
            message: Some(UsageMessage {
                usage: Some(UsageMessageUsage {
                    input_tokens: Some(1000),
                    output_tokens: Some(500),
                    cache_creation_input_tokens: Some(200),
                    cache_read_input_tokens: Some(100),
                }),
                model: Some("claude-sonnet-4-20250514".to_string()),
                id: None,
            }),
            cost_usd: Some(0.05),
            request_id: None,
            request: None,
        };
        let fetcher = PricingFetcher::new();
        let result = calculate_cost_for_entry(&data, CostMode::Display, Some(&fetcher));
        assert_eq!(result, 0.05);
    }

    #[test]
    fn calculate_cost_for_entry_calculate_mode() {
        let data = UsageData {
            timestamp: Some("2024-01-01T10:00:00Z".to_string()),
            message: Some(UsageMessage {
                usage: Some(UsageMessageUsage {
                    input_tokens: Some(1000),
                    output_tokens: Some(500),
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                }),
                model: Some("claude-4-sonnet-20250514".to_string()),
                id: None,
            }),
            cost_usd: Some(99.99),
            request_id: None,
            request: None,
        };
        let fetcher = PricingFetcher::new();
        let result = calculate_cost_for_entry(&data, CostMode::Calculate, Some(&fetcher));
        assert!(result > 0.0);
        assert!(result < 1.0);
    }

    #[test]
    fn calculate_cost_for_entry_auto_mode() {
        let data = UsageData {
            timestamp: Some("2024-01-01T10:00:00Z".to_string()),
            message: Some(UsageMessage {
                usage: Some(UsageMessageUsage {
                    input_tokens: Some(1000),
                    output_tokens: Some(500),
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                }),
                model: Some("claude-4-sonnet-20250514".to_string()),
                id: None,
            }),
            cost_usd: Some(0.05),
            request_id: None,
            request: None,
        };
        let fetcher = PricingFetcher::new();
        let result = calculate_cost_for_entry(&data, CostMode::Auto, Some(&fetcher));
        assert_eq!(result, 0.05);
    }

    #[test]
    fn get_earliest_timestamp_extracts_minimum() {
        let fixture = create_fixture();
        let content = vec![
            json!({ "timestamp": "2025-01-15T12:00:00Z", "message": { "usage": {} } }),
            json!({ "timestamp": "2025-01-10T10:00:00Z", "message": { "usage": {} } }),
            json!({ "timestamp": "2025-01-12T11:00:00Z", "message": { "usage": {} } }),
        ]
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        write_file(fixture.path(), "test.jsonl", &content);
        let ts = get_earliest_timestamp(&fixture.path().join("test.jsonl")).unwrap();
        assert_eq!(
            ts,
            DateTime::parse_from_rfc3339("2025-01-10T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn get_earliest_timestamp_handles_missing() {
        let fixture = create_fixture();
        let content = vec![
            json!({ "message": { "usage": {} } }),
            json!({ "data": "no timestamp" }),
        ]
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        write_file(fixture.path(), "test.jsonl", &content);
        let ts = get_earliest_timestamp(&fixture.path().join("test.jsonl"));
        assert!(ts.is_none());
    }

    #[test]
    fn sort_files_by_timestamp_orders_files() {
        let fixture = create_fixture();
        write_file(
            fixture.path(),
            "file1.jsonl",
            &json!({ "timestamp": "2025-01-15T10:00:00Z" }).to_string(),
        );
        write_file(
            fixture.path(),
            "file2.jsonl",
            &json!({ "timestamp": "2025-01-10T10:00:00Z" }).to_string(),
        );
        write_file(
            fixture.path(),
            "file3.jsonl",
            &json!({ "timestamp": "2025-01-12T10:00:00Z" }).to_string(),
        );
        let files = vec![
            fixture.path().join("file1.jsonl"),
            fixture.path().join("file2.jsonl"),
            fixture.path().join("file3.jsonl"),
        ];
        let sorted = sort_files_by_timestamp(files);
        assert_eq!(
            sorted,
            vec![
                fixture.path().join("file2.jsonl"),
                fixture.path().join("file3.jsonl"),
                fixture.path().join("file1.jsonl"),
            ]
        );
    }

    #[test]
    fn sort_files_by_timestamp_places_missing_at_end() {
        let fixture = create_fixture();
        write_file(
            fixture.path(),
            "file1.jsonl",
            &json!({ "timestamp": "2025-01-15T10:00:00Z" }).to_string(),
        );
        write_file(
            fixture.path(),
            "file2.jsonl",
            &json!({ "no_timestamp": true }).to_string(),
        );
        write_file(
            fixture.path(),
            "file3.jsonl",
            &json!({ "timestamp": "2025-01-10T10:00:00Z" }).to_string(),
        );
        let files = vec![
            fixture.path().join("file1.jsonl"),
            fixture.path().join("file2.jsonl"),
            fixture.path().join("file3.jsonl"),
        ];
        let sorted = sort_files_by_timestamp(files);
        assert_eq!(
            sorted,
            vec![
                fixture.path().join("file3.jsonl"),
                fixture.path().join("file1.jsonl"),
                fixture.path().join("file2.jsonl"),
            ]
        );
    }

    #[test]
    fn load_daily_usage_deduplicates_by_message_and_request() {
        let fixture = create_fixture();
        let entry = json!({
            "timestamp": "2025-01-10T10:00:00Z",
            "message": { "id": "msg_123", "usage": { "input_tokens": 100, "output_tokens": 50 } },
            "requestId": "req_456",
            "costUSD": 0.001
        });
        write_file(
            fixture.path(),
            "projects/project1/session1/file1.jsonl",
            &entry.to_string(),
        );
        write_file(
            fixture.path(),
            "projects/project1/session2/file2.jsonl",
            &entry.to_string(),
        );

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            mode: CostMode::Display,
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].date, "2025-01-10");
        assert_eq!(result[0].input_tokens, 100);
        assert_eq!(result[0].output_tokens, 50);
    }

    #[test]
    fn load_daily_usage_keeps_older_entry_for_dedup() {
        let fixture = create_fixture();
        let newer = json!({
            "timestamp": "2025-01-15T10:00:00Z",
            "message": { "id": "msg_123", "usage": { "input_tokens": 200, "output_tokens": 100 } },
            "requestId": "req_456",
            "costUSD": 0.002
        });
        let older = json!({
            "timestamp": "2025-01-10T10:00:00Z",
            "message": { "id": "msg_123", "usage": { "input_tokens": 100, "output_tokens": 50 } },
            "requestId": "req_456",
            "costUSD": 0.001
        });
        write_file(fixture.path(), "projects/newer.jsonl", &newer.to_string());
        write_file(fixture.path(), "projects/older.jsonl", &older.to_string());

        let result = load_daily_usage_data(LoadOptions {
            claude_path: Some(fixture.path().to_path_buf()),
            mode: CostMode::Display,
            ..LoadOptions::default()
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].date, "2025-01-10");
        assert_eq!(result[0].input_tokens, 100);
        assert_eq!(result[0].output_tokens, 50);
    }

    #[test]
    fn process_jsonl_file_by_line_skips_empty() {
        let fixture = create_fixture();
        write_file(
            fixture.path(),
            "test.jsonl",
            "{\"line\": 1}\n\n{\"line\": 2}\n  \n{\"line\": 3}\n",
        );
        let mut lines = Vec::new();
        process_jsonl_file_by_line(&fixture.path().join("test.jsonl"), |line, _| {
            lines.push(line.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            lines,
            vec!["{\"line\": 1}", "{\"line\": 2}", "{\"line\": 3}"]
        );
    }

    #[test]
    fn process_jsonl_file_by_line_reports_line_numbers() {
        let fixture = create_fixture();
        write_file(
            fixture.path(),
            "test.jsonl",
            "{\"line\": 1}\n{\"line\": 2}\n{\"line\": 3}\n",
        );
        let mut lines = Vec::new();
        process_jsonl_file_by_line(&fixture.path().join("test.jsonl"), |line, number| {
            lines.push((line.to_string(), number));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("{\"line\": 1}".to_string(), 1),
                ("{\"line\": 2}".to_string(), 2),
                ("{\"line\": 3}".to_string(), 3),
            ]
        );
    }

    #[test]
    fn process_jsonl_file_by_line_errors_on_missing_file() {
        let result =
            process_jsonl_file_by_line(Path::new("/nonexistent/file.jsonl"), |_, _| Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn glob_usage_files_handles_multiple_paths() {
        let fixture = create_fixture();
        write_file(
            fixture.path(),
            "path1/projects/project1/session1/usage.jsonl",
            "data1",
        );
        write_file(
            fixture.path(),
            "path2/projects/project2/session2/usage.jsonl",
            "data2",
        );
        write_file(
            fixture.path(),
            "path3/projects/project3/session3/usage.jsonl",
            "data3",
        );

        let paths = vec![
            fixture.path().join("path1"),
            fixture.path().join("path2"),
            fixture.path().join("path3"),
        ];

        let results = glob_usage_files(&paths);
        assert_eq!(results.len(), 3);
        assert!(
            results
                .iter()
                .any(|r| r.file.to_string_lossy().contains("project1"))
        );
        assert!(
            results
                .iter()
                .any(|r| r.file.to_string_lossy().contains("project2"))
        );
        assert!(
            results
                .iter()
                .any(|r| r.file.to_string_lossy().contains("project3"))
        );
    }

    #[test]
    fn glob_usage_files_ignores_missing_paths() {
        let fixture = create_fixture();
        write_file(
            fixture.path(),
            "valid/projects/project1/session1/usage.jsonl",
            "data1",
        );

        let paths = vec![
            fixture.path().join("valid"),
            fixture.path().join("nonexistent"),
        ];
        let results = glob_usage_files(&paths);
        assert_eq!(results.len(), 1);
        assert!(results[0].file.to_string_lossy().contains("project1"));
    }

    #[test]
    fn glob_usage_files_returns_empty_when_no_files() {
        let fixture = create_fixture();
        write_file(fixture.path(), "empty/projects", "");
        let paths = vec![fixture.path().join("empty")];
        let results = glob_usage_files(&paths);
        assert!(results.is_empty());
    }

    #[test]
    fn get_claude_paths_from_env() {
        let fixture1 = create_fixture();
        let fixture2 = create_fixture();
        write_file(
            fixture1.path(),
            "projects/project1/session/usage.jsonl",
            "data1",
        );
        write_file(
            fixture2.path(),
            "projects/project2/session/usage.jsonl",
            "data2",
        );

        unsafe {
            std::env::set_var(
                CLAUDE_CONFIG_DIR_ENV,
                format!(
                    "{},{}",
                    fixture1.path().display(),
                    fixture2.path().display()
                ),
            );
        }
        let paths = get_claude_paths().unwrap();
        assert!(
            paths
                .iter()
                .any(|p| p == &fixture1.path().canonicalize().unwrap())
        );
        assert!(
            paths
                .iter()
                .any(|p| p == &fixture2.path().canonicalize().unwrap())
        );
        unsafe {
            std::env::remove_var(CLAUDE_CONFIG_DIR_ENV);
        }
    }

    #[test]
    fn load_daily_usage_aggregates_multiple_paths() {
        let fixture1 = create_fixture();
        let fixture2 = create_fixture();
        write_file(
            fixture1.path(),
            "projects/project1/session1/usage.jsonl",
            &json!({
                "timestamp": "2024-01-01T12:00:00Z",
                "message": { "usage": { "input_tokens": 100, "output_tokens": 50 } },
                "costUSD": 0.01
            })
            .to_string(),
        );
        write_file(
            fixture2.path(),
            "projects/project2/session2/usage.jsonl",
            &json!({
                "timestamp": "2024-01-01T13:00:00Z",
                "message": { "usage": { "input_tokens": 200, "output_tokens": 100 } },
                "costUSD": 0.02
            })
            .to_string(),
        );
        unsafe {
            std::env::set_var(
                CLAUDE_CONFIG_DIR_ENV,
                format!(
                    "{},{}",
                    fixture1.path().display(),
                    fixture2.path().display()
                ),
            );
        }
        let result = load_daily_usage_data(LoadOptions::default()).unwrap();
        let target = result.iter().find(|day| day.date == "2024-01-01").unwrap();
        assert_eq!(target.input_tokens, 300);
        assert_eq!(target.output_tokens, 150);
        assert_eq!(target.total_cost, 0.03);
        unsafe {
            std::env::remove_var(CLAUDE_CONFIG_DIR_ENV);
        }
    }
}
