//! Module containing functions related to parsing the Site24x7 API payload.
use anyhow::{anyhow, Context, Result};
use log::debug;

use crate::site24x7_types as types;

/// Parse current returned JSON from call to /current_status
pub fn parse_current_status(
    json: &str,
) -> Result<types::CurrentStatusData, types::CurrentStatusError> {
    let deserializer = &mut serde_json::Deserializer::from_str(json);
    let current_status_resp_result = serde_path_to_error::deserialize(deserializer);

    let v: serde_json::Value = serde_json::from_str(json).context("JSON seems invalid.")?;
    debug!(
        "JSON received from server: \n{}",
        serde_json::to_string_pretty(&v).context("Couldn't format JSON for debug output")?
    );
    let current_status_resp_parsed: types::CurrentStatusResponse = current_status_resp_result
        .map_err(|e| {
            // For better error path output, try to parse into `CurrentStatusResponseInner`
            // directly. This will give us a path to the error.
            let debug_deserializer = &mut serde_json::Deserializer::from_str(json);
            let debug_deserializer_result: Result<types::CurrentStatusResponseInner, _> =
                serde_path_to_error::deserialize(debug_deserializer);
            let debug_err = debug_deserializer_result.err();
            anyhow!(types::CurrentStatusError::ParseError(e.to_string()))
                .context(debug_err.map(|e| e.to_string()).unwrap_or_default())
        })
        .context("Couldn't parse server response while fetching monitors.".to_string())?;

    match current_status_resp_parsed {
        types::CurrentStatusResponse::Success(inner) => Ok(inner.data),
        types::CurrentStatusResponse::Error(e) => {
            if e.message == "OAuth Access Token is invalid or has expired." {
                Err(types::CurrentStatusError::ApiAuthError(e.message))
            } else {
                Err(types::CurrentStatusError::ApiUnknownError(e.message))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    /// If we get an entirely empty body, we'll treat it as if there are no monitors at all.
    fn empty_response() -> Result<()> {
        let s = include_str!("../tests/data/empty_response.json");
        let data = parse_current_status(s)?;
        assert!(data.monitor_groups.is_empty());
        assert!(data.monitors.is_empty());
        Ok(())
    }

    #[test]
    /// Properly handle empty lists for monitors.
    fn empty_lists() -> Result<()> {
        let s = include_str!("../tests/data/empty_lists.json");
        let data = parse_current_status(s)?;
        assert!(data.monitor_groups.is_empty());
        assert!(data.monitors.is_empty());
        Ok(())
    }

    #[test]
    /// Sometimes very recent monitors that have yet to make their first poll will have empty data
    /// in some fields.
    fn partial_location_data() -> Result<()> {
        let s = include_str!("../tests/data/partial_location_data.json");
        let data = parse_current_status(s)?;
        let expected_monitor = types::MonitorMaybe::URL(types::Monitor {
            name: "test".to_string(),
            unit: None,
            attribute_key: None,
            status: types::Status::Up,
            locations: vec![
                types::Location {
                    status: types::Status::ConfigurationError,
                    attribute_value: None,
                    location_name: "London - UK".to_string(),
                    last_polled_time: None,
                },
                {
                    types::Location {
                        status: types::Status::Up,
                        attribute_value: Some(757),
                        location_name: "Bucharest - RO".to_string(),
                        last_polled_time: Some(DateTime::parse_from_str(
                            "2021-01-06T18:53:06+0000",
                            types::DATE_FORMAT,
                        )?),
                    }
                },
            ],
            attribute_name: "RESPONSETIME".to_string(),
            attribute_value: None,
            monitor_id: "01".to_string(),
            tags: vec![],
            last_polled_time: Some(DateTime::parse_from_str(
                "2021-01-06T18:53:07+0000",
                types::DATE_FORMAT,
            )?),
        });

        assert_eq!(data.monitors, vec![expected_monitor]);
        assert!(data.monitor_groups.is_empty());
        Ok(())
    }

    #[test]
    /// Sometimes monitors with an old `last_polled_time` will not return their last
    /// `attribute_value` despite them being up.
    /// This is a weird corner case as usually you'd expect monitor to only return no
    /// `attribute_value` in case they are down but in practice this doesn't appear to be the
    /// case.
    fn no_attribute_value_but_monitor_up() -> Result<()> {
        let s = include_str!("../tests/data/no_attribute_value.json");
        let data = parse_current_status(s)?;
        let expected_monitor = types::MonitorMaybe::URL(types::Monitor {
            name: "test".to_string(),
            unit: None,
            attribute_key: None,
            status: types::Status::Up,
            locations: vec![
                types::Location {
                    status: types::Status::Up,
                    attribute_value: None,
                    location_name: "London - UK".to_string(),
                    last_polled_time: Some(DateTime::parse_from_str(
                        "2021-01-06T18:53:06+0000",
                        types::DATE_FORMAT,
                    )?),
                },
                {
                    types::Location {
                        status: types::Status::Up,
                        attribute_value: Some(757),
                        location_name: "Bucharest - RO".to_string(),
                        last_polled_time: Some(DateTime::parse_from_str(
                            "2021-01-06T18:53:06+0000",
                            types::DATE_FORMAT,
                        )?),
                    }
                },
            ],
            attribute_name: "RESPONSETIME".to_string(),
            attribute_value: None,
            monitor_id: "01".to_string(),
            tags: vec![],
            last_polled_time: Some(DateTime::parse_from_str(
                "2021-01-06T18:53:07+0000",
                types::DATE_FORMAT,
            )?),
        });

        assert_eq!(data.monitors, vec![expected_monitor]);
        assert!(data.monitor_groups.is_empty());
        Ok(())
    }

    #[test]
    /// Full test that tests a real-world case with lots of fields.
    fn full_data() -> Result<()> {
        let s = include_str!("../tests/data/full.json");
        let data = parse_current_status(s)?;
        let expected_monitor_group_prod = types::MonitorGroup {
            group_id: "01".to_string(),
            group_name: "production".to_string(),
            monitors: vec![
                types::MonitorMaybe::REALBROWSER(types::Monitor {
                    name: "production (realbrowser)".to_string(),
                    unit: Some("ms".to_string()),
                    attribute_key: Some("transaction_time".to_string()),
                    status: types::Status::Down,
                    locations: vec![
                        types::Location {
                            status: types::Status::Up,
                            attribute_value: Some(27458),
                            location_name: "Falkenstein - DE".to_string(),
                            last_polled_time: Some(DateTime::parse_from_str(
                                "2021-01-06T18:27:41+0000",
                                types::DATE_FORMAT,
                            )?),
                        },
                        types::Location {
                            status: types::Status::Down,
                            attribute_value: None,
                            location_name: "Shenzhen - CHN".to_string(),
                            last_polled_time: Some(DateTime::parse_from_str(
                                "2021-01-06T18:27:41+0000",
                                types::DATE_FORMAT,
                            )?),
                        },
                    ],
                    attribute_name: "TRANSACTIONTIME".to_string(),
                    attribute_value: Some(27458),
                    monitor_id: "0101".to_string(),
                    tags: vec![],
                    last_polled_time: Some(DateTime::parse_from_str(
                        "2021-01-06T18:27:41+0000",
                        types::DATE_FORMAT,
                    )?),
                }),
                types::MonitorMaybe::HOMEPAGE(types::Monitor {
                    name: "production (homepage)".to_string(),
                    unit: Some("ms".to_string()),
                    attribute_key: Some("response_time".to_string()),
                    status: types::Status::Up,
                    locations: vec![
                        types::Location {
                            status: types::Status::Up,
                            attribute_value: Some(718),
                            location_name: "Falkenstein - DE".to_string(),
                            last_polled_time: Some(DateTime::parse_from_str(
                                "2021-01-06T17:44:10+0000",
                                types::DATE_FORMAT,
                            )?),
                        },
                        types::Location {
                            status: types::Status::Up,
                            attribute_value: Some(3830),
                            location_name: "Shenzhen - CHN".to_string(),
                            last_polled_time: Some(DateTime::parse_from_str(
                                "2021-01-06T17:44:10+0000",
                                types::DATE_FORMAT,
                            )?),
                        },
                    ],
                    attribute_name: "RESPONSETIME".to_string(),
                    attribute_value: Some(718),
                    monitor_id: "0102".to_string(),
                    tags: vec![],
                    last_polled_time: Some(DateTime::parse_from_str(
                        "2021-01-06T17:44:10+0000",
                        types::DATE_FORMAT,
                    )?),
                }),
                types::MonitorMaybe::URL(types::Monitor {
                    name: "production (url)".to_string(),
                    unit: Some("ms".to_string()),
                    attribute_key: Some("response_time".to_string()),
                    status: types::Status::Up,
                    locations: vec![
                        types::Location {
                            status: types::Status::Up,
                            attribute_value: Some(173),
                            location_name: "Falkenstein - DE".to_string(),
                            last_polled_time: Some(DateTime::parse_from_str(
                                "2021-01-06T18:43:27+0000",
                                types::DATE_FORMAT,
                            )?),
                        },
                        types::Location {
                            status: types::Status::Up,
                            attribute_value: Some(2322),
                            location_name: "Shenzhen - CHN".to_string(),
                            last_polled_time: Some(DateTime::parse_from_str(
                                "2021-01-06T18:42:16+0000",
                                types::DATE_FORMAT,
                            )?),
                        },
                    ],
                    attribute_name: "RESPONSETIME".to_string(),
                    attribute_value: Some(173),
                    monitor_id: "0103".to_string(),
                    tags: vec![],
                    last_polled_time: Some(DateTime::parse_from_str(
                        "2021-01-06T18:43:27+0000",
                        types::DATE_FORMAT,
                    )?),
                }),
            ],
        };
        let expected_monitor_group_int = types::MonitorGroup {
            group_id: "02".to_string(),
            group_name: "integration".to_string(),
            monitors: vec![types::MonitorMaybe::HOMEPAGE(types::Monitor {
                name: "integration (homepage)".to_string(),
                unit: Some("ms".to_string()),
                attribute_key: Some("response_time".to_string()),
                status: types::Status::Up,
                locations: vec![
                    types::Location {
                        status: types::Status::Up,
                        attribute_value: Some(1081),
                        location_name: "Falkenstein - DE".to_string(),
                        last_polled_time: Some(DateTime::parse_from_str(
                            "2021-01-06T18:33:34+0000",
                            types::DATE_FORMAT,
                        )?),
                    },
                    types::Location {
                        status: types::Status::Up,
                        attribute_value: Some(13706),
                        location_name: "Shenzhen - CHN".to_string(),
                        last_polled_time: Some(DateTime::parse_from_str(
                            "2021-01-06T18:18:31+0000",
                            types::DATE_FORMAT,
                        )?),
                    },
                ],
                attribute_name: "RESPONSETIME".to_string(),
                attribute_value: Some(1081),
                monitor_id: "0201".to_string(),
                tags: vec![
                    types::Tag {
                        key: "test1".to_string(),
                        value: "".to_string(),
                    },
                    types::Tag {
                        key: "test2k".to_string(),
                        value: "test2v".to_string(),
                    },
                    types::Tag {
                        key: "test3k".to_string(),
                        value: "test3v:a:b".to_string(),
                    },
                ],
                last_polled_time: Some(DateTime::parse_from_str(
                    "2021-01-06T18:33:34+0000",
                    types::DATE_FORMAT,
                )?),
            })],
        };
        let expected_monitor = types::MonitorMaybe::URL(types::Monitor {
            name: "separate monitor".to_string(),
            unit: Some("ms".to_string()),
            attribute_key: Some("response_time".to_string()),
            status: types::Status::Up,
            locations: vec![
                {
                    types::Location {
                        status: types::Status::Up,
                        attribute_value: Some(1534),
                        location_name: "Singapore - SG".to_string(),
                        last_polled_time: Some(DateTime::parse_from_str(
                            "2021-01-06T18:26:31+0000",
                            types::DATE_FORMAT,
                        )?),
                    }
                },
                types::Location {
                    status: types::Status::Up,
                    attribute_value: Some(165),
                    location_name: "London - UK".to_string(),
                    last_polled_time: Some(DateTime::parse_from_str(
                        "2021-01-06T18:26:31+0000",
                        types::DATE_FORMAT,
                    )?),
                },
            ],
            attribute_name: "RESPONSETIME".to_string(),
            attribute_value: Some(139),
            monitor_id: "00".to_string(),
            tags: vec![],
            last_polled_time: Some(DateTime::parse_from_str(
                "2021-01-06T18:41:53+0000",
                types::DATE_FORMAT,
            )?),
        });

        assert_eq!(
            data.monitor_groups,
            vec![expected_monitor_group_prod, expected_monitor_group_int]
        );
        assert_eq!(data.monitors, vec![expected_monitor]);
        Ok(())
    }

    #[test]
    /// An OAuth token error should return ApiAuthError.
    fn auth_error_returns_api_auth_error() {
        let s = include_str!("../tests/data/auth_error.json");
        let result = parse_current_status(s);
        assert!(matches!(
            result,
            Err(types::CurrentStatusError::ApiAuthError(_))
        ));
        if let Err(types::CurrentStatusError::ApiAuthError(msg)) = result {
            assert!(msg.contains("OAuth Access Token is invalid or has expired."));
        }
    }

    #[test]
    /// An unknown API error should return ApiUnknownError.
    fn unknown_api_error_returns_api_unknown_error() {
        let s = include_str!("../tests/data/unknown_api_error.json");
        let result = parse_current_status(s);
        assert!(matches!(
            result,
            Err(types::CurrentStatusError::ApiUnknownError(_))
        ));
        if let Err(types::CurrentStatusError::ApiUnknownError(msg)) = result {
            assert!(msg.contains("unknown API error"));
        }
    }

    #[test]
    /// Completely invalid JSON should return a ParseError.
    fn invalid_json_returns_parse_error() {
        let result = parse_current_status("this is not json at all");
        assert!(result.is_err());
    }

    #[test]
    /// A RESTAPI monitor type should be parsed into MonitorMaybe::RESTAPI.
    fn restapi_monitor_type_is_parsed() -> Result<()> {
        let s = include_str!("../tests/data/restapi_monitor.json");
        let data = parse_current_status(s)?;
        assert_eq!(data.monitors.len(), 1);
        assert!(matches!(data.monitors[0], types::MonitorMaybe::RESTAPI(_)));
        Ok(())
    }

    #[test]
    /// An unknown monitor type should be parsed into MonitorMaybe::Unknown.
    fn unknown_monitor_type_is_parsed_as_unknown() -> Result<()> {
        let s = include_str!("../tests/data/unknown_monitor_type.json");
        let data = parse_current_status(s)?;
        assert_eq!(data.monitors.len(), 1);
        assert!(matches!(data.monitors[0], types::MonitorMaybe::Unknown));
        Ok(())
    }

    #[test]
    /// Tags with key:value format should be deserialized correctly.
    fn monitor_tags_are_deserialized() -> Result<()> {
        let s = include_str!("../tests/data/monitor_with_tags.json");
        let data = parse_current_status(s)?;
        assert_eq!(data.monitors.len(), 1);
        if let types::MonitorMaybe::URL(monitor) = &data.monitors[0] {
            assert_eq!(monitor.tags.len(), 3);
            assert_eq!(monitor.tags[0].key, "env");
            assert_eq!(monitor.tags[0].value, "production");
            assert_eq!(monitor.tags[1].key, "team");
            assert_eq!(monitor.tags[1].value, "platform");
            // Tag without colon: key = "novalue", value = ""
            assert_eq!(monitor.tags[2].key, "novalue");
            assert_eq!(monitor.tags[2].value, "");
        } else {
            panic!("Expected MonitorMaybe::URL");
        }
        Ok(())
    }

    #[test]
    /// Monitors inside monitor groups should be parsed correctly.
    fn monitor_group_is_parsed() -> Result<()> {
        let s = include_str!("../tests/data/monitor_group.json");
        let data = parse_current_status(s)?;
        assert!(data.monitors.is_empty());
        assert_eq!(data.monitor_groups.len(), 1);
        let group = &data.monitor_groups[0];
        assert_eq!(group.group_id, "grp01");
        assert_eq!(group.group_name, "production");
        assert_eq!(group.monitors.len(), 1);
        Ok(())
    }

    #[test]
    /// All Status enum variants should be deserialized from their numeric representation.
    fn all_status_variants_are_deserialized() -> Result<()> {
        let s = include_str!("../tests/data/all_statuses.json");
        let data = parse_current_status(s)?;
        assert_eq!(data.monitors.len(), 1);
        if let types::MonitorMaybe::URL(monitor) = &data.monitors[0] {
            let statuses: Vec<&types::Status> =
                monitor.locations.iter().map(|l| &l.status).collect();
            assert!(statuses.contains(&&types::Status::Down));
            assert!(statuses.contains(&&types::Status::Up));
            assert!(statuses.contains(&&types::Status::Trouble));
            assert!(statuses.contains(&&types::Status::Critical));
            assert!(statuses.contains(&&types::Status::Suspended));
            assert!(statuses.contains(&&types::Status::Maintenance));
            assert!(statuses.contains(&&types::Status::Discovery));
            assert!(statuses.contains(&&types::Status::ConfigurationError));
        } else {
            panic!("Expected MonitorMaybe::URL");
        }
        Ok(())
    }

    #[test]
    /// Status::default() should return ConfigurationError.
    fn status_default_is_configuration_error() {
        assert_eq!(types::Status::default(), types::Status::ConfigurationError);
    }

    #[test]
    /// MonitorMaybe::Display should return the monitor_type string.
    fn monitor_maybe_display() {
        use std::fmt::Write;
        let mut s = String::new();
        let monitor = types::MonitorMaybe::URL(types::Monitor {
            name: "x".to_string(),
            unit: None,
            attribute_key: None,
            status: types::Status::Up,
            locations: vec![],
            attribute_name: "RESPONSETIME".to_string(),
            attribute_value: None,
            monitor_id: "1".to_string(),
            tags: vec![],
            last_polled_time: None,
        });
        write!(s, "{monitor}").unwrap();
        assert_eq!(s, "URL");
    }

    #[test]
    /// CurrentStatusError variants should display their messages correctly.
    fn current_status_error_display() {
        let auth_err = types::CurrentStatusError::ApiAuthError("token expired".to_string());
        assert!(auth_err.to_string().contains("token expired"));

        let unknown_err = types::CurrentStatusError::ApiUnknownError("something broke".to_string());
        assert!(unknown_err.to_string().contains("something broke"));

        let parse_err = types::CurrentStatusError::ParseError("bad json".to_string());
        assert!(parse_err.to_string().contains("bad json"));
    }
}
