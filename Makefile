.PHONY: check install wiki wiki-build

# Run build, test, clippy, and format check
check:
	cargo build
	cargo test
	cargo clippy -- -D warnings
	cargo fmt --check

# Install the jig CLI
install:
	cargo install --path crates/jig-cli

# Preview wiki locally at http://localhost:4000
wiki:
	cd wiki && bundle install && bundle exec jekyll serve --livereload

# Build wiki without serving
wiki-build:
	cd wiki && bundle exec jekyll build
