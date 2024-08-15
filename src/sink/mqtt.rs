use std::fmt::{Display, Formatter};
use std::time::Duration;

use anyhow::{Ok, Result, bail};
use json::object;
use rumqttc::{AsyncClient, MqttOptions, QoS};

use crate::arexx::TemperatureReading;
use crate::config::MqttConfig;

use crate::sink::Sink;

pub struct MqttSink {
    host: String,
    client: AsyncClient,
    topic_base: String,
    _eventloop: tokio::task::JoinHandle<()>,
}

impl Display for MqttSink {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "MqttSink({})", self.host)
    }
}

impl Sink for MqttSink {
    async fn publish(&self, reading: &TemperatureReading) -> Result<()> {
        tracing::trace!("publish MQTT {}", reading);

        let value = object! {
            time: reading.timestamp.to_rfc3339(),
            value: reading.value
        }
        .dump();
       
        let res = self.client
            .publish(self.format_topic(reading.sensor), QoS::AtLeastOnce, false, value)
            .await;

        match res {
            std::result::Result::Ok(()) => Ok(()),
            Err(_) => bail!("publish failed")
        }
    }
}

impl MqttSink {
    pub fn new(config: &MqttConfig) -> Result<Option<Self>> {
        if config.enabled {
            let mut mqtt_options = MqttOptions::new("arexx-mqtt", config.host.clone(), config.port);
            mqtt_options.set_keep_alive(Duration::from_secs(5));

            let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
            let handle = tokio::spawn(async move {
                while let std::result::Result::Ok(notification) = eventloop.poll().await {
                    tracing::trace!("MQTT event = {:?}", notification);
                }
            });

            Ok(Some(MqttSink {
                client,
                topic_base: config.topic_base.to_string(),
                host: config.host.to_string(),
                _eventloop: handle,
            }))
        } else {
            Ok(None)
        }
    }

    fn format_topic(&self, sensor: u16) -> String {
        format!("{}/{}", self.topic_base, sensor)
    }
}