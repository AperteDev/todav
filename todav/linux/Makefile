# Per-user install for development; `make flatpak` builds and installs the Flatpak instead.
PREFIX ?= $(HOME)/.local
BINDIR = $(PREFIX)/bin
DATADIR = $(PREFIX)/share
AUTOSTART = $(HOME)/.config/autostart
ICONS = $(DATADIR)/icons/hicolor
LOCALE = $(DATADIR)/locale
ID = dev.aperte.Todav
# Translations (see scripts/i18n.sh); po/its says what of the metainfo is translated.
PO = ../../po
export GETTEXTDATADIRS = $(abspath $(PO))
# The repository's workspace: from here Cargo would find the bindings' one (todav/Cargo.toml).
CARGO_BUILD = cargo build --manifest-path ../../Cargo.toml

install:
	$(CARGO_BUILD) --release -p todav-linux
	$(MAKE) install-data
	install -Dm644 data/$(ID)-autostart.desktop $(AUTOSTART)/$(ID).desktop

# Binary and data files of an already built release; the Flatpak build uses this too (it asks
# the Background portal for autostart instead). DESTDIR stages them for a package. The desktop
# file and metainfo get their translations; the catalogs go where the app looks for them
# (../share/locale from its binary).
install-data:
	install -Dm755 ../../target/release/todav-linux $(DESTDIR)$(BINDIR)/todav-linux
	mkdir -p $(DESTDIR)$(DATADIR)/applications $(DESTDIR)$(DATADIR)/metainfo
	msgfmt --desktop --template=data/$(ID).desktop -d $(PO) \
	  -o $(DESTDIR)$(DATADIR)/applications/$(ID).desktop
	msgfmt --xml --template=data/$(ID).metainfo.xml -d $(PO) \
	  -o $(DESTDIR)$(DATADIR)/metainfo/$(ID).metainfo.xml
	for lang in $$(grep -v '^#' $(PO)/LINGUAS); do \
	  mkdir -p $(DESTDIR)$(LOCALE)/$$lang/LC_MESSAGES; \
	  msgfmt -o $(DESTDIR)$(LOCALE)/$$lang/LC_MESSAGES/todav.mo $(PO)/$$lang.po; \
	done
	mkdir -p $(DESTDIR)$(DATADIR)/dbus-1/services
	sed 's|@bindir@|$(BINDIR)|' data/$(ID).service.in > $(DESTDIR)$(DATADIR)/dbus-1/services/$(ID).service
	install -Dm644 data/$(ID).search-provider.ini \
	  $(DESTDIR)$(DATADIR)/gnome-shell/search-providers/$(ID).search-provider.ini
	install -Dm644 data/$(ID).svg $(DESTDIR)$(ICONS)/scalable/apps/$(ID).svg
	install -Dm644 data/$(ID)-symbolic.svg $(DESTDIR)$(ICONS)/symbolic/apps/$(ID)-symbolic.svg

# The system-wide tree the tarball and the .deb are made of, in DESTDIR:
#   make stage PREFIX=/usr DESTDIR=$PWD/../../target/stage
# The app and its data, autostart for every session, and the GNOME Shell extension.
stage:
	$(CARGO_BUILD) --release --locked -p todav-linux
	$(MAKE) install-data
	install -Dm644 data/$(ID)-autostart.desktop $(DESTDIR)/etc/xdg/autostart/$(ID).desktop
	$(MAKE) -C ../gnome-extension install DEST=$(DESTDIR)$(DATADIR)/gnome-shell/extensions/todav@aperte.dev

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
	$(CARGO_BUILD) -p todav-linux
	busctl --user --auto-start=no call $(ID) /dev/aperte/Todav org.gtk.Actions Activate 'sava{sv}' quit 0 0 2>/dev/null || true
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
	$(CARGO_BUILD) -p todav-linux
	TODAV_SANDBOX=1 XDG_DATA_HOME=$(CURDIR)/../../target/sandbox ../../target/debug/todav-linux

sandbox-reset:
	rm -rf ../../target/sandbox

uninstall:
	rm -f $(BINDIR)/todav-linux $(DATADIR)/applications/$(ID).desktop \
	  $(DATADIR)/metainfo/$(ID).metainfo.xml \
	  $(DATADIR)/dbus-1/services/$(ID).service $(AUTOSTART)/$(ID).desktop \
	  $(DATADIR)/gnome-shell/search-providers/$(ID).search-provider.ini \
	  $(ICONS)/scalable/apps/$(ID).svg $(ICONS)/symbolic/apps/$(ID)-symbolic.svg \
	  $(LOCALE)/*/LC_MESSAGES/todav.mo

.PHONY: install install-data stage flatpak uninstall broadway sandbox sandbox-reset
