use std::fmt;

pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl fmt::Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Point({}, {})", self.x, self.y)
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x as i64, self.y as i64)
    }
}

pub fn format_point(p: &Point) -> String {
    format!("{}", p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_point() {
        let p = Point { x: 1.0, y: 2.0 };
        assert_eq!(format_point(&p), "(1, 2)");
    }
}
