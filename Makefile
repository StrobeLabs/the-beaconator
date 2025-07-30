# Makefile for the-beaconator

.PHONY: help build build-release test lint fmt check clean docker-build docker-run dev

# Default target
help: ## Show this help message
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

# Development targets
dev: ## Run the application in development mode
	cargo run

build: ## Build the application in debug mode
	cargo build

build-release: ## Build the application in release mode
	cargo build --release

test: ## Run all tests
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test

test-verbose: ## Run tests with verbose output
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test -- --nocapture

# Code quality targets
lint: ## Run clippy linter
	cargo clippy -- -D warnings

fmt: ## Format code with rustfmt
	cargo fmt

fmt-check: ## Check if code is properly formatted
	cargo fmt -- --check

check: ## Run cargo check (faster than build)
	cargo check

# Comprehensive quality check
quality: fmt-check lint test ## Run all quality checks (format, lint, test)

# Cleanup targets
clean: ## Clean build artifacts
	cargo clean

clean-all: clean ## Clean everything including target directory
	rm -rf target/

# Docker targets
docker-build: ## Build Docker image
	docker build -t the-beaconator .

docker-run: ## Run Docker container (requires env vars)
	docker run --env-file .env -p 8000:8000 the-beaconator

docker-run-local: ## Run Docker container with local env file
	docker run --env-file .env.local -p 8000:8000 the-beaconator

# Documentation
docs: ## Generate and open documentation
	cargo doc --open

# Install dependencies
install: ## Install required tools
	rustup component add clippy rustfmt

# Pre-commit hook simulation
pre-commit: quality ## Run all pre-commit checks
	@echo "✅ All pre-commit checks passed!"

# Release preparation
release-prep: clean quality build-release ## Prepare for release (clean, check quality, build)
	@echo "✅ Ready for release!" 