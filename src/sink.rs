use crate::arexx::TemperatureReading;
use std::fmt;

mod data_file;
mod influxdb;
mod mqtt;

pub use crate::sink::data_file::DataFileSink;
pub use crate::sink::influxdb::InfluxDbSink;
pub use crate::sink::mqtt::MqttSink;

pub enum SinkType {
    DataFile(Box<DataFileSink>),
    InfluxDb(Box<InfluxDbSink>),
    Mqtt(Box<MqttSink>),
}

impl fmt::Display for SinkType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SinkType::DataFile(_) => write!(f, "DateFile"),
            SinkType::InfluxDb(_) => write!(f, "InfluxDB"),
            SinkType::Mqtt(_) => write!(f, "MQTT"),
        }
    }
}

pub trait Sink {
    async fn publish(&self, reading: &TemperatureReading) -> anyhow::Result<()>;
}
