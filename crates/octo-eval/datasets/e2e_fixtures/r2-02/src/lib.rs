pub fn greet(name: &str) -> &str {
    let greeting = format!("Hello, {}!", name);
    &greeting
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greet() {
        assert_eq!(greet("World"), "Hello, World!");
    }
}
