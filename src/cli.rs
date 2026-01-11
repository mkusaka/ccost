use crate::data_loader::{
    DailyUsage, LoadOptions, ModelBreakdown, MonthlyUsage, UsageTotals, calculate_totals_daily,
    calculate_totals_monthly, group_daily_by_project, load_daily_usage_data,
    load_monthly_usage_data,
};
use crate::pricing::CostMode;
use crate::table::{
    ModelBreakdownRow, TableMode, UsageDataRow, build_breakdown_rows, build_totals_row,
    build_usage_row,
};
use crate::time_utils::{SortOrder, format_date_compact};
use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};
use comfy_table::Table;
use serde::Serialize;
use terminal_size::terminal_size;

#[derive(Parser)]
#[command(
    name = "ccost",
    version,
    about = "Claude Code usage report (daily/monthly)"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Daily(DailyArgs),
    Monthly(MonthlyArgs),
}

#[derive(Args, Clone)]
pub struct CommonArgs {
    #[arg(short, long, help = "Filter from date (YYYYMMDD format)")]
    since: Option<String>,
    #[arg(short, long, help = "Filter until date (YYYYMMDD format)")]
    until: Option<String>,
    #[arg(short = 'j', long, help = "Output in JSON format")]
    json: bool,
    #[arg(short, long, default_value = "auto", help = "Cost calculation mode")]
    mode: String,
    #[arg(short, long, default_value = "asc", help = "Sort order: asc or desc")]
    order: String,
    #[arg(short, long, help = "Show per-model cost breakdown")]
    breakdown: bool,
    #[arg(
        short = 'O',
        long,
        default_value_t = true,
        help = "Use offline pricing data"
    )]
    offline: bool,
    #[arg(short, long, help = "Timezone for date grouping")]
    timezone: Option<String>,
    #[arg(long, default_value_t = false, help = "Force compact mode")]
    compact: bool,
}

#[derive(Args, Clone)]
pub struct DailyArgs {
    #[command(flatten)]
    common: CommonArgs,
    #[arg(short = 'i', long, default_value_t = false, help = "Group by project")]
    instances: bool,
    #[arg(short = 'p', long, help = "Filter to specific project name")]
    project: Option<String>,
}

