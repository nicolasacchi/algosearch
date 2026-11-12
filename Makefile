# Default matches where `algosearch` lives on PATH (~/.local/bin).
PREFIX ?= $(HOME)/.local/bin

.PHONY: build install uninstall

build:
	cargo build --release

install: build
	mkdir -p $(PREFIX)
	install -m 755 target/release/algosearch $(PREFIX)/algosearch.new
	mv -f $(PREFIX)/algosearch.new $(PREFIX)/algosearch
	@echo "Installed algosearch to $(PREFIX)/algosearch"
	@ON_PATH="$$(command -v algosearch || true)"; \
	if [ -z "$$ON_PATH" ]; then echo "Make sure $(PREFIX) is on your PATH"; \
	elif [ "$$ON_PATH" != "$(PREFIX)/algosearch" ]; then \
	  echo "WARNING: 'algosearch' on PATH resolves to $$ON_PATH, not the binary just installed"; fi

uninstall:
	rm -f $(PREFIX)/algosearch
	@echo "Removed $(PREFIX)/algosearch"
