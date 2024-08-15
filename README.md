# arexx-tap

This is a little tinker project that reads temperature data from an Arexx BS510 base station. The temperature values from the connected TL-2TSN sensors are recorded and persisted in different storages.

Three kind of storages exist currently:

- JSON file ([json lines](https://jsonlines.org/))
- InfluxDB (using the [line protocol](https://docs.influxdata.com/influxdb/cloud/reference/syntax/line-protocol/))
- MQTT 

In my setup the base station is connected to the Raspberry 3B+ USB port and data is stored in a local InfluxDB instance and data is visualized with Grafana.

## Configuration

The application is configured using a configuraton file (see example [config.toml](./config.toml)). The configuration file is passed as a startup parameter when starting the application:

Example:

```
 > ./arexx-tap -c config.toml
```

or when starting with cargo:

```
 > cargo run -- -c config.toml
```

The temperature number values are calibrated using a scaling factor (`temperature-scaling`). Different sources on the the internet suggest to take `0.0078` which is now the default value. This scaling factor can be globally changed in the configuration file or individually for every configured sensor.

## References

- [arexx-multilogger-collectd-plugin](https://github.com/pka/arexx-multilogger-collectd-plugin)
- [Arexx Data Logger Protocol](https://github.com/pka/arexx-multilogger-collectd-plugin/blob/master/PROTOCOL.md)
- [pylarexx](https://github.com/redflo/pylarexx)
- [Reading Arexx TL-500 with Python on Linux](https://www.j-raedler.de/2010/08/reading-arexx-tl-500-with-python-on-linux-part-ii/)
- [FriendlyARM Mini2440 + AREXX TL-500 temperature logger on Linux](http://rndhax.blogspot.com/2010/03/friendlyarm-mini2440-arexx-tl-500.html)
- [Decoding AREXX TSN-70E](https://github.com/merbanan/rtl_433/issues/2482)
- [arexxfs](https://github.com/drystone/arexxfs)
