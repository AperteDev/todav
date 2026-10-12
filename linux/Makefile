# Per-user install for development; Flatpak packaging comes later.
PREFIX ?= $(HOME)/.local
BINDIR = $(PREFIX)/bin
DATADIR = $(PREFIX)/share
AUTOSTART = $(HOME)/.config/autostart
ICONS = $(DATADIR)/icons/hicolor
ID = io.github.selfref.Todav

install:
	cargo build --release -p todav-linux
	install -Dm755 ../target/release/todav-linux $(BINDIR)/todav-linux
	install -Dm644 data/$(ID).desktop $(DATADIR)/applications/$(ID).desktop
	mkdir -p $(DATADIR)/dbus-1/services
	sed 's|@bindir@|$(BINDIR)|' data/$(ID).service.in > $(DATADIR)/dbus-1/services/$(ID).service
	install -Dm644 data/$(ID)-autostart.desktop $(AUTOSTART)/$(ID).desktop
	install -Dm644 data/$(ID).svg $(ICONS)/scalable/apps/$(ID).svg
	install -Dm644 data/$(ID)-symbolic.svg $(ICONS)/symbolic/apps/$(ID)-symbolic.svg

# GTK's Broadway backend draws into a web page; drag and drop crashes there.
# broadwayd stops with the app.
broadway:
	cargo build -p todav-linux
	gtk4-broadwayd :5 & b=$$!; trap 'kill $$b' EXIT; sleep 0.5; \
	  xdg-open http://127.0.0.1:8085; \
	  GDK_BACKEND=broadway BROADWAY_DISPLAY=:5 ../target/debug/todav-linux

uninstall:
	rm -f $(BINDIR)/todav-linux $(DATADIR)/applications/$(ID).desktop \
	  $(DATADIR)/dbus-1/services/$(ID).service $(AUTOSTART)/$(ID).desktop \
	  $(ICONS)/scalable/apps/$(ID).svg $(ICONS)/symbolic/apps/$(ID)-symbolic.svg

.PHONY: install uninstall broadway
