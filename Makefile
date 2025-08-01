# Makefile for the-beaconator

.PHONY: help build build-release test test-unit test-integration test-parallel test-verbose test-verify lint fmt check clean docker-build docker-run dev

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

test: ## Run all tests (CI-style: unit parallel, integration single-threaded)
	@echo "Running tests CI-style..."
	@./scripts/anvil-cleanup.sh
	@echo "Running unit tests (parallel)..."
	@PATH="$$HOME/.foundry/bin:$$PATH" cargo test models::models_test -- --nocapture
	@./scripts/anvil-cleanup.sh
	@echo "Running integration tests (single-threaded)..."
	@PATH="$$HOME/.foundry/bin:$$PATH" cargo test routes -- --nocapture --test-threads=1
	@echo "All tests completed successfully ✅"

test-verbose: ## Run tests with verbose output (legacy single-threaded)
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test -- --nocapture --test-threads=1

test-unit: ## Run only unit tests (fast, parallel)
	@echo "Running unit tests in parallel..."
	@PATH="$$HOME/.foundry/bin:$$PATH" cargo test models::models_test -- --nocapture

test-integration: ## Run only integration tests (single-threaded)
	@./scripts/anvil-cleanup.sh
	@echo "Running integration tests with single thread..."
	@PATH="$$HOME/.foundry/bin:$$PATH" cargo test routes -- --nocapture --test-threads=1

test-parallel: ## Run all tests in parallel (may have race conditions)
	@./scripts/anvil-cleanup.sh
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test -- --nocapture

test-verify: ## Verify test coverage and categorization
	@echo "Verifying test coverage..."
	@TOTAL_TESTS=$$(PATH="$$HOME/.foundry/bin:$$PATH" cargo test -- --list | grep -c ": test$$"); \
	UNIT_TESTS=$$(PATH="$$HOME/.foundry/bin:$$PATH" cargo test models::models_test -- --list | grep -c ": test$$"); \
	INTEGRATION_TESTS=$$(PATH="$$HOME/.foundry/bin:$$PATH" cargo test routes -- --list | grep -c ": test$$"); \
	COVERED=$$((UNIT_TESTS + INTEGRATION_TESTS)); \
	echo "Total tests found: $$TOTAL_TESTS"; \
	echo "Unit tests: $$UNIT_TESTS"; \
	echo "Integration tests: $$INTEGRATION_TESTS"; \
	echo "Tests covered by CI: $$COVERED"; \
	if [ "$$TOTAL_TESTS" -ne "$$COVERED" ]; then \
		echo "⚠️  Warning: Some tests may not be covered by CI patterns"; \
		echo "Missing tests: $$((TOTAL_TESTS - COVERED))"; \
	else \
		echo "✅ All tests are covered by CI"; \
	fi

# Code quality targets
lint: ## Run clippy linter
	cargo clippy -- -D warnings

fmt: ## Format code with rustfmt
	cargo fmt

fmt-check: ## Check if code is properly formatted
	cargo fmt -- --check

check: ## Run cargo check and anvil cleanup (faster than build)
	cargo check
	@echo "Running anvil cleanup..."
	./scripts/anvil-cleanup.sh

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
	@echo "All pre-commit checks passed!"

# Release preparation
release-prep: clean quality build-release ## Prepare for release (clean, check quality, build)
	@echo "Ready for release!" 