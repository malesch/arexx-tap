use std::fmt::{Display, Formatter};
use std::{
    fs::{File, OpenOptions},
    io::Write,
};

use crate::arexx::TemperatureReading;
use crate::config::DataFileConfig;
use anyhow::{Context, Ok, Result};

use super::Sink;

pub struct DataFileSink {
    file: File,
}

impl DataFileSink {
    pub fn new(config: &DataFileConfig) -> Result<Option<Self>> {
        if config.enabled {
            let path = &config.file;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("Can't open file {}", path))
                .unwrap();
            Ok(Some(DataFileSink { file }))
        } else {
            Ok(None)
        }
    }
}

impl Display for DataFileSink {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "DataFileSink({:?})", self.file)
    }
}

impl Sink for DataFileSink {
    async fn publish(&self, reading: &TemperatureReading) -> Result<()> {
        tracing::trace!("publish DataFile {}", reading);
        let temperature_json = serde_json::to_string(&reading)
            .context("Json serialization failed")
            .unwrap();
        let mut f = &self.file;
        writeln!(f, "{}", &temperature_json)
            .context("cannot write to file")
            .unwrap();
        f.flush().context("flush failed").unwrap();

        Ok(())
    }
}