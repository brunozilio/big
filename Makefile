COMMAND := $(shell sed -n 's/^command *= *"\(.*\)"/\1/p' config.toml)

.PHONY: run
run:
	CARGO_TARGET_DIR="$(CURDIR)/target/agents/$(COMMAND)" cargo run -p cli --bin $(COMMAND)
