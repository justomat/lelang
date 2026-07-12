# Default task: list all available tasks
default:
    @just --list

# Build the project in release mode
build:
    cargo build --release

# List all available provinces
provinces:
    cargo run --release -- provinces

# Run the full pipeline for a specific province (e.g., just run-province "DKI JAKARTA")
run-province province:
    cargo run --release -- --provinces "{{province}}" full

# Run the full pipeline for all provinces (can take ~30-40 minutes)
run-all:
    cargo run --release -- full

# Export the local DuckDB database to Parquet files
export:
    cargo run --release -- export

# Run the full pipeline with a limit on pages (useful for testing)
test-run province="DKI JAKARTA" pages="2":
    cargo run -- --provinces "{{province}}" --max-pages {{pages}} full
