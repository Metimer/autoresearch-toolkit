//! Strict, inert historical import. Declared outcomes never become engine decisions.
use crate::session::{self, SessionError, SessionGuard};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{Map, Number, Value};
use std::{fmt, path::Path};
type Result<T> = std::result::Result<T, SessionError>;
pub const SOURCE_LIMIT: usize = 4 * 1024 * 1024;
pub(crate) const REPORT_LIMIT: usize = 8 * 1024 * 1024;
const LINE_LIMIT: usize = 64 * 1024;
const LINE_COUNT: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyFormat {
    Auto,
    Pi,
    Portable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anomaly {
    pub line: Option<usize>,
    pub severity: String,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalEntry {
    pub line: usize,
    pub segment: u64,
    pub kind: String,
    pub trust: String,
    /// Validated historical data, never executable commands or current proof.
    pub declared: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportReport {
    pub format_version: u32,
    pub source_sha256: String,
    pub source_bytes: usize,
    pub source_format: Option<LegacyFormat>,
    pub source_profile: Option<String>,
    pub importable: bool,
    pub trust: String,
    pub entries: Vec<HistoricalEntry>,
    pub anomalies: Vec<Anomaly>,
}
impl ImportReport {
    fn issue(&mut self, line: Option<usize>, severity: &str, code: &str, message: &str) {
        self.anomalies.push(Anomaly {
            line,
            severity: severity.into(),
            code: code.into(),
            message: message.into(),
        });
    }
}
/// Immutable preparation permits inspection before any destination is created.
pub struct PreparedImport {
    source: Vec<u8>,
    report: ImportReport,
}
impl PreparedImport {
    pub fn read(path: &Path, format: LegacyFormat) -> Result<Self> {
        Self::parse(&session::read_bounded(path, SOURCE_LIMIT)?, format)
    }
    pub fn report(&self) -> &ImportReport {
        &self.report
    }
    pub fn parse(bytes: &[u8], format: LegacyFormat) -> Result<Self> {
        if bytes.len() > SOURCE_LIMIT {
            return Err(error("legacy input exceeds 4 MiB"));
        }
        let mut report = ImportReport {
            format_version: 1,
            source_sha256: session::hash(bytes),
            source_bytes: bytes.len(),
            source_format: None,
            source_profile: None,
            importable: false,
            trust: "historical_unverified".into(),
            entries: Vec::new(),
            anomalies: Vec::new(),
        };
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                report.issue(None, "error", "invalid_utf8", "journal must be UTF-8");
                return Ok(Self {
                    source: bytes.to_vec(),
                    report,
                });
            }
        };
        let mut selected = if format == LegacyFormat::Auto {
            None
        } else {
            Some(format)
        };
        let mut segment = 0;
        let mut has_config = false;
        let mut last_ordinal = 0;
        let mut runs = 0;
        let mut portable_version = None;
        for (offset, line) in text.lines().enumerate() {
            let number = offset + 1;
            if number > LINE_COUNT {
                report.issue(
                    Some(number),
                    "error",
                    "too_many_lines",
                    "journal exceeds 4096 lines",
                );
                break;
            }
            if line.len() > LINE_LIMIT {
                report.issue(
                    Some(number),
                    "error",
                    "line_too_large",
                    "record exceeds 64 KiB",
                );
                continue;
            }
            if line.trim().is_empty() {
                report.issue(
                    Some(number),
                    "warning",
                    "blank_line",
                    "blank line preserved in original copy",
                );
                continue;
            }
            let value = match serde_json::from_str::<Unique>(line) {
                Ok(Unique(Value::Object(object))) => Value::Object(object),
                _ => {
                    report.issue(Some(number), "error", "invalid_json", "record must be a complete JSON object with distinct keys and finite numbers");
                    continue;
                }
            };
            let detected = if value.get("type").and_then(Value::as_str) == Some("config")
                || value.get("run").is_some()
            {
                Some(LegacyFormat::Pi)
            } else if value.get("iteration").is_some() {
                Some(LegacyFormat::Portable)
            } else {
                None
            };
            if selected.is_none() {
                selected = detected;
            }
            if selected.is_none() || detected != selected {
                report.issue(
                    Some(number),
                    "error",
                    "unknown_or_mixed_format",
                    "record does not match a supported journal format",
                );
                continue;
            }
            report.source_format = selected;
            let result = match selected.unwrap() {
                LegacyFormat::Pi => {
                    report.source_profile = Some("pi_unversioned".into());
                    if value.get("schema_version").is_some() || value.get("version").is_some() {
                        Err((
                            "unsupported_version",
                            "Pi import supports only the observed unversioned archive format",
                        ))
                    } else if value.get("type").and_then(Value::as_str) == Some("config") {
                        validate_pi_config(&value).map(|()| {
                            if has_config && runs > 0 {
                                segment += 1;
                            }
                            has_config = true;
                            ("config", None)
                        })
                    } else if !has_config {
                        Err((
                            "missing_config",
                            "Pi runs require a preceding explicit metric configuration",
                        ))
                    } else {
                        validate_pi_run(&value, segment).map(|ordinal| ("run", Some(ordinal)))
                    }
                }
                LegacyFormat::Portable => {
                    let version = value.get("schema_version");
                    if version.is_some() && version.and_then(Value::as_u64) != Some(1) {
                        Err((
                            "unsupported_version",
                            "only portable schema_version 1 or an unversioned record is supported",
                        ))
                    } else if portable_version.is_some_and(|prior| prior != version.is_some()) {
                        Err((
                            "mixed_versions",
                            "portable version markers must be consistent within one journal",
                        ))
                    } else {
                        portable_version = Some(version.is_some());
                        report.source_profile = Some(
                            if version.is_some() {
                                "portable_v1"
                            } else {
                                "portable_unversioned"
                            }
                            .into(),
                        );
                        validate_portable(&value, version.is_some())
                            .map(|ordinal| ("run", Some(ordinal)))
                    }
                }
                LegacyFormat::Auto => unreachable!(),
            };
            match result {
                Err((code, message)) => report.issue(Some(number), "error", code, message),
                Ok((kind, ordinal)) => {
                    if let Some(ordinal) = ordinal {
                        if ordinal <= last_ordinal {
                            report.issue(
                                Some(number),
                                "error",
                                "invalid_sequence",
                                "run/iteration numbers must be positive and strictly increasing",
                            );
                            continue;
                        }
                        last_ordinal = ordinal;
                        runs += 1;
                    }
                    if selected == Some(LegacyFormat::Portable)
                        && portable_version == Some(false)
                        && [
                            "metric",
                            "unit",
                            "direction",
                            "baseline_samples",
                            "candidate_samples",
                            "head",
                            "check_exit_status",
                        ]
                        .iter()
                        .any(|key| value.get(*key).is_none())
                    {
                        report.issue(Some(number), "warning", "incomplete_context", "unversioned portable context is incomplete; missing facts remain absent");
                    }
                    report.entries.push(HistoricalEntry {
                        line: number,
                        segment,
                        kind: kind.into(),
                        trust: "historical_unverified".into(),
                        declared: value,
                    });
                }
            }
        }
        if runs == 0 {
            report.issue(
                None,
                "error",
                "no_runs",
                "journal contains no valid historical run",
            );
        }
        if !bytes.ends_with(b"\n") && !bytes.is_empty() {
            report.issue(
                None,
                "warning",
                "missing_final_newline",
                "last record has no newline; it is accepted only if complete JSON",
            );
        }
        if report
            .source_profile
            .as_deref()
            .is_some_and(|p| p.ends_with("unversioned"))
        {
            report.issue(
                None,
                "warning",
                "unversioned_source",
                "profile inferred from structure; original producer version is not known",
            );
        }
        report.issue(None, "warning", "unverified_history", "declared outcomes and metrics are historical only; no scripts, code changes or budgets are resumed");
        report.importable = !report
            .anomalies
            .iter()
            .any(|issue| issue.severity == "error");
        Ok(Self {
            source: bytes.to_vec(),
            report,
        })
    }
    pub(crate) fn report_bytes(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec_pretty(&self.report)
            .map_err(|_| error("cannot serialize historical report"))?;
        if bytes.len() > REPORT_LIMIT {
            return Err(error("historical report exceeds 8 MiB"));
        }
        Ok(bytes)
    }
    pub(crate) fn write(&self, directory: &Path, report: &[u8]) -> Result<()> {
        crate::workspace::owned_directory(directory)?;
        session::write_new(&directory.join("source.jsonl"), &self.source)?;
        session::write_new(&directory.join("report.json"), report)?;
        session::sync_directory(directory)
    }
}
impl SessionGuard {
    pub fn legacy_report(&self) -> Result<Option<ImportReport>> {
        let Some(expected) = &self.state().legacy_import else {
            return Ok(None);
        };
        let directory = self.owned_path().join("legacy");
        session::check_directory(&directory)?;
        let bytes = session::read_bounded(&directory.join("report.json"), REPORT_LIMIT)?;
        if &session::hash(&bytes) != expected {
            return Err(SessionError::new(
                "corrupt_artifact",
                "historical report differs from the journal",
            ));
        }
        let report: ImportReport = serde_json::from_slice(&bytes)
            .map_err(|_| SessionError::new("corrupt_artifact", "invalid historical report"))?;
        let source = session::read_bounded(&directory.join("source.jsonl"), SOURCE_LIMIT)?;
        if report.format_version != 1
            || !report.importable
            || report.trust != "historical_unverified"
            || report.source_bytes != source.len()
            || report.source_sha256 != session::hash(&source)
        {
            return Err(SessionError::new(
                "corrupt_artifact",
                "historical source or report is inconsistent",
            ));
        }
        Ok(Some(report))
    }
}

type Validation<T> = std::result::Result<T, (&'static str, &'static str)>;
fn invalid<T>() -> Validation<T> {
    Err((
        "invalid_record",
        "record has missing, unsupported or invalid fields",
    ))
}
fn known(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .unwrap()
        .keys()
        .all(|key| fields.contains(&key.as_str()))
}
fn string(v: &Value, key: &str, min: usize, max: usize) -> bool {
    v.get(key)
        .and_then(Value::as_str)
        .is_some_and(|s| (min..=max).contains(&s.len()))
}
fn direction(v: &Value, key: &str) -> bool {
    matches!(v.get(key).and_then(Value::as_str), Some("lower" | "higher"))
}
fn finite(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
}
fn timestamp(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n > 0 && n <= i64::MAX as u64)
}
fn hex(s: &str, sizes: &[usize]) -> bool {
    sizes.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate_pi_config(v: &Value) -> Validation<()> {
    if !known(
        v,
        &["type", "name", "metricName", "metricUnit", "bestDirection"],
    ) || !string(v, "metricName", 1, 128)
        || !string(v, "metricUnit", 0, 32)
        || !direction(v, "bestDirection")
        || v.get("name")
            .is_some_and(|n| !n.is_null() && !n.as_str().is_some_and(|s| s.len() <= 4096))
    {
        return invalid();
    }
    Ok(())
}
fn validate_pi_run(v: &Value, segment: u64) -> Validation<u64> {
    if !matches!(
        v.get("status").and_then(Value::as_str),
        Some("keep" | "discard" | "crash" | "checks_failed")
    ) {
        return Err((
            "unknown_status",
            "Pi status must be keep, discard, crash or checks_failed",
        ));
    }
    if !known(
        v,
        &[
            "run",
            "commit",
            "metric",
            "metrics",
            "status",
            "description",
            "timestamp",
            "segment",
            "confidence",
            "asi",
        ],
    ) || !v.get("metric").is_some_and(finite)
        || !v.get("timestamp").is_some_and(timestamp)
        || !v.get("commit").and_then(Value::as_str).is_some_and(|s| {
            s.is_empty()
                || ((7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit()))
        })
        || !string(v, "description", 0, 4096)
        || v.get("segment")
            .is_some_and(|n| n.as_u64() != Some(segment))
        || v.get("confidence")
            .is_some_and(|n| !n.is_null() && !n.as_f64().is_some_and(|x| (0.0..=1.0).contains(&x)))
        || v.get("metrics").is_some_and(|m| {
            !m.as_object().is_some_and(|map| {
                map.len() <= 128
                    && map
                        .iter()
                        .all(|(k, v)| !k.is_empty() && k.len() <= 128 && finite(v))
            })
        })
        || v.get("asi").is_some_and(|v| !v.is_object())
    {
        return invalid();
    }
    v.get("run")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or(("invalid_sequence", "run must be a positive integer"))
}
fn samples(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|a| a.len() <= 1024 && a.iter().all(finite))
}
fn validate_portable(v: &Value, versioned: bool) -> Validation<u64> {
    if !matches!(
        v.get("decision").and_then(Value::as_str),
        Some("keep" | "discard" | "inconclusive")
    ) {
        return Err((
            "unknown_status",
            "portable decision must be keep, discard or inconclusive",
        ));
    }
    let fields = [
        "schema_version",
        "iteration",
        "timestamp_unix_ms",
        "timestamp",
        "hypothesis",
        "head",
        "changed_paths",
        "baseline_samples",
        "candidate_samples",
        "metric",
        "unit",
        "direction",
        "check_exit_status",
        "decision",
        "reason",
        "elapsed_seconds",
    ];
    if !known(v, &fields) {
        return invalid();
    }
    if versioned
        && [
            "timestamp_unix_ms",
            "hypothesis",
            "head",
            "changed_paths",
            "baseline_samples",
            "candidate_samples",
            "metric",
            "unit",
            "direction",
            "check_exit_status",
            "reason",
            "elapsed_seconds",
        ]
        .iter()
        .any(|key| v.get(*key).is_none())
    {
        return invalid();
    }
    if v.get("timestamp").is_some() && v.get("timestamp_unix_ms").is_some() {
        return invalid();
    }
    if v.get("timestamp_unix_ms").is_some_and(|n| !timestamp(n))
        || v.get("timestamp")
            .is_some_and(|n| !n.as_str().is_some_and(utc_timestamp))
        || ["hypothesis", "reason"].iter().any(|k| {
            v.get(*k)
                .is_some_and(|x| !x.as_str().is_some_and(|s| s.len() <= 4096))
        })
        || v.get("head")
            .is_some_and(|h| !h.as_str().is_some_and(|s| hex(s, &[40, 64])))
        || v.get("changed_paths").is_some_and(|x| {
            !x.as_array().is_some_and(|a| {
                a.len() <= 1024
                    && a.iter()
                        .all(|p| p.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 4096))
            })
        })
        || ["baseline_samples", "candidate_samples"]
            .iter()
            .any(|k| v.get(*k).is_some_and(|a| !samples(a)))
        || v.get("metric")
            .is_some_and(|_| !string(v, "metric", 1, 128))
        || v.get("unit").is_some_and(|_| !string(v, "unit", 0, 32))
        || v.get("direction")
            .is_some_and(|_| !direction(v, "direction"))
        || v.get("check_exit_status")
            .is_some_and(|n| !n.as_i64().is_some_and(|x| i32::try_from(x).is_ok()))
        || v.get("elapsed_seconds")
            .is_some_and(|n| !n.as_f64().is_some_and(|x| x.is_finite() && x >= 0.0))
    {
        return invalid();
    }
    if v["decision"] == "keep"
        && v.get("check_exit_status")
            .is_some_and(|x| x.as_i64() != Some(0))
    {
        return Err((
            "inconsistent_verdict",
            "portable keep declares a failed behavior check",
        ));
    }
    if versioned
        && v["decision"] == "keep"
        && ["baseline_samples", "candidate_samples"]
            .iter()
            .any(|k| v[*k].as_array().unwrap().is_empty())
    {
        return invalid();
    }
    v.get("iteration")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or(("invalid_sequence", "iteration must be a positive integer"))
}
fn utc_timestamp(text: &str) -> bool {
    // Legacy portable UTC strings are retained, not converted or used as engine clocks.
    let b = text.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b.last() != Some(&b'Z')
    {
        return false;
    }
    let number = |start, end| {
        std::str::from_utf8(&b[start..end]).ok().and_then(|s| {
            if s.bytes().all(|c| c.is_ascii_digit()) {
                s.parse::<u32>().ok()
            } else {
                None
            }
        })
    };
    let (Some(y), Some(m), Some(d), Some(h), Some(min), Some(sec)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    let days = match m {
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    y >= 1970
        && d > 0
        && d <= days
        && h < 24
        && min < 60
        && sec < 60
        && (b.len() == 20
            || ((22..=30).contains(&b.len())
                && b[19] == b'.'
                && b[20..b.len() - 1].iter().all(u8::is_ascii_digit)))
}
fn error(message: &str) -> SessionError {
    SessionError::new("invalid_legacy", message)
}

// Preserve JSON's type information while rejecting duplicates at every depth.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Json;
        impl<'de> Visitor<'de> for Json {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON with distinct object keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
                Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = Map::new();
                while let Some((k, Unique(v))) = a.next_entry::<String, Unique>()? {
                    if values.insert(k, v).is_some() {
                        return Err(de::Error::custom("duplicate object key"));
                    }
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        d.deserialize_any(Json)
    }
}
