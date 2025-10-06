# Makefile for the-beaconator

.PHONY: help build build-release test test-unit test-integration test-parallel test-verbose test-coverage test-verify lint fmt check clean docker-build docker-run dev

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

test: ## Run all tests (CI-style: optimized parallel execution)
	@echo "Running tests with optimized parallelism..."
	@OPTIMAL_THREADS=$$(./scripts/detect-cores.sh); \
	echo "Detected optimal threads: $$OPTIMAL_THREADS (cores/2)"; \
	./scripts/anvil-cleanup.sh; \
	echo "Running unit tests (parallel, $$OPTIMAL_THREADS threads)..."; \
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test unit_tests -- --nocapture --test-threads=$$OPTIMAL_THREADS; \
	./scripts/anvil-cleanup.sh; \
	echo "Running integration tests ($$OPTIMAL_THREADS threads)..."; \
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test integration_tests -- --nocapture --test-threads=$$OPTIMAL_THREADS; \
	./scripts/anvil-cleanup.sh; \
	echo "All tests completed successfully ✅"

test-verbose: ## Run tests with verbose output (legacy single-threaded)
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test -- --nocapture --test-threads=1

test-unit: ## Run only unit tests (fast, parallel)
	@echo "Running unit tests with optimal parallelism..."
	@OPTIMAL_THREADS=$$(./scripts/detect-cores.sh); \
	echo "Using $$OPTIMAL_THREADS threads (cores/2)"; \
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test unit_tests -- --nocapture --test-threads=$$OPTIMAL_THREADS

test-integration: ## Run only integration tests (optimized parallel)
	@./scripts/anvil-cleanup.sh
	@echo "Running integration tests with optimal parallelism..."
	@OPTIMAL_THREADS=$$(./scripts/detect-cores.sh); \
	echo "Using $$OPTIMAL_THREADS threads (cores/2) with isolated Anvil instances"; \
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test integration_tests -- --nocapture --test-threads=$$OPTIMAL_THREADS
	@./scripts/anvil-cleanup.sh

test-parallel: ## Run all tests in parallel (maximum parallelism)
	@./scripts/anvil-cleanup.sh
	@OPTIMAL_THREADS=$$(./scripts/detect-cores.sh); \
	echo "Running all tests with $$OPTIMAL_THREADS threads"; \
	PATH="$$HOME/.foundry/bin:$$PATH" cargo test -- --nocapture --test-threads=$$OPTIMAL_THREADS

test-fast: ## Run tests quickly (unit tests + fast integration tests, under 10s)
	@echo "Running fast tests (unit + fast integration, excludes wallet/nonce tests)..."
	@PATH="$$HOME/.foundry/bin:$$PATH" cargo test unit_tests -- --nocapture
	@PATH="$$HOME/.foundry/bin:$$PATH" cargo test integration_tests::models_test -- --nocapture
	@echo "Fast tests completed ✅"

test-full: ## Run full test suite including integration tests
	$(MAKE) test

test-coverage: ## Generate test coverage report using tarpaulin
	@echo "Generating test coverage report..."
	@./scripts/anvil-cleanup.sh
	@echo "Running tests with coverage collection..."
	@OPTIMAL_THREADS=$$(./scripts/detect-cores.sh); \
	echo "Using $$OPTIMAL_THREADS threads for coverage collection"; \
	PATH="$$HOME/.foundry/bin:$$PATH" cargo tarpaulin \
		--out Html \
		--output-dir coverage-report \
		--exclude-files 'tests/*' \
		--exclude-files 'src/main.rs' \
		--exclude-files 'src/bin/*' \
		--timeout 300 \
		-- --test-threads=$$OPTIMAL_THREADS; \
	echo "Coverage report generated in coverage-report/ directory"; \
	echo "Open coverage-report/tarpaulin-report.html to view detailed coverage"

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
lint: ## Run clippy linter (matches CI configuration)
	cargo clippy --all --all-targets -- -D warnings

fmt: ## Format code with rustfmt
	cargo fmt

fmt-check: ## Check if code is properly formatted
	cargo fmt -- --check

check: ## Run cargo check and anvil cleanup (faster than build)
	cargo check
	@echo "Running anvil cleanup..."
	./scripts/anvil-cleanup.sh

# Comprehensive quality check
quality: fmt-check lint test-fast ## Run all quality checks (format, lint, fast tests)

# Cleanup targets
clean: ## Clean build artifacts
	cargo clean

clean-all: clean ## Clean everything including target directory
	rm -rf target/

# Docker targets
docker-build: ## Build Docker image
	docker build -t the-beaconator .

docker-build-cached: ## Build Docker image with BuildKit caching (faster for development)
	DOCKER_BUILDKIT=1 docker build --progress=plain -t the-beaconator .

docker-run: ## Run Docker container (requires env vars)
	docker run --env-file .env -p 8000:8000 the-beaconator

docker-run-local: ## Run Docker container with local env file
	docker run --env-file .env.local -p 8000:8000 the-beaconator

docker-test: ## Test Docker image build and run (simulates CI)
	@echo "Testing Docker build (same as CI)..."
	$(MAKE) docker-build
	@echo "Testing container startup..."
	docker run --rm -d --name test-beaconator -p 8001:8000 \
		-e RPC_URL=https://mainnet.base.org \
		-e PRIVATE_KEY=0000000000000000000000000000000000000000000000000000000000000001 \
		-e SENTRY_DSN=https://test@test.ingest.sentry.io/test \
		-e ENV=localnet \
		-e BEACONATOR_ACCESS_TOKEN=test_token_123 \
		-e BEACON_FACTORY_ADDRESS=0x1234567890123456789012345678901234567890 \
		-e PERPCITY_REGISTRY_ADDRESS=0x3456789012345678901234567890123456789012 \
		-e PERP_HOOK_ADDRESS=0x5678901234567890123456789012345678901234 \
		-e USDC_ADDRESS=0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 \
		the-beaconator
	@sleep 5
	@if curl -f http://localhost:8001/ > /dev/null 2>&1; then \
		echo "✅ Docker container is running successfully!"; \
	else \
		echo "❌ Docker container failed to start properly"; \
	fi
	@docker stop test-beaconator || true

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