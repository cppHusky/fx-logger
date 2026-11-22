mod cache;
mod config;
mod fetch;
mod output;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};

use crate::config::HistoryDirection;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum OutputStyle {
    Terminal,
    Waybar,
    #[value(name = "ntfy")]
    Ntfy,
}

#[derive(Parser, Debug)]
#[command(name = "fx-logger", about = "FX rate logger backed by PostgreSQL")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(
        short = 'b',
        long = "base",
        help = "the base currency you want, Accepts ISO 4217 codes"
    )]
    base: Option<String>,

    #[arg(
        short = 'o',
        long = "output",
        value_delimiter = ',',
        help = "filter the currency codes you want to output"
    )]
    output: Option<Vec<String>>,

    #[arg(short = 'c', long = "config", help = "the config file you want to use")]
    config: Option<String>,

    #[arg(
        short = 's',
        long = "output-style",
        value_enum,
        help = "select the output format"
    )]
    output_style: Option<OutputStyle>,

    #[arg(
        long = "history-direction",
        visible_alias = "history",
        value_enum,
        help = "watch a high or low historical rank"
    )]
    history_direction: Option<HistoryDirection>,

    #[arg(
        long = "history-percent",
        visible_alias = "history-percentile",
        help = "percent of history the current value must exceed in the selected direction"
    )]
    history_percent: Option<f64>,

    #[arg(long = "history-days", help = "number of previous days to compare")]
    history_days: Option<u64>,

    #[arg(
        long = "suppress-equal",
        help = "suppress ntfy alerts when the latest three pair rates are equal"
    )]
    suppress_equal: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "get the latest stored rates. do not update cash rate")]
    Get {
        #[arg(short = 'b', long = "base")]
        base: Option<String>,

        #[arg(short = 'o', long = "output", value_delimiter = ',')]
        output: Option<Vec<String>>,

        #[arg(short = 'c', long = "config")]
        config: Option<String>,

        #[arg(short = 's', long = "output-style", value_enum)]
        output_style: Option<OutputStyle>,

        #[arg(long = "history-direction", visible_alias = "history", value_enum)]
        history_direction: Option<HistoryDirection>,

        #[arg(long = "history-percent", visible_alias = "history-percentile")]
        history_percent: Option<f64>,

        #[arg(long = "history-days")]
        history_days: Option<u64>,

        #[arg(
            long = "suppress-equal",
            help = "suppress ntfy alerts when the latest three pair rates are equal"
        )]
        suppress_equal: bool,
    },

    #[command(about = "fetch current cash rate from API and store in PostgreSQL")]
    Fetch,
}

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Fetch) => {
            let app_id = std::env::var("OPEN_EXCHANGE_RATE_APP_ID")
                .context("OPEN_EXCHANGE_RATE_APP_ID not set in environment or .env file")?;
            fetch::fetch_and_cache(&app_id)?;
        }
        Some(Command::Get {
            base,
            output,
            config,
            output_style,
            history_direction,
            history_percent,
            history_days,
            suppress_equal,
        }) => {
            let cfg = config::Config::load(
                first_some(base.clone(), cli.base.clone()),
                first_some(output.clone(), cli.output.clone()),
                first_some(config.as_deref(), cli.config.as_deref()),
                first_some(*history_days, cli.history_days),
                first_some(*history_percent, cli.history_percent),
                first_some(*history_direction, cli.history_direction),
            )?;
            run_get(
                &cfg,
                first_some(*output_style, cli.output_style).unwrap_or(OutputStyle::Terminal),
                *suppress_equal || cli.suppress_equal,
            )?;
        }
        None => {
            let cfg = config::Config::load(
                cli.base.clone(),
                cli.output.clone(),
                cli.config.as_deref(),
                cli.history_days,
                cli.history_percent,
                cli.history_direction,
            )?;
            run_get(
                &cfg,
                cli.output_style.unwrap_or(OutputStyle::Terminal),
                cli.suppress_equal,
            )?;
        }
    }

    Ok(())
}

fn first_some<T>(a: Option<T>, b: Option<T>) -> Option<T> {
    a.or(b)
}

fn run_get(cfg: &config::Config, style: OutputStyle, suppress_equal: bool) -> anyhow::Result<()> {
    let history = cache::get_history()?;
    let entry = history
        .last()
        .cloned()
        .context("no cached data found. run 'fetch' first")?;

    match style {
        OutputStyle::Ntfy => {
            print!(
                "{}",
                output::format_notification(
                    &entry,
                    &history,
                    &cfg.base,
                    &cfg.output,
                    cfg.history.as_ref(),
                    suppress_equal,
                )?
            );
        }
        OutputStyle::Waybar => {
            print!(
                "{}",
                output::format_waybar(
                    &entry,
                    &history,
                    &cfg.base,
                    &cfg.output,
                    cfg.history.as_ref(),
                )?
            );
        }
        OutputStyle::Terminal => {
            print!(
                "{}",
                output::format_terminal(
                    &entry,
                    &history,
                    &cfg.base,
                    &cfg.output,
                    cfg.history.as_ref(),
                )?
            );
        }
    }

    Ok(())
}
