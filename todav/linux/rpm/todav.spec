# Todav for Fedora, built on COPR from .copr/Makefile (the crates vendored next to the sources;
# rust-libsecret is not in Fedora yet). scripts/release.py sets the version and adds the
# changelog entry.
%bcond check 1

Name:           todav
Version:        2.0.0
Release:        1%{?dist}
Summary:        Tasks and calendar kept in your Nextcloud

SourceLicense:  GPL-3.0-or-later
# The app, and the crates linked into it (%%cargo_license_summary; LICENSE.dependencies lists
# them by crate). Check it again when the dependencies change.
License:        GPL-3.0-or-later AND Apache-2.0 AND BSD-3-Clause AND CDLA-Permissive-2.0 AND ISC AND MIT AND Unicode-3.0 AND Zlib AND (0BSD OR MIT OR Apache-2.0) AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR MIT) AND (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT) AND (MIT OR Zlib OR Apache-2.0) AND (Unlicense OR MIT)
URL:            https://github.com/AperteDev/todav
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
# cargo vendor --locked, as .copr/Makefile makes it.
Source1:        %{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo-rpm-macros >= 25
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  gettext
BuildRequires:  blueprint-compiler
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1) >= 1.8
BuildRequires:  pkgconfig(libsecret-1)
BuildRequires:  pkgconfig(sqlite3)
BuildRequires:  glib2-devel
BuildRequires:  desktop-file-utils
BuildRequires:  appstream

# The UI files use Adw.ShortcutsDialog; the library's soname does not say which 1.x it is.
Requires:       libadwaita%{?_isa} >= 1.8
Recommends:     gnome-shell-extension-todav
Suggests:       gnome-online-accounts

%description
Todav is a task list and calendar kept in your Nextcloud through CalDAV, so
the Nextcloud Tasks and Calendar web apps and other CalDAV clients see the
same lists and events. It works offline, brings changes from other devices
within seconds through push, and runs in the background for reminders.

%package -n gnome-shell-extension-todav
Summary:        GNOME Shell extension for Todav: tasks and calendar in the top bar
License:        GPL-3.0-or-later
BuildArch:      noarch
Requires:       gnome-shell >= 49
Requires:       %{name} = %{version}-%{release}

%description -n gnome-shell-extension-todav
A task list, a month calendar and the coming week's agenda from Todav in the
top bar; clicking an item opens it in the app. Its options are in Todav's
Preferences.

%prep
%autosetup -p1 -a1
%cargo_prep -v vendor

%build
%cargo_build -- -p todav-linux
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%cargo_vendor_manifest

%install
make -C todav/linux install-data PREFIX=%{_prefix} DESTDIR=%{buildroot}
install -Dpm644 todav/linux/data/dev.aperte.Todav-autostart.desktop \
  %{buildroot}%{_sysconfdir}/xdg/autostart/dev.aperte.Todav.desktop
make -C todav/gnome-extension install \
  DEST=%{buildroot}%{_datadir}/gnome-shell/extensions/todav@aperte.dev
%find_lang %{name}
# The extension's catalogs (same domain) belong to its package.
sed -i '\|/gnome-shell/extensions/|d' %{name}.lang

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/dev.aperte.Todav.desktop
appstreamcli validate --no-net %{buildroot}%{_metainfodir}/dev.aperte.Todav.metainfo.xml
%if %{with check}
# Unit tests; the integration tests need a Nextcloud account.
%cargo_test -- -p todav-core -p todav-linux
%endif

%files -f %{name}.lang
%license LICENSE LICENSE.dependencies cargo-vendor.txt
%doc README.md docs/CHANGELOG.md
%{_bindir}/todav-linux
%{_datadir}/applications/dev.aperte.Todav.desktop
%{_metainfodir}/dev.aperte.Todav.metainfo.xml
%{_datadir}/dbus-1/services/dev.aperte.Todav.service
%{_datadir}/gnome-shell/search-providers/dev.aperte.Todav.search-provider.ini
%{_datadir}/icons/hicolor/scalable/apps/dev.aperte.Todav.svg
%{_datadir}/icons/hicolor/symbolic/apps/dev.aperte.Todav-symbolic.svg
%config(noreplace) %{_sysconfdir}/xdg/autostart/dev.aperte.Todav.desktop

%files -n gnome-shell-extension-todav
%license LICENSE
%{_datadir}/gnome-shell/extensions/todav@aperte.dev/

%changelog
* Wed Oct 28 2026 SelfRef <git@selfref.dev> - 2.0.0-1
- See CHANGELOG.md

* Mon Oct 26 2026 SelfRef <git@selfref.dev> - 1.3.1-1
- See CHANGELOG.md

* Mon Oct 26 2026 SelfRef <git@selfref.dev> - 1.3.0-1
- See CHANGELOG.md

* Sun Oct 25 2026 SelfRef <git@selfref.dev> - 1.2.2-1
- See CHANGELOG.md

* Sun Oct 25 2026 SelfRef <git@selfref.dev> - 1.2.1-1
- See CHANGELOG.md
