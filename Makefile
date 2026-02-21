PREFIX ?= $(HOME)/bin

.PHONY: build install uninstall

build:
	cargo build --release

install: build
	mkdir -p $(PREFIX)
	cp target/release/algosearch $(PREFIX)/algosearch
	@echo "Installed algosearch to $(PREFIX)/algosearch"
	@echo "Make sure $(PREFIX) is on your PATH"

uninstall:
	rm -f $(PREFIX)/algosearch
	@echo "Removed $(PREFIX)/algosearch"