#[derive(Args, Clone)]
pub struct MonthlyArgs {
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotalsOutput {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    total_tokens: u64,
    total_cost: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyEntryOutput {
    date: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    total_tokens: u64,
    total_cost: f64,
    models_used: Vec<String>,
    model_breakdowns: Vec<ModelBreakdownOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonthlyEntryOutput {
    month: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    total_tokens: u64,
    total_cost: f64,
    models_used: Vec<String>,
    model_breakdowns: Vec<ModelBreakdownOutput>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelBreakdownOutput {
    model_name: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    cost: f64,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daily(args) => run_daily(args),
        Command::Monthly(args) => run_monthly(args),
    }
}

fn parse_cost_mode(value: &str) -> Result<CostMode> {
    value
        .parse::<CostMode>()
        .map_err(|_| anyhow!("Invalid cost mode: {value}"))
}

fn parse_sort_order(value: &str) -> Result<SortOrder> {
    value
        .parse::<SortOrder>()
        .map_err(|_| anyhow!("Invalid sort order: {value}"))
}

fn common_options(args: &CommonArgs) -> Result<LoadOptions> {
    Ok(LoadOptions {
        mode: parse_cost_mode(&args.mode)?,
        order: parse_sort_order(&args.order)?,
        offline: args.offline,
        since: args.since.clone(),
        until: args.until.clone(),
        timezone: args.timezone.clone(),
        ..LoadOptions::default()
    })
}

fn run_daily(args: DailyArgs) -> Result<()> {
    let mut options = common_options(&args.common)?;
    options.group_by_project = args.instances;
    options.project = args.project.clone();

    let daily = load_daily_usage_data(options)?;
    if daily.is_empty() {
        if args.common.json {
            println!("[]");
        } else {
            eprintln!("No Claude usage data found.");
        }
        return Ok(());
    }

    let totals = calculate_totals_daily(&daily);

    if args.common.json {
        if args.instances && daily.iter().any(|d| d.project.is_some()) {
            let grouped = group_daily_by_project(&daily);
            let mut projects_output = std::collections::HashMap::new();
            for (project, entries) in grouped {
                let mapped = entries
                    .into_iter()
                    .map(|entry| daily_entry_output(entry, false))
                    .collect::<Vec<_>>();
                projects_output.insert(project, mapped);
            }
            let json = serde_json::json!({
                "projects": projects_output,
                "totals": totals_output(totals)
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        } else {
            let json = serde_json::json!({
                "daily": daily.into_iter().map(|entry| daily_entry_output(entry, true)).collect::<Vec<_>>(),
                "totals": totals_output(totals)
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        return Ok(());
    }

    println!("Claude Code Token Usage Report - Daily");

    let mode = table_mode(args.common.compact);
    let mut table = usage_table("Date", mode);

    if args.instances && daily.iter().any(|d| d.project.is_some()) {
        let grouped = group_daily_by_project(&daily);
        let mut first = true;
        for (project, entries) in grouped {
            if !first {
                table.add_row(vec![String::new(); table.column_count()]);
            }
            let mut header_row = vec![String::new(); table.column_count()];
            header_row[0] = format!("Project: {project}");
            table.add_row(header_row);
            for entry in entries {
                let first_col = format_date_compact(&entry.date, args.common.timezone.as_deref())
                    .unwrap_or(entry.date.clone());
                let row = build_usage_row(&first_col, &usage_row_from_daily(&entry), mode);
                table.add_row(row);
                if args.common.breakdown {
                    let breakdowns = breakdown_rows_from_breakdowns(&entry.model_breakdowns);
                    for breakdown in build_breakdown_rows(&breakdowns, mode) {
                        table.add_row(breakdown);
                    }
                }
            }
            first = false;
        }
    } else {
        for entry in &daily {
            let first_col = format_date_compact(&entry.date, args.common.timezone.as_deref())
                .unwrap_or(entry.date.clone());
            let row = build_usage_row(&first_col, &usage_row_from_daily(entry), mode);
            table.add_row(row);
            if args.common.breakdown {
                let breakdowns = breakdown_rows_from_breakdowns(&entry.model_breakdowns);
                for breakdown in build_breakdown_rows(&breakdowns, mode) {
                    table.add_row(breakdown);
                }
            }
        }
    }

    table.add_row(build_totals_row(&usage_row_from_totals(&totals), mode));
    println!("{table}");

    if matches!(mode, TableMode::Compact) {
        println!("\nRunning in Compact Mode");
        println!("Expand terminal width to see cache metrics and total tokens");
    }

    Ok(())
}

fn run_monthly(args: MonthlyArgs) -> Result<()> {
    let options = common_options(&args.common)?;
    let monthly = load_monthly_usage_data(options)?;
    if monthly.is_empty() {
        if args.common.json {
            let empty = serde_json::json!({
                "monthly": [],
                "totals": totals_output(UsageTotals::default())
            });
            println!("{}", serde_json::to_string_pretty(&empty)?);
        } else {
            eprintln!("No Claude usage data found.");
        }
        return Ok(());
    }

    let totals = calculate_totals_monthly(&monthly);

    if args.common.json {
        let json = serde_json::json!({
            "monthly": monthly.into_iter().map(monthly_entry_output).collect::<Vec<_>>(),
            "totals": totals_output(totals)
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    println!("Claude Code Token Usage Report - Monthly");

    let mode = table_mode(args.common.compact);
    let mut table = usage_table("Month", mode);

    for entry in &monthly {
        let row = build_usage_row(&entry.month, &usage_row_from_monthly(entry), mode);
        table.add_row(row);
        if args.common.breakdown {
            let breakdowns = breakdown_rows_from_breakdowns(&entry.model_breakdowns);
            for breakdown in build_breakdown_rows(&breakdowns, mode) {
                table.add_row(breakdown);
            }
        }
    }

    table.add_row(build_totals_row(&usage_row_from_totals(&totals), mode));
    println!("{table}");

    if matches!(mode, TableMode::Compact) {
        println!("\nRunning in Compact Mode");
        println!("Expand terminal width to see cache metrics and total tokens");
    }

    Ok(())
}

fn table_mode(force_compact: bool) -> TableMode {
    if force_compact {
        return TableMode::Compact;
    }
    let width = terminal_size().map(|(w, _)| w.0 as usize).unwrap_or(120);
    if width < 100 {
        TableMode::Compact
    } else {
        TableMode::Full
    }
}

fn usage_table(first_column: &str, mode: TableMode) -> UsageTable {
    let headers = match mode {
        TableMode::Full => vec![
            first_column,
            "Models",
            "Input",
            "Output",
            "Cache Create",
            "Cache Read",
            "Total Tokens",
            "Cost (USD)",
        ],
        TableMode::Compact => vec![first_column, "Models", "Input", "Output", "Cost (USD)"],
    };

    let mut table = Table::new();
    table.load_preset("││──╞═╪╡│─┼├┤┬┴┌┐└┘");
    table.set_header(headers);
    UsageTable { table, mode }
}

fn usage_row_from_daily(entry: &DailyUsage) -> UsageDataRow {
    UsageDataRow {
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        total_cost: entry.total_cost,
        models_used: entry.models_used.clone(),
    }
}

fn usage_row_from_monthly(entry: &MonthlyUsage) -> UsageDataRow {
    UsageDataRow {
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        total_cost: entry.total_cost,
        models_used: entry.models_used.clone(),
    }
}

fn usage_row_from_totals(totals: &UsageTotals) -> UsageDataRow {
    UsageDataRow {
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_creation_tokens: totals.cache_creation_tokens,
        cache_read_tokens: totals.cache_read_tokens,
        total_cost: totals.total_cost,
        models_used: Vec::new(),
    }
}

fn breakdown_rows_from_breakdowns(breakdowns: &[ModelBreakdown]) -> Vec<ModelBreakdownRow> {
    breakdowns
        .iter()
        .map(|b| ModelBreakdownRow {
            model_name: b.model_name.clone(),
            input_tokens: b.input_tokens,
            output_tokens: b.output_tokens,
            cache_creation_tokens: b.cache_creation_tokens,
            cache_read_tokens: b.cache_read_tokens,
            cost: b.cost,
        })
        .collect()
}

fn totals_output(totals: UsageTotals) -> TotalsOutput {
    TotalsOutput {
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_creation_tokens: totals.cache_creation_tokens,
        cache_read_tokens: totals.cache_read_tokens,
        total_tokens: totals.total_tokens(),
        total_cost: totals.total_cost,
    }
}

fn daily_entry_output(entry: DailyUsage, include_project: bool) -> DailyEntryOutput {
    let total_tokens = entry.input_tokens
        + entry.output_tokens
        + entry.cache_creation_tokens
        + entry.cache_read_tokens;
    DailyEntryOutput {
        date: entry.date,
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        total_tokens,
        total_cost: entry.total_cost,
        models_used: entry.models_used,
        model_breakdowns: entry
            .model_breakdowns
            .into_iter()
            .map(model_breakdown_output)
            .collect(),
        project: if include_project { entry.project } else { None },
    }
}

fn monthly_entry_output(entry: MonthlyUsage) -> MonthlyEntryOutput {
    let total_tokens = entry.input_tokens
        + entry.output_tokens
        + entry.cache_creation_tokens
        + entry.cache_read_tokens;
    MonthlyEntryOutput {
        month: entry.month,
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        total_tokens,
        total_cost: entry.total_cost,
        models_used: entry.models_used,
        model_breakdowns: entry
            .model_breakdowns
            .into_iter()
            .map(model_breakdown_output)
            .collect(),
    }
}

fn model_breakdown_output(entry: ModelBreakdown) -> ModelBreakdownOutput {
    ModelBreakdownOutput {
        model_name: entry.model_name,
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        cost: entry.cost,
    }
}

struct UsageTable {
    table: Table,
    mode: TableMode,
}

impl UsageTable {
    fn add_row(&mut self, row: Vec<String>) {
        self.table.add_row(row);
    }

    fn column_count(&self) -> usize {
        match self.mode {
            TableMode::Full => 8,
            TableMode::Compact => 5,
        }
    }
}

impl std::fmt::Display for UsageTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.table)
    }
}
