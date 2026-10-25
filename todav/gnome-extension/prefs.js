// The extension's settings button (Extensions app): its options are app settings, so it opens
// Todav's Preferences on the Extension page and closes; without the app, it says what is missing.

import Adw from 'gi://Adw';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk';

import {ExtensionPreferences, gettext as _} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

import {BUS_NAME, OBJECT_PATH, RELEASES} from './dbus.js';

/// The app's `show-preferences` action, through GApplication's own interface: it carries an
/// activation token, so the app's window may take the focus from this one.
function showInApp(display, page) {
    const token = display.get_app_launch_context().get_startup_notify_id(null, []);
    const platform = token ? {'activation-token': new GLib.Variant('s', token)} : {};
    const args = new GLib.Variant('(sava{sv})', ['show-preferences', [new GLib.Variant('s', page)], platform]);
    return new Promise((resolve, reject) => {
        Gio.DBus.session.call(BUS_NAME, OBJECT_PATH, 'org.freedesktop.Application', 'ActivateAction',
            args, null, Gio.DBusCallFlags.NONE, -1, null, (bus, res) => {
                try {
                    resolve(bus.call_finish(res));
                } catch (e) {
                    reject(e);
                }
            });
    });
}

export default class TodavPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const page = new Adw.PreferencesPage();
        window.add(page);
        // Asked once this window has the focus: only a focused window gets a token that lets the
        // app's take it. Not focused soon after showing (seen in a headless shell), asked anyway:
        // the app then only says it is ready.
        let asked = false;
        const open = () => {
            if (asked)
                return;
            asked = true;
            showInApp(window.get_display(), 'extension')
                .then(() => window.close())
                .catch(e => {
                    const missing = e.matches?.(Gio.DBusError, Gio.DBusError.SERVICE_UNKNOWN);
                    if (!missing)
                        logError(e, 'Todav: cannot open its Preferences');
                    page.add(missing ? notInstalled() : elsewhere());
                });
        };
        window.connect('notify::is-active', () => window.is_active && open());
        window.connect('map', () => GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
            open();
            return GLib.SOURCE_REMOVE;
        }));
    }
}

/// As the menu says it without the app: what is missing, then the way to get it.
function notInstalled() {
    const group = new Adw.PreferencesGroup({
        title: _('Todav isn’t installed'),
        description: _('This extension shows the tasks and calendar of the Todav app. Install the app to use it.'),
    });
    const get = new Gtk.Button({label: _('Get Todav'), halign: Gtk.Align.START, css_classes: ['pill', 'suggested-action']});
    get.connect('clicked', () => new Gtk.UriLauncher({uri: RELEASES}).launch(get.get_root(), null, null));
    group.add(get);
    return group;
}

/// The app is there but did not open them (an older version, say): where to find them.
function elsewhere() {
    return new Adw.PreferencesGroup({
        title: _('Options in Todav'),
        description: _('This extension’s options are in Todav’s Preferences, on the Extension page.'),
    });
}
