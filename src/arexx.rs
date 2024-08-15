use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use std::cell::Cell;
use anyhow::{bail, Result};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use crate::config::{ConfigFile, SensorConfig};
use crate::usb::{self, UsbDevice, UsbInner};

const INTERNAL_TEMPERATURE_SCALE: f32 = 0.0078;

#[derive(Debug, Serialize, Deserialize)]
pub struct TemperatureReading {
    pub timestamp: DateTime<FixedOffset>,
    pub sensor: u16,
    pub value: f32,
}

impl Display for TemperatureReading {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Temperature[time: {}, sensor: {}, temp: {}]",
            self.timestamp, self.sensor, self.value
        )
    }
}

#[derive(Debug)]
pub struct Arexx {
    start_time: Cell<Option<DateTime<FixedOffset>>>,
    connect_initialized: usize,
    pub sensor_config_lookup: HashMap<u16,SensorConfig>,
    pub usb: Arc<Mutex<UsbDevice>>,
}

fn create_arexx_date_bytes(date_time: DateTime<FixedOffset>) -> Result<[u8; 4]> {
    let ref_date: DateTime<Utc> = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
    let arexx_init_seconds = date_time.signed_duration_since(ref_date).num_seconds() as u32;
    tracing::trace!("initialize arexx with {} seconds since \"2000-01-01 00:00:00\"", arexx_init_seconds);
    Ok(arexx_init_seconds.to_le_bytes())
}

fn parse_arexx_date_bytes(bytes: [u8; 4]) -> Result<DateTime<FixedOffset>> {
    let ref_date: DateTime<Utc> = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
    let secs = u32::from_le_bytes(bytes);
    Ok((ref_date + Duration::from_secs(secs.into())).into())
}

pub enum ArexxResult {
    Temperature(TemperatureReading),
    Other,
    NotAvailable
}

fn parse_start_time(start_time: Option<String>) -> Option<DateTime<FixedOffset>> {
    if let Some(ts) = start_time {
        let local: DateTime<Local> = Local::now();
            if let Ok(time_only) = NaiveTime::parse_from_str(ts.as_str(), "%H:%M:%S") {
                let naive_date_time = local.with_time(time_only).single().unwrap();
                Some(naive_date_time.fixed_offset())
            } else {
                if let Ok(day_only) = NaiveDate::parse_from_str(ts.as_str(), "%Y-%m-%d") {
                    let naive_date_time = day_only.and_time(local.time());
                    let date_time: DateTime<Local> = Local.from_local_datetime(&naive_date_time).unwrap();
                    Some(date_time.fixed_offset())
                } else {
                    if let Ok(naive_date_time) = NaiveDateTime::parse_from_str(ts.as_str(), "%Y-%m-%d %H:%M:%S") {
                        let date_time: DateTime<Local> = Local.from_local_datetime(&naive_date_time).unwrap();
                        Some(date_time.fixed_offset())
                    } else {
                        None
                    }
                }
            }
    } else {
        None
    }
}

impl Arexx {
    pub fn new(config: ConfigFile, start_time: Option<String>) -> Result<Arexx> {
        let vid = config.vid;
        let pid = config.pid;
        let usb = usb::UsbDevice::new(vid, pid)?;

        let mut sensor_config_lookup = HashMap::new();
        let fallback_temperature_scaling = config.temperature_scaling.unwrap_or(INTERNAL_TEMPERATURE_SCALE);
        for sensor in config.sensors {
            if sensor.temperature_scaling.get().is_none() {
                sensor.temperature_scaling.set(Some(fallback_temperature_scaling));
            }
            sensor_config_lookup.insert(sensor.id, sensor);
        }

        Ok(Arexx {
            usb,
            sensor_config_lookup,
            connect_initialized: 0,
            start_time: Cell::new(parse_start_time(start_time))
        })
    }

    fn init_arexx(&self, usb_inner: &UsbInner) -> anyhow::Result<()> {
        let mut buf: [u8; 64] = [0; 64];
        let timeout = Duration::from_secs(30);

        let arexx_start_time = self.start_time.get().unwrap_or(Local::now().fixed_offset());
        tracing::info!("init arexx with start time {}", arexx_start_time);
        // passed start_time should only be applied once on the first init.
        // reset therefor start_time to None so that the current time is used afterwards.
        self.start_time.set(None);

        buf[0] = 0x04;
        buf[1..5].copy_from_slice(&create_arexx_date_bytes(arexx_start_time)?);

        let write_addr = usb_inner.endpoints.write_addr;
        match usb_inner.handle.borrow().write_bulk(write_addr, &buf, timeout) {
            Ok(len) => {
                tracing::debug!("arexx init written {} bytes", len);
                anyhow::Ok(())
            }
            Err(error) => bail!(error)
        }
    }

    pub fn read_record(&mut self) -> Result<ArexxResult> {
        let connect_count = self.usb.lock().unwrap().connect_count;
        if let Some(ref usb_inner) = self.usb.lock().unwrap().inner {
            if self.connect_initialized != connect_count {
                self.init_arexx(usb_inner)?;
                self.connect_initialized = connect_count;
            }

            let handle = usb_inner.handle.borrow();
            let endpoints = usb_inner.endpoints;

            let mut buf: [u8; 64] = [0; 64];
            let timeout = Duration::from_secs(30);

            // trigger arexx to send data
            buf[0] = 0x03;
            match handle.write_bulk(endpoints.write_addr, &buf, timeout) {
                Ok(len) => {
                    tracing::trace!("successfully sent trigger to arexx ({})", len)
                }
                Err(err) => {
                    tracing::error!("arexx trigger: Error ({:?})", err);
                    bail!("failed to trigger arexx: {}", err);
                }
            }

            // read data
            match handle.read_bulk(endpoints.read_addr, &mut buf, timeout) {
                Ok(_len) => {
                    let sensor_id_bytes = buf[2..4].try_into()?;
                    let sensor_id = u16::from_le_bytes(sensor_id_bytes);

                    let value_bytes = buf[4..6].try_into()?;
                    let value = u16::from_be_bytes(value_bytes);

                    let ts_bytes = buf[6..10].try_into()?;
                    let timestamp = parse_arexx_date_bytes(ts_bytes);

                    tracing::trace!("read_bulk: {:?} {}", timestamp, sensor_id);
                    if let Ok(ts) = timestamp {
                        if sensor_id != 0xFFFF {
                            match self.sensor_config_lookup.get(&sensor_id) {
                                Some(sensor_config) => {
                                    let scaled_value = value as f32 * sensor_config.temperature_scaling.get().unwrap();
                                    tracing::trace!("sensor {}, value={}, scaled_value={}", &sensor_id, value, scaled_value);
                                    Ok(ArexxResult::Temperature(TemperatureReading {
                                        timestamp: ts,
                                        sensor: sensor_id,
                                        value: scaled_value,
                                    }))
                                },
                                None => {
                                    tracing::trace!("temperature read from unknown sensor ID {}", &sensor_id);
                                    Ok(ArexxResult::Other)
                                }
                            }
                        } else {
                            Ok(ArexxResult::Other)
                        }
                    } else {
                        bail!("error parsing timestamp: {:?}", ts_bytes)
                    }
                }
                Err(err) => {
                    tracing::error!("failed to read from arexx endpoint: {}", err);
                    bail!(err.to_string());
                }
            }
        } else {
            Ok(ArexxResult::NotAvailable)
        }
    }
}