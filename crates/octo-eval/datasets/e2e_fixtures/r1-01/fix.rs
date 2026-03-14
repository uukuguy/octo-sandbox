pub fn first_n(items: &[i32], n: usize) -> Vec<i32> {
    items.iter().take(n).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_n() {
        assert_eq!(first_n(&[1, 2, 3, 4, 5], 3), vec![1, 2, 3]);
    }

    #[test]
    fn test_first_n_empty() {
        assert_eq!(first_n(&[], 3), vec![]);
    }
}
