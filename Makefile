# Makefile
BINARY_NAME=dockman
INSTALL_DIR=$(HOME)/.local/bin
DESKTOP_ENTRY_DIR=$(HOME)/.local/share/applications

build:
	cargo build --release

install: build
	mkdir -p $(INSTALL_DIR)
	cp target/release/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME)
	mkdir -p $(DESKTOP_ENTRY_DIR)
	# This installs the autostart entry
	echo "[Desktop Entry]\nName=Dockman\nExec=$(INSTALL_DIR)/$(BINARY_NAME)\nType=Application\nCategories=Utility;" > $(DESKTOP_ENTRY_DIR)/$(BINARY_NAME).desktop

uninstall:
	rm -f $(INSTALL_DIR)/$(BINARY_NAME)
	rm -f $(DESKTOP_ENTRY_DIR)/$(BINARY_NAME).desktop
