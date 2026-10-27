UUID = todav@aperte.dev
DEST = $(HOME)/.local/share/gnome-shell/extensions/$(UUID)
FILES = metadata.json extension.js prefs.js dbus.js stylesheet.css icons
PO = ../../po
# The extension as installed: its files and the translations compiled into locale/ (the shell
# binds the gettext domain there).
STAGE = ../../target/extension
ZIP = ../../target/$(UUID).shell-extension.zip

stage:
	rm -rf $(STAGE)
	mkdir -p $(STAGE)
	cp -r $(FILES) $(STAGE)/
	for lang in $$(grep -v '^#' $(PO)/LINGUAS); do \
	  mkdir -p $(STAGE)/locale/$$lang/LC_MESSAGES; \
	  msgfmt -o $(STAGE)/locale/$$lang/LC_MESSAGES/todav.mo $(PO)/$$lang.po; \
	done

install: stage
	mkdir -p $(DEST)
	cp -r $(STAGE)/. $(DEST)/

# The zip `gnome-extensions install` takes (CI and releases build it).
pack: stage
	rm -f $(ZIP)
	cd $(STAGE) && zip -qr $(abspath $(ZIP)) .

# Development: a folder of links to each source file, so edits apply on the next shell start.
# Not a link to this folder: removing or updating the extension (Extension Manager, an update
# from extensions.gnome.org) deletes its folder's files, which through such a link are these.
# Run again after adding a file.
link:
	@if [ -e $(DEST) ] && [ ! -L $(DEST) ] && [ ! -L $(DEST)/metadata.json ]; then \
	  echo "$(DEST) is an installed copy; run 'make uninstall' first"; exit 1; fi
	if [ -L $(DEST) ]; then rm $(DEST); fi
	for f in $$(find $(FILES) -type f); do \
	  mkdir -p $(DEST)/$$(dirname $$f); \
	  ln -sfn $(CURDIR)/$$f $(DEST)/$$f; \
	done

uninstall:
	rm -rf $(DEST)

# Nested GNOME Shell with the extension and the dev build of the app (needs mutter-devkit).
nested: link
	./run-nested.sh

.PHONY: stage install pack link uninstall nested
