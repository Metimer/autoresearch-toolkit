//! Fixed, conservative paired comparison policy; no statistical probability is claimed.
use crate::{
    config::{ConstraintBound, Direction, MetricDomain},
    session::SessionError,
    SessionConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
type Result<T> = std::result::Result<T, SessionError>;
pub type Metrics = BTreeMap<String, f64>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricLine {
    name: String,
    value: f64,
    unit: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub median: f64,
    pub noise: f64,
    pub count: usize,
}

pub fn parse(bytes: &[u8], config: &SessionConfig) -> Result<Metrics> {
    let text = std::str::from_utf8(bytes).map_err(|_| error("benchmark stdout must be UTF-8"))?;
    let mut metrics = BTreeMap::new();
    for line in text.lines().filter(|line| line.starts_with("METRIC")) {
        let payload = line
            .strip_prefix("METRIC ")
            .ok_or_else(|| error("metric prefix must be METRIC followed by a JSON object"))?;
        let metric: MetricLine = serde_json::from_str(payload).map_err(|_| {
            error("invalid metric JSON; require distinct name, value and unit fields")
        })?;
        let (unit, domain) = if metric.name == config.metric.name {
            (&config.metric.unit, config.metric.domain)
        } else {
            let spec = config
                .secondary_constraints
                .iter()
                .find(|spec| spec.name == metric.name)
                .ok_or_else(|| error("undeclared metric"))?;
            (&spec.unit, spec.domain)
        };
        if &metric.unit != unit
            || !valid(metric.value, domain)
            || metrics.insert(metric.name, metric.value).is_some()
        {
            return Err(error(
                "duplicate metric, incompatible unit or invalid value domain",
            ));
        }
    }
    if !metrics.contains_key(&config.metric.name)
        || config
            .secondary_constraints
            .iter()
            .any(|spec| !metrics.contains_key(&spec.name))
    {
        return Err(error("required metric is missing"));
    }
    Ok(metrics)
}
fn valid(value: f64, domain: MetricDomain) -> bool {
    value.is_finite()
        && match domain {
            MetricDomain::Positive => value > 0.0,
            MetricDomain::NonNegative => value >= 0.0,
            MetricDomain::Finite => true,
        }
}
pub fn constraints(metrics: &Metrics, config: &SessionConfig) -> bool {
    config.secondary_constraints.iter().all(|spec| {
        metrics
            .get(&spec.name)
            .is_some_and(|value| match spec.bound {
                ConstraintBound::AtMost { value: limit } => *value <= limit,
                ConstraintBound::AtLeast { value: limit } => *value >= limit,
            })
    })
}
pub fn summarize(values: &[f64]) -> Result<Summary> {
    if values.len() < 3 || values.iter().any(|v| !v.is_finite()) {
        return Err(error("at least three finite observations are required"));
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = if sorted.len() % 2 == 0 {
        sorted[sorted.len() / 2 - 1] / 2.0 + sorted[sorted.len() / 2] / 2.0
    } else {
        sorted[sorted.len() / 2]
    };
    let noise = sorted[sorted.len() - 1] - sorted[0];
    if !median.is_finite() || !noise.is_finite() {
        return Err(error("metric range cannot be represented"));
    }
    Ok(Summary {
        median,
        noise,
        count: values.len(),
    })
}
pub fn paired_gain(reference: &[f64], candidate: &[f64], direction: Direction) -> Result<f64> {
    if reference.len() != candidate.len() || reference.len() < 5 {
        return Err(error("comparison requires at least five matched pairs"));
    }
    let gains: Vec<_> = reference
        .iter()
        .zip(candidate)
        .map(|(r, c)| match direction {
            Direction::Lower => r - c,
            Direction::Higher => c - r,
        })
        .collect();
    Ok(summarize(&gains)?.median)
}
fn error(message: &str) -> SessionError {
    SessionError::new("invalid_metric", message)
}
