UUID = todav@selfref.dev
DEST = $(HOME)/.local/share/gnome-shell/extensions/$(UUID)
FILES = metadata.json extension.js dbus.js stylesheet.css icons
ZIP = ../../target/$(UUID).shell-extension.zip

install:
	mkdir -p $(DEST)
	cp -r $(FILES) $(DEST)/

# The zip `gnome-extensions install` takes (CI and releases build it).
pack:
	mkdir -p $(dir $(ZIP))
	rm -f $(ZIP)
	zip -r $(ZIP) $(FILES)

# Development: symlink the source dir so edits apply on the next shell start.
link:
	@if [ -e $(DEST) ] && [ ! -L $(DEST) ]; then \
	  echo "$(DEST) is an installed copy; run 'make uninstall' first"; exit 1; fi
	mkdir -p $(dir $(DEST))
	ln -sfn $(CURDIR) $(DEST)

uninstall:
	rm -rf $(DEST)

# Nested GNOME Shell with the extension and the dev build of the app (needs mutter-devkit).
nested: link
	./run-nested.sh

.PHONY: install pack link uninstall nested
