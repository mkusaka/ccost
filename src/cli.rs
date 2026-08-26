use crate::data_loader::{
    DailyUsage, LoadOptions, ModelBreakdown, MonthlyUsage, UsageTotals, calculate_totals_daily,
    calculate_totals_monthly, group_daily_by_project, load_daily_usage_data,
    load_monthly_usage_data,
};
use crate::pricing::CostMode;
use crate::table::{
    ModelBreakdownRow, TableMode, TokenFormat, UsageDataRow, build_breakdown_rows,
    build_totals_row, build_usage_row,
};
use crate::time_utils::{Granularity, SortOrder, format_date_compact, format_hour_compact};
use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand, ValueEnum};
use comfy_table::Table;
use serde::Serialize;
use std::io::Write;
use terminal_size::terminal_size;

fn write_output<W: Write>(writer: &mut W, args: std::fmt::Arguments<'_>) -> Result<()> {
    if let Err(e) = writer.write_fmt(args) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            return Ok(());
        }
        return Err(e.into());
    }
    if let Err(e) = writer.write_all(b"\n") {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            return Ok(());
        }
        return Err(e.into());
    }
    Ok(())
}

macro_rules! println_safe {
    ($fmt:literal $(, $arg:expr)*) => {
        write_output(&mut std::io::stdout().lock(), format_args!($fmt $(, $arg)*))?
    };
}

macro_rules! eprintln_safe {
    ($fmt:literal $(, $arg:expr)*) => {
        write_output(&mut std::io::stderr().lock(), format_args!($fmt $(, $arg)*))?
    };
}

#[derive(Parser)]
#[command(
    name = "ccost",
    version,
    about = "Claude Code / Codex / Pi / OMP / OpenCode / Devin usage report (hourly/daily/monthly)"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Hourly(DailyArgs),
    Daily(DailyArgs),
    Monthly(MonthlyArgs),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Agent {
    Codex,
    Claudecode,
    Pi,
    Omp,
    Opencode,
    Devin,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AgentFlags {
    codex: bool,
    claudecode: bool,
    pi: bool,
    omp: bool,
    opencode: bool,
    devin: bool,
}

impl AgentFlags {
    fn all() -> Self {
        Self {
            codex: true,
            claudecode: true,
            pi: true,
            omp: true,
            opencode: true,
            devin: true,
        }
    }
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
    #[arg(long, help = "Format table token counts with K, M, or B suffixes")]
    kmb: bool,
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "all",
        help = "Usage data source: all, codex, claudecode, pi, omp, opencode, or devin"
    )]
    agent: Vec<Agent>,
}

impl CommonArgs {
    fn agent_flags(&self) -> AgentFlags {
        if self.agent.is_empty() || self.agent.contains(&Agent::All) {
            return AgentFlags::all();
        }

        AgentFlags {
            codex: self.agent.contains(&Agent::Codex),
            claudecode: self.agent.contains(&Agent::Claudecode),
            pi: self.agent.contains(&Agent::Pi),
            omp: self.agent.contains(&Agent::Omp),
            opencode: self.agent.contains(&Agent::Opencode),
            devin: self.agent.contains(&Agent::Devin),
        }
    }
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
struct DailyMetadataOutput {
    agents: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyEntryOutput {
    agent: String,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    input_tokens: u64,
    metadata: DailyMetadataOutput,
    model_breakdowns: Vec<ModelBreakdownOutput>,
    models_used: Vec<String>,
    output_tokens: u64,
    period: String,
    total_cost: f64,
    total_tokens: u64,
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
    let mut args = std::env::args_os().collect::<Vec<_>>();
    let needs_default = match args.get(1).and_then(|arg| arg.to_str()) {
        None => true,
        Some(arg) => {
            if arg.starts_with('-') {
                !matches!(arg, "-h" | "--help" | "-V" | "--version")
            } else {
                false
            }
        }
    };
    if needs_default {
        args.insert(1, std::ffi::OsString::from("daily"));
    }
    let cli = Cli::parse_from(args);
    match cli.command {
        Command::Hourly(args) => run_daily(args, Granularity::Hour),
        Command::Daily(args) => run_daily(args, Granularity::Day),
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
    let agents = args.agent_flags();
    Ok(LoadOptions {
        mode: parse_cost_mode(&args.mode)?,
        order: parse_sort_order(&args.order)?,
        offline: args.offline,
        codex: agents.codex,
        claudecode: agents.claudecode,
        pi: agents.pi,
        omp: agents.omp,
        opencode: agents.opencode,
        devin: agents.devin,
        since: args.since.clone(),
        until: args.until.clone(),
        timezone: args.timezone.clone(),
        ..LoadOptions::default()
    })
}

fn run_daily(args: DailyArgs, granularity: Granularity) -> Result<()> {
    let mut options = common_options(&args.common)?;
    options.granularity = granularity;
    options.group_by_project = args.instances;
    options.project = args.project.clone();

    let daily = load_daily_usage_data(options)?;
    if daily.is_empty() {
        if args.common.json {
            println_safe!("[]");
        } else {
            eprintln_safe!("No usage data found.");
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
            println_safe!("{}", serde_json::to_string_pretty(&json)?);
        } else {
            let key = match granularity {
                Granularity::Day => "daily",
                Granularity::Hour => "hourly",
            };
            let json = serde_json::json!({
                key: daily.into_iter().map(|entry| daily_entry_output(entry, true)).collect::<Vec<_>>(),
                "totals": totals_output(totals)
            });
            println_safe!("{}", serde_json::to_string_pretty(&json)?);
        }
        return Ok(());
    }

    let (period_label, column_label) = match granularity {
        Granularity::Day => ("Daily", "Date"),
        Granularity::Hour => ("Hourly", "Hour"),
    };
    println_safe!("{}", report_title(period_label, &args.common));

    let mode = table_mode(args.common.compact);
    let token_format = token_format(args.common.kmb);
    let mut table = usage_table(column_label, mode);

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
                let first_col = format_period_column(&entry.date, granularity, &args.common);
                let row = build_usage_row(
                    &first_col,
                    &usage_row_from_daily(&entry),
                    mode,
                    token_format,
                );
                table.add_row(row);
                if args.common.breakdown {
                    let breakdowns = breakdown_rows_from_breakdowns(&entry.model_breakdowns);
                    for breakdown in build_breakdown_rows(&breakdowns, mode, token_format) {
                        table.add_row(breakdown);
                    }
                }
            }
            first = false;
        }
    } else {
        for entry in &daily {
            let first_col = format_period_column(&entry.date, granularity, &args.common);
            let row = build_usage_row(&first_col, &usage_row_from_daily(entry), mode, token_format);
            table.add_row(row);
            if args.common.breakdown {
                let breakdowns = breakdown_rows_from_breakdowns(&entry.model_breakdowns);
                for breakdown in build_breakdown_rows(&breakdowns, mode, token_format) {
                    table.add_row(breakdown);
                }
            }
        }
    }

