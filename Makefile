COMMAND := $(shell sed -n '1,/^\[/s/^command *= *"\(.*\)"/\1/p' config.toml)
NAME := $(shell sed -n '1,/^\[/s/^name *= *"\(.*\)"/\1/p' config.toml)

ifeq ($(shell uname),Darwin)
DATA := $(HOME)/Library/Application Support/$(NAME)
else
DATA := $(or $(XDG_CONFIG_HOME),$(HOME)/.config)/$(COMMAND)
endif

.PHONY: run clean
run:
	CARGO_TARGET_DIR="$(CURDIR)/target/agents/$(COMMAND)" cargo run -p cli --bin $(COMMAND)

clean:
	@if pgrep -x "$(COMMAND)" >/dev/null; then echo "$(COMMAND) está rodando; pare antes de limpar"; exit 1; fi
	@test -n "$(NAME)" -a -n "$(COMMAND)"
	rm -rf "$(DATA)"
