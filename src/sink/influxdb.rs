use crate::arexx::TemperatureReading;
use crate::config::InfluxDbConfig;
use crate::sink::Sink;
use anyhow::{Context, Ok, Result};
use chrono::{DateTime, Utc};
use influxdb::{Client, InfluxDbWriteable, ReadQuery, Timestamp};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(InfluxDbWriteable, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct InfluxDbTemperatureReading {
    time: DateTime<Utc>,
    temperature: f32,
}

pub struct InfluxDbSink {
    url: String,
    client: Client,
    measurement_base: String,
}

impl Display for InfluxDbSink {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "InfluxDbSink({})", self.url)
    }
}

impl InfluxDbSink {
    pub fn new(config: &InfluxDbConfig) -> Result<Option<Self>> {
        if config.enabled {
            let client = Client::new(&config.url, &config.bucket).with_token(&config.token);
            Ok(Some(InfluxDbSink {
                client,
                measurement_base: config.measurement_base.to_owned(),
                url: config.url.to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    fn format_measurement_name(&self, sensor: u16) -> String {
        format!("{}.{}", &self.measurement_base, sensor)
    }

    // currently not used
    #[allow(dead_code)]
    pub async fn last_insert_time(&self) -> Result<Option<DateTime<Utc>>> {
        // https://docs.influxdata.com/influxdb/v1/query_language/explore-data/
        // SELECT count(*) FROM /^mqtt.0.smartmeter.61064149.*/
        // SELECT * FROM /^mqtt\.0\.smartmeter\.61064149.*/ ORDER BY time DESC LIMIT 1

        let all_topic_regex = format!("^{}.*", self.measurement_base);
        let read_query = ReadQuery::new(format!(
            "SELECT * FROM /{}/ ORDER BY time DESC LIMIT 1",
            all_topic_regex
        ));
        let read_result = self
            .client
            .json_query(read_query)
            .await
            .and_then(|mut db_result| db_result.deserialize_next::<InfluxDbTemperatureReading>())
            .context("failed to execute InfluxDB query")
            .unwrap();
        if !read_result.series.is_empty() {
            let temperature_reading = &read_result.series[0].values[0];
            Ok(Some(temperature_reading.time))
        } else {
            Ok(None)
        }
    }
}

impl Sink for InfluxDbSink {
    async fn publish(&self, reading: &TemperatureReading) -> Result<()> {
        tracing::trace!("publish InfluxDB {}", reading);
        let millis = reading.timestamp.to_utc().timestamp_millis() as u128;
        let wq = self.format_measurement_name(reading.sensor);
        let temperature_readings = Timestamp::Milliseconds(millis)
            .into_query(wq)
            .add_field("value", reading.value);

        self.client.query(temperature_readings).await.expect("failed writing temperature record");

        Ok(())
    }
}