    table.add_row(build_totals_row(
        &usage_row_from_totals(&totals),
        mode,
        token_format,
    ));
    println_safe!("{table}");

    if matches!(mode, TableMode::Compact) {
        println_safe!("\nRunning in Compact Mode");
        println_safe!("Expand terminal width to see cache metrics and total tokens");
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
            println_safe!("{}", serde_json::to_string_pretty(&empty)?);
        } else {
            eprintln_safe!("No usage data found.");
        }
        return Ok(());
    }

    let totals = calculate_totals_monthly(&monthly);

    if args.common.json {
        let json = serde_json::json!({
            "monthly": monthly.into_iter().map(monthly_entry_output).collect::<Vec<_>>(),
            "totals": totals_output(totals)
        });
        println_safe!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    println_safe!("{}", report_title("Monthly", &args.common));

    let mode = table_mode(args.common.compact);
    let token_format = token_format(args.common.kmb);
    let mut table = usage_table("Month", mode);

    for entry in &monthly {
        let row = build_usage_row(
            &entry.month,
            &usage_row_from_monthly(entry),
            mode,
            token_format,
        );
        table.add_row(row);
        if args.common.breakdown {
            let breakdowns = breakdown_rows_from_breakdowns(&entry.model_breakdowns);
            for breakdown in build_breakdown_rows(&breakdowns, mode, token_format) {
                table.add_row(breakdown);
            }
        }
    }

    table.add_row(build_totals_row(
        &usage_row_from_totals(&totals),
        mode,
        token_format,
    ));
    println_safe!("{table}");

    if matches!(mode, TableMode::Compact) {
        println_safe!("\nRunning in Compact Mode");
        println_safe!("Expand terminal width to see cache metrics and total tokens");
    }

    Ok(())
}

fn format_period_column(period: &str, granularity: Granularity, args: &CommonArgs) -> String {
    match granularity {
        Granularity::Day => format_date_compact(period, args.timezone.as_deref())
            .unwrap_or_else(|| period.to_string()),
        Granularity::Hour => format_hour_compact(period).unwrap_or_else(|| period.to_string()),
    }
}

fn terminal_width() -> u16 {
    terminal_size().map(|(width, _)| width.0).unwrap_or(120)
}

fn table_mode(force_compact: bool) -> TableMode {
    if force_compact || terminal_width() < 160 {
        TableMode::Compact
    } else {
        TableMode::Full
    }
}

fn token_format(kmb: bool) -> TokenFormat {
    if kmb {
        TokenFormat::HumanReadable
    } else {
        TokenFormat::Exact
    }
}

fn report_title(period: &str, args: &CommonArgs) -> String {
    let agents = args.agent_flags();
    let mut sources = Vec::new();
    if agents.claudecode {
        sources.push("Claude Code");
    }
    if agents.codex {
        sources.push("Codex");
    }
    if agents.pi {
        sources.push("Pi");
    }
    if agents.omp {
        sources.push("OMP");
    }
    if agents.opencode {
        sources.push("OpenCode");
    }
    if agents.devin {
        sources.push("Devin");
    }
    let source = if sources.is_empty() {
        "No Source".to_string()
    } else {
        sources.join(" + ")
    };
    format!("{source} Token Usage Report - {period}")
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
    table.set_width(terminal_width());
    table.set_header(headers);
    UsageTable { table, mode }
}

