//! Streaming accumulators. Nothing here keeps the values themselves.

use std::collections::HashMap;

use serde::Serialize;

/// Number of histogram bins. Fixed so the SVG layout is predictable.
pub const HISTOGRAM_BINS: usize = 24;

/// Distinct text values tracked before the counter stops admitting new keys.
/// Bounds memory on high-cardinality columns like free-form addresses.
const TOP_VALUES_CAPACITY: usize = 10_000;

/// How many of the most frequent values end up in the profile.
const TOP_VALUES_REPORTED: usize = 10;

/// Welford's online algorithm: mean and variance in one pass, without
/// the catastrophic cancellation of the naive sum-of-squares form.
#[derive(Debug, Default, Clone)]
pub struct NumericAccumulator {
    count: u64,
    mean: f64,
    m2: f64,
    min: f64,
    max: f64,
    sum: f64,
}

impl NumericAccumulator {
    pub fn push(&mut self, x: f64) {
        self.count += 1;
        if self.count == 1 {
            self.min = x;
            self.max = x;
        } else {
            self.min = self.min.min(x);
            self.max = self.max.max(x);
        }
        self.sum += x;
        let delta = x - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (x - self.mean);
    }

    pub fn range(&self) -> Option<(f64, f64)> {
        (self.count > 0).then_some((self.min, self.max))
    }

    pub fn summary(&self) -> NumericSummary {
        let stddev = (self.count > 1).then(|| (self.m2 / (self.count - 1) as f64).sqrt());
        NumericSummary {
            min: (self.count > 0).then_some(self.min),
            max: (self.count > 0).then_some(self.max),
            mean: (self.count > 0).then_some(self.mean),
            sum: (self.count > 0).then_some(self.sum),
            stddev,
        }
    }
}

/// Numeric summary. Every field is optional so an all-null column serialises
/// as `null` rather than as a misleading zero.
#[derive(Debug, Clone, Serialize)]
pub struct NumericSummary {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub sum: Option<f64>,
    pub stddev: Option<f64>,
}

/// Equal-width bins over `[min, max]`.
#[derive(Debug, Clone, Serialize)]
pub struct Histogram {
    pub min: f64,
    pub max: f64,
    pub counts: Vec<u64>,
}

impl Histogram {
    pub fn new(min: f64, max: f64) -> Self {
        Histogram {
            min,
            max,
            counts: vec![0; HISTOGRAM_BINS],
        }
    }

    pub fn push(&mut self, x: f64) {
        if !x.is_finite() || x < self.min || x > self.max {
            return;
        }
        let span = self.max - self.min;
        // A constant column collapses to a single bin.
        let bin = if span <= 0.0 {
            0
        } else {
            let scaled = (x - self.min) / span * HISTOGRAM_BINS as f64;
            (scaled as usize).min(HISTOGRAM_BINS - 1)
        };
        self.counts[bin] += 1;
    }
}

/// Length statistics plus a capped frequency table.
#[derive(Debug, Default, Clone)]
pub struct TextAccumulator {
    count: u64,
    min_len: usize,
    max_len: usize,
    total_len: u64,
    counts: HashMap<String, u64>,
    /// True once the frequency table stopped admitting new keys, which makes
    /// the distinct count a lower bound rather than an exact figure.
    saturated: bool,
}

impl TextAccumulator {
    pub fn push(&mut self, s: &str) {
        let len = s.chars().count();
        if self.count == 0 {
            self.min_len = len;
            self.max_len = len;
        } else {
            self.min_len = self.min_len.min(len);
            self.max_len = self.max_len.max(len);
        }
        self.count += 1;
        self.total_len += len as u64;

        if let Some(n) = self.counts.get_mut(s) {
            *n += 1;
        } else if self.counts.len() < TOP_VALUES_CAPACITY {
            self.counts.insert(s.to_owned(), 1);
        } else {
            self.saturated = true;
        }
    }

    pub fn summary(&self) -> TextSummary {
        let mut top: Vec<_> = self
            .counts
            .iter()
            .map(|(v, &n)| ValueCount {
                value: v.clone(),
                count: n,
            })
            .collect();
        // Descending by count, then by value so equal counts are deterministic.
        top.sort_unstable_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
        top.truncate(TOP_VALUES_REPORTED);

        TextSummary {
            min_length: (self.count > 0).then_some(self.min_len),
            max_length: (self.count > 0).then_some(self.max_len),
            mean_length: (self.count > 0).then(|| self.total_len as f64 / self.count as f64),
            distinct: self.counts.len() as u64,
            distinct_is_exact: !self.saturated,
            top_values: top,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSummary {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub mean_length: Option<f64>,
    pub distinct: u64,
    /// False when the frequency table saturated, making `distinct` a floor.
    pub distinct_is_exact: bool,
    pub top_values: Vec<ValueCount>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValueCount {
    pub value: String,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_summary_matches_hand_computation() {
        let mut acc = NumericAccumulator::default();
        for x in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
            acc.push(x);
        }
        let s = acc.summary();
        assert_eq!(s.min, Some(2.0));
        assert_eq!(s.max, Some(9.0));
        assert_eq!(s.mean, Some(5.0));
        assert_eq!(s.sum, Some(40.0));
        // Sample stddev of that classic set is sqrt(32/7).
        assert!((s.stddev.unwrap() - (32.0f64 / 7.0).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn empty_numeric_summary_is_all_none() {
        let s = NumericAccumulator::default().summary();
        assert!(s.min.is_none() && s.mean.is_none() && s.stddev.is_none());
    }

    #[test]
    fn single_value_has_no_stddev() {
        let mut acc = NumericAccumulator::default();
        acc.push(3.0);
        assert_eq!(acc.summary().stddev, None);
    }

    #[test]
    fn histogram_bins_endpoints_inclusively() {
        let mut h = Histogram::new(0.0, 10.0);
        for x in [0.0, 5.0, 10.0] {
            h.push(x);
        }
        assert_eq!(h.counts.iter().sum::<u64>(), 3);
        assert_eq!(h.counts[0], 1);
        assert_eq!(h.counts[HISTOGRAM_BINS - 1], 1);
    }

    #[test]
    fn constant_column_collapses_to_one_bin() {
        let mut h = Histogram::new(7.0, 7.0);
        h.push(7.0);
        h.push(7.0);
        assert_eq!(h.counts[0], 2);
    }

    #[test]
    fn text_top_values_are_ordered_by_frequency() {
        let mut acc = TextAccumulator::default();
        for s in ["a", "b", "b", "c", "c", "c"] {
            acc.push(s);
        }
        let s = acc.summary();
        assert_eq!(s.distinct, 3);
        assert!(s.distinct_is_exact);
        assert_eq!(s.top_values[0].value, "c");
        assert_eq!(s.top_values[0].count, 3);
        assert_eq!(s.min_length, Some(1));
    }

    #[test]
    fn text_lengths_count_characters_not_bytes() {
        let mut acc = TextAccumulator::default();
        acc.push("ação");
        assert_eq!(acc.summary().max_length, Some(4));
    }

    #[test]
    fn distinct_is_marked_inexact_once_saturated() {
        let mut acc = TextAccumulator::default();
        for i in 0..TOP_VALUES_CAPACITY + 50 {
            acc.push(&format!("v{i}"));
        }
        let s = acc.summary();
        assert_eq!(s.distinct, TOP_VALUES_CAPACITY as u64);
        assert!(!s.distinct_is_exact);
    }
}
