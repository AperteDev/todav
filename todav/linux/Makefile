# Per-user install for development; `make flatpak` builds and installs the Flatpak instead.
PREFIX ?= $(HOME)/.local
BINDIR = $(PREFIX)/bin
DATADIR = $(PREFIX)/share
AUTOSTART = $(HOME)/.config/autostart
ICONS = $(DATADIR)/icons/hicolor
ID = io.github.selfref.Todav

install:
	cargo build --release -p todav-linux
	$(MAKE) install-data
	install -Dm644 data/$(ID)-autostart.desktop $(AUTOSTART)/$(ID).desktop

# Binary and data files of an already built release; the Flatpak build uses this too (it asks
# the Background portal for autostart instead).
install-data:
	install -Dm755 ../../target/release/todav-linux $(BINDIR)/todav-linux
	install -Dm644 data/$(ID).desktop $(DATADIR)/applications/$(ID).desktop
	install -Dm644 data/$(ID).metainfo.xml $(DATADIR)/metainfo/$(ID).metainfo.xml
	mkdir -p $(DATADIR)/dbus-1/services
	sed 's|@bindir@|$(BINDIR)|' data/$(ID).service.in > $(DATADIR)/dbus-1/services/$(ID).service
	install -Dm644 data/$(ID).svg $(ICONS)/scalable/apps/$(ID).svg
	install -Dm644 data/$(ID)-symbolic.svg $(ICONS)/symbolic/apps/$(ID)-symbolic.svg

# Needs flatpak-builder (or the org.flatpak.Builder Flatpak); the build fetches crates over the
# network. ponytail: Flathub builds offline, add a generated cargo-sources.json when submitting.
FLATPAK_BUILDER ?= $(shell command -v flatpak-builder || echo flatpak run org.flatpak.Builder)
flatpak:
	$(FLATPAK_BUILDER) --user --install --force-clean --install-deps-from=flathub \
	  --state-dir=../../target/flatpak-builder ../../target/flatpak flatpak/$(ID).json

# GTK's Broadway backend draws into a web page; drag and drop crashes there.
# The page opens once the app has drawn: a browser connecting earlier aborts broadwayd
# (append_node_ops assertion). ponytail: fixed 2 s wait, raise it if a cold start is slower.
# Animations are off (GTK settings in target/broadway): an animating expander row aborts it too.
# A running instance is quit first: the app is single-instance, so a new launch would only
# show the existing window. broadwayd stops with the app.
broadway:
	cargo build -p todav-linux
	busctl --user --auto-start=no call $(ID) /io/github/selfref/Todav org.gtk.Actions Activate 'sava{sv}' quit 0 0 2>/dev/null || true
	for i in $$(seq 50); do busctl --user status $(ID) >/dev/null 2>&1 || break; sleep 0.1; done
	mkdir -p ../../target/broadway/gtk-4.0
	printf '[Settings]\ngtk-enable-animations=false\n' > ../../target/broadway/gtk-4.0/settings.ini
	gtk4-broadwayd :5 & b=$$!; trap 'kill $$b' EXIT; sleep 0.5; \
	  XDG_CONFIG_HOME=$(CURDIR)/../../target/broadway GDK_BACKEND=broadway BROADWAY_DISPLAY=:5 \
	  ../../target/debug/todav-linux & a=$$!; \
	  sleep 2; chromium --app=http://127.0.0.1:8085 >/dev/null 2>&1 & \
	  wait $$a

# A separate copy for testing other connections: own data in target/sandbox (kept between
# runs; sandbox-reset wipes it), own keyring entries, runs next to the normal app.
sandbox:
	cargo build -p todav-linux
	TODAV_SANDBOX=1 XDG_DATA_HOME=$(CURDIR)/../../target/sandbox ../../target/debug/todav-linux

sandbox-reset:
	rm -rf ../../target/sandbox

uninstall:
	rm -f $(BINDIR)/todav-linux $(DATADIR)/applications/$(ID).desktop \
	  $(DATADIR)/metainfo/$(ID).metainfo.xml \
	  $(DATADIR)/dbus-1/services/$(ID).service $(AUTOSTART)/$(ID).desktop \
	  $(ICONS)/scalable/apps/$(ID).svg $(ICONS)/symbolic/apps/$(ID)-symbolic.svg

.PHONY: install install-data flatpak uninstall broadway sandbox sandbox-reset