fn usage_row_from_daily(entry: &DailyUsage) -> UsageDataRow {
    UsageDataRow {
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        total_tokens: entry.total_tokens,
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
        total_tokens: entry.total_tokens,
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
        total_tokens: totals.total_tokens(),
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
            total_tokens: b.total_tokens,
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
    DailyEntryOutput {
        agent: "all".to_string(),
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        input_tokens: entry.input_tokens,
        metadata: DailyMetadataOutput { agents: vec![] },
        model_breakdowns: entry
            .model_breakdowns
            .into_iter()
            .map(model_breakdown_output)
            .collect(),
        models_used: entry.models_used,
        output_tokens: entry.output_tokens,
        period: entry.date,
        total_cost: entry.total_cost,
        total_tokens: entry.total_tokens,
        project: if include_project { entry.project } else { None },
    }
}

fn monthly_entry_output(entry: MonthlyUsage) -> MonthlyEntryOutput {
    MonthlyEntryOutput {
        month: entry.month,
        input_tokens: entry.input_tokens,
        output_tokens: entry.output_tokens,
        cache_creation_tokens: entry.cache_creation_tokens,
        cache_read_tokens: entry.cache_read_tokens,
        total_tokens: entry.total_tokens,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_daily_common(args: &[&str]) -> CommonArgs {
        let parsed =
            Cli::try_parse_from(["ccost", "daily"].into_iter().chain(args.iter().copied()))
                .unwrap();
        match parsed.command {
            Command::Daily(args) => args.common,
            Command::Hourly(_) | Command::Monthly(_) => unreachable!(),
        }
    }

    #[test]
    fn agent_defaults_to_all_sources() {
        let common = parse_daily_common(&[]);

        assert_eq!(common.agent_flags(), AgentFlags::all());
        assert_eq!(
            report_title("Daily", &common),
            "Claude Code + Codex + Pi + OMP + OpenCode + Devin Token Usage Report - Daily"
        );
    }

    #[test]
    fn agent_accepts_single_source() {
        let common = parse_daily_common(&["--agent=codex"]);

        assert_eq!(
            common.agent_flags(),
            AgentFlags {
                codex: true,
                claudecode: false,
                pi: false,
                omp: false,
                opencode: false,
                devin: false,
            }
        );
        assert_eq!(
            report_title("Daily", &common),
            "Codex Token Usage Report - Daily"
        );
    }

    #[test]
    fn agent_accepts_comma_separated_sources() {
        let common = parse_daily_common(&["--agent=codex,opencode"]);

        assert_eq!(
            common.agent_flags(),
            AgentFlags {
                codex: true,
                claudecode: false,
                pi: false,
                omp: false,
                opencode: true,
                devin: false,
            }
        );
        assert_eq!(
            report_title("Daily", &common),
            "Codex + OpenCode Token Usage Report - Daily"
        );
    }

    #[test]
    fn agent_accepts_pi_and_omp_sources() {
        let common = parse_daily_common(&["--agent=pi,omp"]);

        assert_eq!(
            report_title("Daily", &common),
            "Pi + OMP Token Usage Report - Daily"
        );
    }

    #[test]
    fn removed_source_boolean_flags_are_rejected() {
        let result = Cli::try_parse_from(["ccost", "daily", "--codex=false"]);

        assert!(result.is_err());
    }

    #[test]
    fn kmb_is_opt_in() {
        assert!(!parse_daily_common(&[]).kmb);
        assert!(parse_daily_common(&["--kmb"]).kmb);

        let parsed = Cli::try_parse_from(["ccost", "monthly", "--json", "--kmb"]).unwrap();
        let Command::Monthly(args) = parsed.command else {
            unreachable!();
        };
        assert!(args.common.json);
        assert!(args.common.kmb);
    }

    #[test]
    fn json_totals_keep_raw_numeric_tokens() {
        let output = totals_output(UsageTotals {
            input_tokens: 69_960_297_352,
            total_tokens: 69_960_297_352,
            ..UsageTotals::default()
        });
        let json = serde_json::to_value(output).unwrap();

        assert_eq!(json["inputTokens"].as_u64(), Some(69_960_297_352));
        assert_eq!(json["totalTokens"].as_u64(), Some(69_960_297_352));
    }

    struct BrokenPipe;

    impl Write for BrokenPipe {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "broken pipe",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_output_succeeds_and_ignores_broken_pipe() {
        let mut buf = Vec::new();
        assert!(write_output(&mut buf, format_args!("hello")).is_ok());
        assert_eq!(buf, b"hello\n");

        let mut broken = BrokenPipe;
        assert!(write_output(&mut broken, format_args!("hello")).is_ok());
    }
}
