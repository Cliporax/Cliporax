// This file aggregates all backend tests
mod db;
mod clipboard;
mod commands;

// Re-export tests so they can be run together
#[cfg(test)]
mod backend_tests {
    use super::*;

    #[tokio::test]
    async fn run_all_backend_tests() {
        // This is just a placeholder to ensure the test modules are compiled
        // Individual tests are in their respective modules
        println!("Running backend tests...");
    }
}