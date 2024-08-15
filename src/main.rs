use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::config::SinkTypeConfig::{DataFile, InfluxDb, Mqtt};
use crate::config::{read_config_file, ConfigFile, LogConfig};
use crate::sink::{DataFileSink, InfluxDbSink, MqttSink, Sink, SinkType};
use anyhow::{bail, Context, Result};
use arexx::ArexxResult;
use clap::{arg, Parser};
use time::macros::format_description;
use tracing::level_filters::LevelFilter;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Layer};

mod arexx;
mod config;
mod sink;
mod usb;

const POLL_INTERVAL_SECONDS: u64 = 1;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct CliOptions {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(long)]
    start_time: Option<String>,
}

fn configure_tracing(opts: Option<LogConfig>) -> Result<Vec<WorkerGuard>> {
    let mut guards: Vec<WorkerGuard> = Vec::new();
    if let Some(LogConfig {
        enabled,
        directory,
        prefix,
        level,
    }) = opts
    {
        let file_log_layer = if enabled {
            let log_dir = directory.unwrap_or(String::from("."));
            let log_prefix = prefix.unwrap_or(String::from("arexx-tap"));

            let default_level = if enabled {
                "info".to_owned()
            } else {
                "off".to_owned()
            };
            let level = Level::from_str(level.unwrap_or(default_level).as_str())
                .context("invalid log level")?;

            let file_appender = RollingFileAppender::builder()
                .filename_prefix(log_prefix)
                .filename_suffix("log")
                .rotation(Rotation::DAILY)
                .build(log_dir)
                .unwrap();

            let timer = UtcTime::new(format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"));
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            let layer = fmt::Layer::new()
                .with_writer(non_blocking)
                .with_timer(timer)
                .with_ansi(false)
                .with_target(false)
                .with_filter(LevelFilter::from(level));

            guards.push(guard);
            Some(layer)
        } else {
            None
        };

        tracing_subscriber::registry().with(file_log_layer).init();
    }
    Ok(guards)
}

fn assemble_sinks(config: &ConfigFile) -> Vec<SinkType> {
    let mut sinks: Vec<SinkType> = Vec::new();
    for sink_type in &config.sink {
        match sink_type {
            DataFile(config) => {
                if let Ok(Some(sink)) = DataFileSink::new(config) {
                    sinks.push(SinkType::DataFile(Box::new(sink)))
                }
            }
            InfluxDb(config) => {
                if let Ok(Some(sink)) = InfluxDbSink::new(config) {
                    sinks.push(SinkType::InfluxDb(Box::new(sink)))
                }
            }
            Mqtt(config) => {
                if let Ok(Some(sink)) = MqttSink::new(config) {
                    sinks.push(SinkType::Mqtt(Box::new(sink)))
                }
            }
        }
    }

    sinks
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_options = CliOptions::parse();

    let config: ConfigFile;
    if let Some(config_file) = cli_options.config {
        if !config_file.exists() {
            bail!(format!(
                "config file `{}` not found. Aborting.",
                config_file.to_str().unwrap()
            ));
        }
        config = read_config_file(config_file)
        .context("error reading config file")
        .unwrap();
    } else {
        config = ConfigFile::default();
    }

    let _guards = configure_tracing(config.log.clone()).context("failed initializing tracing");

    println!("Starting arexx-tap");
    ConfigFile::print(config.clone());
    println!();

    let mut arexx = arexx::Arexx::new(config.clone(), cli_options.start_time)
        .context("failed to create Arexx instance")
        .unwrap();
    let sinks = assemble_sinks(&config);

    loop {
        match arexx.read_record() {
            Ok(ArexxResult::Temperature(reading)) => {
                tracing::debug!("read record: {:?}", &reading);
                if sinks.len() == 0 {
                    println!("{}", reading);
                } else {
                    for sink_type in &sinks {
                        let publish_result = match sink_type {
                            SinkType::DataFile(sink) => sink.publish(&reading).await,
                            SinkType::InfluxDb(sink) => sink.publish(&reading).await,
                            SinkType::Mqtt(sink) => sink.publish(&reading).await,
                        };
                        match publish_result {
                            Ok(_) => tracing::trace!("published {} to {}", &reading, sink_type),
                            Err(error) => tracing::error!("error publishing {} to {}: {}", &reading, sink_type, error)
                        }
                    }
                }
            },
            Ok(ArexxResult::NotAvailable) => {
                tracing::info!("Arexx device not available. Sleep 5 secs");
                std::thread::sleep(Duration::from_secs(5));
            }
            Ok(_) => {
                tracing::debug!("Ignore other data");
            }
            Err(error) => {
                tracing::error!("error reading record: {}", error);
            }
        }
        std::thread::sleep(Duration::from_secs(POLL_INTERVAL_SECONDS));
    }
    // unreachable:
    // Ok(())
}