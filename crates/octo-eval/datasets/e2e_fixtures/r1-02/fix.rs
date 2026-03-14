use std::collections::HashMap;

pub fn get_value(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_existing_key() {
        let mut map = HashMap::new();
        map.insert("name".to_string(), "Alice".to_string());
        assert_eq!(get_value(&map, "name"), "Alice");
    }

    #[test]
    fn test_missing_key() {
        let map: HashMap<String, String> = HashMap::new();
        assert_eq!(get_value(&map, "missing"), "");
    }
}
