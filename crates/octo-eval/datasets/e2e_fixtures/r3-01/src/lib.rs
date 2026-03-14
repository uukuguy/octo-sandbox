pub struct Stats {
    pub total: i32,
    pub average: f64,
}

impl Stats {
    pub fn from_data(data: &[i32]) -> Self {
        let count = data.len() as i32;
        let sum: i32 = data.iter().sum();
        let average = if count > 0 { sum as f64 / count as f64 } else { 0.0 };
        Stats {
            count,
            average,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats() {
        let stats = Stats::from_data(&[10, 20, 30]);
        assert_eq!(stats.total, 3);
        assert!((stats.average - 20.0).abs() < f64::EPSILON);
    }
}
