use std::{cell::Cell, path::PathBuf};

use anyhow::{Context, Ok, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct ConfigFile {
    pub vid: u16,
    pub pid: u16,

    #[serde(rename = "temperature-scaling")]
    pub temperature_scaling: Option<f32>,

    pub log: Option<LogConfig>,

    pub sink: Vec<SinkTypeConfig>,

    pub sensors: Vec<SensorConfig>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self { vid: 0x0451, pid: 0x3211, temperature_scaling: None, log: Default::default(), sink: Default::default(), sensors: Default::default(), }
    }
}

impl ConfigFile {
    pub fn print(self) {
        println!("\nConfiguration");
        println!("  USB Port: vid = 0x{:04x}, pid = 0x{:04x}", self.vid, self.pid);
        if self.temperature_scaling.is_some() {
            println!("  Global temperature scale = {}", self.temperature_scaling.unwrap());
        }
        if let Some(log_config) = self.log {
            if log_config.enabled {
                println!("  Logging: level={}, directory={}, prefix={}",
                        log_config.level.unwrap_or(String::from("info")),
                        log_config.directory.unwrap_or("<current dir>".into()),
                        log_config.prefix.unwrap_or("-".into()))
            } else {
                println!("  Logging: disabled")
            }
        } else {
            println!("  Logging: disabled")
        }
        let enabled_sinks : Vec<&SinkTypeConfig> = self.sink.iter().filter(|sink_config| {
                                                        match sink_config {
                                                            SinkTypeConfig::DataFile(config) => config.enabled,
                                                            SinkTypeConfig::InfluxDb(config)=> config.enabled,
                                                            SinkTypeConfig::Mqtt(config) => config.enabled
                                                        }}).collect();
        if enabled_sinks.len() == 0 {
            println!("  Sinks: none");
        } else {
            println!("  Sinks:");
            for sink_config in enabled_sinks {
                let ser_sink_config = match sink_config {
                    SinkTypeConfig::DataFile(config) => format!("Data File: {}", serde_json::to_string(config).unwrap()),
                    SinkTypeConfig::InfluxDb(config)=>  format!("InfluxDB:  {}", serde_json::to_string(config).unwrap()),
                    SinkTypeConfig::Mqtt(config) =>     format!("MQTT:      {}", serde_json::to_string(config).unwrap())
                };
                println!("     {:?}", ser_sink_config);
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogConfig {
    pub enabled: bool,
    pub directory: Option<String>,
    pub prefix: Option<String>,
    pub level: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataFileConfig {
    pub enabled: bool,
    pub file: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InfluxDbConfig {
    pub enabled: bool,
    pub url: String,
    pub bucket: String,
    pub token: String,
    pub detect_start_time: Option<bool>,
    #[serde(rename = "measurement-base")]
    pub measurement_base: String
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MqttConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    #[serde(rename = "topic-base")]
    pub topic_base: String,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum SinkTypeConfig {
    DataFile(DataFileConfig),
    #[serde(rename = "InfluxDB")]
    InfluxDb(InfluxDbConfig),
    #[serde(rename = "MQTT")]
    Mqtt(MqttConfig),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SensorConfig {
    pub id: u16,
    pub name: String,
    #[serde(rename = "temperature-scaling")]
    pub temperature_scaling: Cell<Option<f32>>
}

pub fn read_config_file(config_file: PathBuf) -> Result<ConfigFile> {
    let config_str = std::fs::read_to_string(config_file)
        .context("Failed to open file")
        .unwrap();
    let config = toml::from_str::<ConfigFile>(&config_str)
        .context("Failed to read toml configuration")
        .unwrap();

    Ok(config)
}