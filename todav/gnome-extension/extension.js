// Todav panel indicator: a list's tasks and a month calendar, read through the app's D-Bus API;
// clicking one opens Todav there. Each part shows in its own menu, in GNOME's clock menu (in
// place of its calendar and events) or nowhere. Holds no state and does no networking of its
// own. Its options are app settings (Todav's Preferences → Extension), synced like the rest.

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension, gettext as _, ngettext, pgettext} from 'resource:///org/gnome/shell/extensions/extension.js';

import {BUS_NAME, OBJECT_PATH, RELEASES, TodavProxy} from './dbus.js';

const MAX_TASKS = 100;
/// Days the agenda covers, from today.
const AGENDA_DAYS = 7;
const DAY = 86400;
const {HORIZONTAL, VERTICAL} = Clutter.Orientation;
const CENTER = Clutter.ActorAlign.CENTER;
/// What St draws dimmed (it has no CSS opacity): done tasks, other months' days, weekdays.
const DIM = 140;

function label(text, style_class, props = {}) {
    const l = new St.Label({text, style_class, y_align: CENTER, ...props});
    l.clutter_text.ellipsize = Pango.EllipsizeMode.END;
    return l;
}

/// A round mark in a list's colour, as the app shows it.
function dot(color) {
    return new St.Widget({style_class: 'todav-dot', style: `background-color: ${color};`, y_align: CENTER});
}

/// A flat, focusable row holding `children` side by side.
function row(children, style_class = 'todav-row') {
    const content = new St.BoxLayout({x_expand: true});
    children.forEach(c => content.add_child(c));
    return new St.Button({style_class, child: content, can_focus: true, x_expand: true});
}

/// The app's day key: local midnight, counted as if it were UTC (its wall-clock seconds).
function dayKey(date) {
    return GLib.DateTime.new_utc(date.get_year(), date.get_month(), date.get_day_of_month(), 0, 0, 0).to_unix();
}

/// A menu item that only holds other widgets: reactive, or the shell dims it as insensitive, but
/// never highlighted itself (`hover: false` alone still tracks hover, and themes such as Blur
/// my Shell's style any hovered menu item with !important); its own rows take the clicks.
function holder(child, style_class = null) {
    const item = new PopupMenu.PopupBaseMenuItem({activate: false, hover: false, can_focus: false, style_class});
    item.track_hover = false;
    item.add_child(child);
    return item;
}

/// Put `entry` at the bottom of `col`: side by side, both columns get the taller one's height, and
/// the extra goes to a list (`y_expand`) or, without one, to a gap above, so the fields line up.
function addAtBottom(col, entry) {
    if (!col.get_children().some(c => c.y_expand))
        col.add_child(new St.Widget({y_expand: true}));
    col.add_child(entry);
}

function entry(hint, done) {
    const e = new St.Entry({hint_text: hint, can_focus: true, x_expand: true, style_class: 'todav-entry'});
    e.clutter_text.connect('activate', () => {
        const text = e.get_text().trim();
        if (text) {
            e.set_text('');
            done(text);
        }
    });
    return e;
}

const Indicator = GObject.registerClass(
class TodavIndicator extends PanelMenu.Button {
    _init(path) {
        super._init(0.5, 'Todav');
        this.add_style_class_name('todav-button');
        this._proxy = null;
        // The day picked in the month, its agenda shown (null: the coming week's).
        this._selected = null;
        this._midnight();

        const box = new St.BoxLayout({style_class: 'todav-panel'});
        // Its own copy of the app's symbolic icon (linux/data), found whether or not the app is installed.
        const icon = Gio.icon_new_for_string(`${path}/icons/dev.aperte.Todav-symbolic.svg`);
        box.add_child(new St.Icon({gicon: icon, style_class: 'system-status-icon'}));
        this._count = new St.Label({text: '', y_align: CENTER, style_class: 'todav-count'});
        box.add_child(this._count);
        this._warning = new St.Icon({icon_name: 'dialog-warning-symbolic', style_class: 'system-status-icon', visible: false});
        box.add_child(this._warning);
        this.add_child(box);

        // Don't launch the app at Shell start; the first method call (menu open) activates it.
        new TodavProxy(Gio.DBus.session, BUS_NAME, OBJECT_PATH, (proxy, error) => {
            if (error || this._destroyed) {
                if (error)
                    logError(error, 'Todav: cannot create D-Bus proxy');
                return;
            }
            this._proxy = proxy;
            this._signalId = proxy.connectSignal('Changed', () => this._refreshCount());
            this._propsId = proxy.connect('g-properties-changed', () => this._updateState());
            this._ownerId = proxy.connect('notify::g-name-owner', () => {
                this._refreshCount();
                this._updateState();
            });
            this._refreshCount();
            this._updateState();
        }, null, Gio.DBusProxyFlags.DO_NOT_AUTO_START_AT_CONSTRUCTION);

        // Built on open: each time the lists, this month and the options as they are now.
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open) {
                this._choosing = false;
                this._month = null;
                this._selected = null;
                this._rebuild();
            }
        });
        this._placeholder(_('Starting…'));
        this._contextSetup();
        // A right click opens the context menu.
        const toggle = this.menu.toggle.bind(this.menu);
        this.menu.toggle = () => {
            if (Clutter.get_current_event()?.get_button() === Clutter.BUTTON_SECONDARY)
                this._context.toggle();
            else
                toggle();
        };
        this._clockSetup();
    }

    /// Open Todav, and the extension's preferences: Todav's, on the Extension page, asked
    /// directly (the Extensions app's settings window would only flash up to ask it).
    _contextSetup() {
        this._context = new PopupMenu.PopupMenu(this, 0.5, St.Side.TOP);
        Main.uiGroup.add_child(this._context.actor);
        this._context.actor.hide();
        Main.panel.menuManager.addMenu(this._context);
        // Highlighted while open, as for its main menu.
        this._context.connect('open-state-changed', (_menu, open) => {
            if (open)
                this.add_style_pseudo_class('active');
            else
                this.remove_style_pseudo_class('active');
        });
        this._context.addAction(_('Open Todav'), () => this._open(p => p.ShowListAsync(this._pinned ?? '')));
        const args = new GLib.Variant('(sava{sv})', ['show-preferences', [new GLib.Variant('s', 'extension')], {}]);
        this._context.addAction(_('Preferences'), () => this._open(() => Gio.DBus.session.call(BUS_NAME,
            OBJECT_PATH, 'org.freedesktop.Application', 'ActivateAction', args, null, Gio.DBusCallFlags.NONE, -1, null)));
    }

    /// GNOME's clock menu: a place for the month next to its calendar, one for the agenda and the
    /// task list after its events. Its parts are private: when they are gone, nothing goes there.
    _clockSetup() {
        const dateMenu = Main.panel.statusArea.dateMenu;
        const calendar = dateMenu?._calendar;
        const events = dateMenu?._eventsItem;
        if (!calendar?.get_parent() || !events?.get_parent())
            return;
        this._dateMenu = dateMenu;
        // A box: the month takes all its width (a bin keeps it at its own).
        this._clockMonth = new St.BoxLayout({style_class: 'todav-clock-month', orientation: VERTICAL, x_expand: true, visible: false});
        calendar.get_parent().insert_child_above(this._clockMonth, calendar);
        this._clockBox = new St.BoxLayout({orientation: VERTICAL, x_expand: true});
        events.get_parent().insert_child_above(this._clockBox, events);
        this._clockOpenId = dateMenu.menu.connect('open-state-changed', (_menu, open) => {
            if (!open)
                return;
            this._choosing = false;
            this._selected = null;
            this._clockRebuild().catch(e => logError(e, 'Todav: cannot fill the clock menu'));
        });
    }

    /// Swap GNOME's calendar and events for ours as the options say (`true` restores them).
    _clockPlace(clock) {
        if (!this._dateMenu)
            return;
        const {_calendar: calendar, _eventsItem: events} = this._dateMenu;
        calendar.visible = !clock('month');
        this._clockMonth.visible = clock('month');
        if (!clock('month')) {
            const column = calendar.get_parent();
            column.min_width_set = column.natural_width_set = false;
        }
        // The events section shows itself again as calendars change: out of the menu instead.
        const box = this._clockBox.get_parent();
        if (clock('agenda') && events.get_parent() === box)
            box.remove_child(events);
        else if (!clock('agenda') && !events.get_parent())
            box.insert_child_below(events, this._clockBox);
    }

    /// The parts set to the clock menu, as they are now. Only from a running app: opening the
    /// clock menu doesn't start it (it starts with the session).
    async _clockRebuild() {
        if (!this._dateMenu || !this._proxy?.g_name_owner)
            return;
        const data = await this._load();
        const {on, clock} = data;
        const card = children => {
            const box = new St.BoxLayout({orientation: VERTICAL, style_class: 'events-button todav-clock-card'});
            children.forEach(c => box.add_child(c));
            return box;
        };
        const cards = [];
        const event = clock('add_event') && entry(_('Add an event…'), text => this._newEvent(text));
        if (clock('agenda') || event)
            cards.push(card([clock('agenda') && await this._agenda(), event].filter(Boolean)));
        if (clock('tasks') || clock('add_task'))
            cards.push(card([await this._tasksColumn(data, clock)]));
        this._month = null;
        this._clockMonth.destroy_all_children();
        if (clock('month')) {
            // The column as wide as with the calendar: it takes the calendar's width only, which
            // hidden (or at Shell start) is less; measured shown.
            // Both widths: a natural one under the minimum makes Clutter abort.
            const calendar = this._dateMenu._calendar;
            calendar.show();
            const width = calendar.get_preferred_width(-1)[1];
            calendar.hide();
            const column = calendar.get_parent();
            column.min_width = column.natural_width = width;
            this._clockMonth.add_child(this._monthView(on('month_dots')));
        }
        this._clockBox.destroy_all_children();
        cards.forEach(c => this._clockBox.add_child(c));
    }

    /// The icon after the clock (`ext_icon`, the default), or on the right.
    _placeIcon(where) {
        const center = where !== 'right';
        // The clock's highlight is its time only, inside its button's padding (12px): moved over
        // that padding, as far from the clock as panel buttons are from one another. Drawn
        // there, laid out as before (St has no negative margins).
        const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
        this.container.translation_x = center ? -12 * scale : 0;
        const box = center ? Main.panel._centerBox : Main.panel._rightBox;
        const parent = this.container.get_parent();
        if (!box || !parent || parent === box)
            return;
        parent.remove_child(this.container);
        const clock = box.get_children().indexOf(this._dateMenu?.container);
        box.insert_child_at_index(this.container, center ? clock + 1 : 0);
    }

    /// What is due today, today's events and the agenda's days change at midnight: the number
    /// read again then (the menus, on open).
    _midnight() {
        const now = GLib.DateTime.new_now_local();
        const next = GLib.DateTime.new_local(now.get_year(), now.get_month(), now.get_day_of_month(), 0, 0, 1).add_days(1);
        this._midnightId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, Math.ceil(next.difference(now) / 1e6), () => {
            this._refreshCount();
            this._midnight();
            return GLib.SOURCE_REMOVE;
        });
    }

    _updateState() {
        const state = this._proxy?.g_name_owner ? this._proxy.SyncState ?? '' : '';
        this._warning.visible = state.startsWith('error:');
    }

    /// The options, the lists and the one shown (`ext_list`, else the first); the parts and the
    /// icon go where the options say. A part is `true` (in this menu), `clock` or `false`.
    async _load() {
        const [[settings], [lists]] = await Promise.all([
            this._proxy.GetSettingsAsync(),
            this._proxy.GetListsAsync(),
        ]);
        const pinned = lists.find(l => l[0] === settings.ext_list) ?? lists[0];
        // The number by the icon (`ext_count`): the list's open tasks, or one of GetCounts'.
        const kind = settings.ext_count ?? 'list';
        let count = kind === 'list' ? pinned?.[2] ?? 0 : 0;
        if (kind !== 'list' && kind !== 'none') {
            const [counts] = await this._proxy.GetCountsAsync().catch(e => {
                logError(e, 'Todav: cannot read the counts');
                return [{}];
            });
            count = counts[kind] ?? 0;
        }
        this._count.text = count > 0 ? `${count}` : '';
        this._pinned = pinned?.[0];
        // Without a clock menu to go in, its parts stay here.
        const at = key => settings[`ext_${key}`];
        const clock = key => Boolean(this._dateMenu) && at(key) === 'clock';
        const on = key => at(key) === 'true' || (at(key) === 'clock' && !this._dateMenu);
        this._clockPlace(clock);
        // With none of them in its menu, no icon (until then it shows, to say what is missing).
        this.container.visible = ['tasks', 'add_task', 'month', 'agenda', 'add_event'].some(on);
        this._placeIcon(settings.ext_icon);
        return {on, clock, settings, lists, pinned};
    }

    async _refreshCount() {
        if (!this._proxy?.g_name_owner) {
            this._count.text = '';
            return;
        }
        await this._load().catch(e => logError(e, 'Todav: cannot read the lists'));
    }

    _placeholder(text) {
        this.menu.removeAll();
        this.menu.addMenuItem(new PopupMenu.PopupMenuItem(text, {reactive: false}));
    }

    /// No app answers or can be started (D-Bus knows no such service): say what is missing and
    /// where to get it.
    _notInstalled() {
        this.menu.removeAll();
        const box = new St.BoxLayout({orientation: VERTICAL, style_class: 'todav-missing'});
        box.add_child(new St.Label({text: _('Todav isn’t installed'), style_class: 'todav-missing-title'}));
        const text = new St.Label({
            text: _('This extension shows the tasks and calendar of the Todav app. Install the app to use it.'),
            style_class: 'todav-missing-text',
        });
        text.clutter_text.line_wrap = true;
        box.add_child(text);
        this.menu.addMenuItem(holder(box));
        const get = new PopupMenu.PopupMenuItem(_('Get Todav'));
        get.connect('activate', () => Gio.AppInfo.launch_default_for_uri(RELEASES, global.create_app_launch_context(0, -1)));
        this.menu.addMenuItem(get);
    }

    /// Close the menu and have the app show something. Its window asks for attention (the shell
    /// says "is ready" rather than raise a window on a D-Bus call); asked by us, it is raised.
    _open(call) {
        this.menu.close();
        this._dateMenu?.menu.close();
        const id = global.display.connect('window-demands-attention', (_display, window) => {
            if (window.get_wm_class()?.startsWith(BUS_NAME))
                Main.activateWindow(window);
        });
        GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 3, () => {
            global.display.disconnect(id);
            return GLib.SOURCE_REMOVE;
        });
        call(this._proxy).catch(e => logError(e, 'Todav'));
    }

    async _rebuild() {
        if (!this._proxy)
            return;
        if (!this._proxy.g_name_owner)
            this._placeholder(_('Starting…'));
        let data;
        try {
            data = await this._load();
        } catch (e) {
            if (e.matches?.(Gio.DBusError, Gio.DBusError.SERVICE_UNKNOWN))
                this._notInstalled();
            else
                this._placeholder(`Todav: ${e.message}`);
            return;
        }
        const {on, settings, pinned} = data;
        const tasks = on('tasks') || on('add_task') ? await this._tasksColumn(data) : null;
        const calendar = on('month') || on('agenda') || on('add_event') ? await this._calendarColumn(on) : null;
        this.menu.removeAll();

        // Side by side, or one under the other; tasks first unless `ext_order` says otherwise.
        const double = settings.ext_columns !== 'single';
        const columns = new St.BoxLayout({style_class: 'todav-columns', orientation: double ? HORIZONTAL : VERTICAL});
        const [first, second] = settings.ext_order === 'calendar' ? [calendar, tasks] : [tasks, calendar];
        if (first)
            columns.add_child(first);
        if (first && second)
            columns.add_child(new St.Widget({style_class: double ? 'todav-vsep' : 'todav-hsep'}));
        if (second)
            columns.add_child(second);
        if (tasks || calendar)
            this.menu.addMenuItem(holder(columns, 'todav-item'));
        if (on('open')) {
            if (tasks || calendar)
                this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
            const open = new PopupMenu.PopupMenuItem(_('Open Todav'));
            open.connect('activate', () => this._open(p => p.ShowListAsync(pinned?.[0] ?? '')));
            this.menu.addMenuItem(open);
        }
    }

    /// The list's tasks, or (its header clicked) the lists to pick another; then a field to add
    /// one. `has` tells whether those two go here.
    async _tasksColumn({on, lists, pinned}, has = on) {
        const col = new St.BoxLayout({style_class: 'todav-column', orientation: VERTICAL, x_expand: true});
        if (!pinned) {
            col.add_child(label(_('No task lists — sign in in Todav'), 'todav-empty'));
            return col;
        }
        const [href, name, , color] = pinned;
        const body = new St.ScrollView({
            style_class: 'todav-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
            x_expand: true,
            y_expand: true,
        });
        const fill = async () => {
            const content = new St.BoxLayout({orientation: VERTICAL, x_expand: true});
            if (this._choosing)
                lists.forEach(l => content.add_child(this._listRow(l, l[0] === href)));
            else
                await this._fillTasks(content, href, on('subtasks'));
            body.child = content;
        };
        if (has('tasks')) {
            const arrow = new St.Icon({icon_name: 'pan-down-symbolic', style_class: 'todav-arrow'});
            const mark = dot(color);
            const title = label(name, 'todav-header-name', {x_expand: true});
            const header = row([mark, title, arrow], 'todav-row todav-header');
            header.accessible_name = _('%s, choose another list').replace('%s', name);
            // While the lists show, it says what they are for rather than the list they replace.
            header.connect('clicked', () => {
                this._choosing = !this._choosing;
                arrow.icon_name = this._choosing ? 'pan-up-symbolic' : 'pan-down-symbolic';
                title.text = this._choosing ? _('Choose a list') : name;
                mark.visible = !this._choosing;
                fill().catch(e => logError(e, 'Todav: cannot show the list'));
            });
            col.add_child(header);
            col.add_child(body);
            await fill();
        }
        if (has('add_task')) {
            addAtBottom(col, entry(_('Add a task…'), text => {
                this._proxy.AddTaskAsync(href, text)
                    .then(() => has('tasks') && !this._choosing && fill())
                    .catch(e => logError(e, 'Todav: AddTask failed'));
            }));
        }
        return col;
    }

    /// Picking a list shows it here and in the panel, on every device.
    _listRow([href, name, open, color], current) {
        const count = label(`${open}`, 'todav-list-count');
        const item = row([dot(color), label(name, current ? 'todav-list current' : 'todav-list', {x_expand: true}), count]);
        item.connect('clicked', () => {
            this._choosing = false;
            this._proxy.SetSettingAsync('ext_list', href)
                .then(() => (this._dateMenu?.menu.isOpen ? this._clockRebuild() : this._rebuild()))
                .catch(e => logError(e, 'Todav: cannot pick the list'));
        });
        return item;
    }

    async _fillTasks(content, href, subtasks) {
        const [all] = await this._proxy.GetTasksAsync(href);
        const tasks = all.filter(t => subtasks || t[4] === 0);
        // Category headings only when there are categories.
        const titled = tasks.some(t => t[2]);
        let category = null;
        for (const t of tasks.slice(0, MAX_TASKS)) {
            if (titled && t[2] !== category) {
                category = t[2];
                content.add_child(label(category || pgettext('category', 'Other'), 'todav-category'));
            }
            content.add_child(this._taskRow(t));
        }
        if (tasks.length > MAX_TASKS) {
            const n = tasks.length - MAX_TASKS;
            content.add_child(label(ngettext('+ %d more', '+ %d more', n).replace('%d', n), 'todav-empty'));
        } else if (tasks.length === 0) {
            content.add_child(label(_('All done'), 'todav-empty'));
        }
    }

    /// Read-only: a circle, with a check in it when done; clicking opens the task in Todav.
    _taskRow([uid, summary, , done, depth]) {
        const mark = new St.Bin({style_class: 'todav-mark', y_align: CENTER});
        if (done)
            mark.child = new St.Icon({icon_name: 'object-select-symbolic', style_class: 'todav-mark-icon'});
        const title = label(summary, done ? 'todav-task done' : 'todav-task', {x_expand: true});
        if (done)
            title.opacity = DIM;
        const item = row([mark, title]);
        item.child.style = `padding-left: ${depth * 16}px;`;
        item.connect('clicked', () => this._open(p => p.ShowTaskAsync(uid)));
        return item;
    }

    async _calendarColumn(on) {
        const col = new St.BoxLayout({style_class: 'todav-column', orientation: VERTICAL, x_expand: true});
        if (on('month'))
            col.add_child(this._monthView(on('month_dots')));
        if (on('agenda'))
            col.add_child(await this._agenda());
        if (on('add_event'))
            addAtBottom(col, entry(_('Add an event…'), text => this._newEvent(text)));
        return col;
    }

    /// The coming week's events and dated tasks under their days, or those of the day picked in
    /// the month (`_selected`, shown again by `_showAgenda`); one opens the calendar there.
    async _agenda() {
        const show = async () => {
            const picked = this._selected;
            const [items] = await (picked === null
                ? this._proxy.GetAgendaAsync(AGENDA_DAYS)
                : this._proxy.GetAgendaFromAsync(picked, 1)
            ).catch(e => {
                logError(e, 'Todav: cannot read the agenda');
                return [[]];
            });
            // Another day picked meanwhile, or the menu gone.
            if (this._selected !== picked || this._showAgenda !== show)
                return;
            const content = new St.BoxLayout({orientation: VERTICAL, x_expand: true});
            const today = dayKey(GLib.DateTime.new_now_local());
            const heading = day => {
                const date = GLib.DateTime.new_from_unix_utc(day).format('%A %-d %B');
                const when = [_('Today'), _('Tomorrow')][(day - today) / DAY];
                content.add_child(label(when ? `${when} · ${date}` : date.charAt(0).toUpperCase() + date.slice(1), 'todav-category'));
            };
            // A picked day's heading even when it holds nothing.
            let shown = picked;
            if (picked !== null)
                heading(picked);
            for (const [day, time, title, color, task, done] of items) {
                if (day !== shown) {
                    shown = day;
                    heading(day);
                }
                content.add_child(this._agendaRow(day, time, title, color, task, done));
            }
            if (items.length === 0)
                content.add_child(label(picked === null ? _('Nothing planned this week') : _('Nothing planned'), 'todav-empty'));
            scroll.child = content;
        };
        this._showAgenda = show;
        const scroll = this._agendaView = new St.ScrollView({
            style_class: 'todav-agenda',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
            x_expand: true,
            y_expand: true,
        });
        scroll.connect('destroy', () => {
            if (this._showAgenda === show)
                this._showAgenda = null;
        });
        await show();
        return scroll;
    }

    /// An event's dot or a task's circle (checked when done) in its calendar's colour, its time
    /// and title.
    _agendaRow(day, time, title, color, task, done) {
        let mark;
        if (task) {
            mark = new St.Bin({style_class: 'todav-mark', style: `border-color: ${color};`, y_align: CENTER});
            if (done)
                mark.child = new St.Icon({icon_name: 'object-select-symbolic', style_class: 'todav-mark-icon'});
        } else {
            mark = new St.Bin({style_class: 'todav-mark-slot', child: dot(color), y_align: CENTER});
        }
        const name = label(title, done ? 'todav-task done' : 'todav-task', {x_expand: true});
        if (done)
            name.opacity = DIM;
        const item = row([mark, label(time, 'todav-time'), name]);
        item.connect('clicked', () => this._open(p => p.ShowDayAsync(day)));
        return item;
    }

    /// A new event titled `text` in Todav: on the day picked in the month, else today. An app
    /// without NewEventOn opens it today.
    _newEvent(text) {
        const day = this._selected;
        this._open(p => (day === null
            ? p.NewEventAsync(text)
            : p.NewEventOnAsync(text, day).catch(() => p.NewEventAsync(text))));
    }

    /// A day of the month clicked (`days`: the month's day buttons by day): the agenda shows it.
    /// Clicked again at once (a double click), or with no agenda in sight, Todav opens there.
    _dayClicked(key, days) {
        const now = GLib.get_monotonic_time() / 1000;
        const last = this._lastClick;
        this._lastClick = {key, at: now};
        this._mouse ??= new Gio.Settings({schema_id: 'org.gnome.desktop.peripherals.mouse'});
        const again = last?.key === key && now - last.at < this._mouse.get_int('double-click');
        if (again || !this._showAgenda || !this._agendaView?.is_mapped()) {
            this._open(p => p.ShowDayAsync(key));
            return;
        }
        this._selected = key;
        days.forEach((b, k) => (k === key ? b.add_style_class_name('selected') : b.remove_style_class_name('selected')));
        this._showAgenda().catch(e => logError(e, 'Todav: cannot show the day'));
    }

    /// This month (arrows turn it), with dots for what the days hold when `dots`; a day shows
    /// its agenda, or opens the calendar there (`_dayClicked`).
    _monthView(dots) {
        const today = GLib.DateTime.new_now_local();
        this._month ??= GLib.DateTime.new_local(today.get_year(), today.get_month(), 1, 0, 0, 0);
        const view = new St.BoxLayout({style_class: 'todav-month', orientation: VERTICAL});
        const title = label('', 'todav-month-title', {x_expand: true, x_align: CENTER});
        const nav = (icon, by) => {
            const b = new St.Button({style_class: 'todav-nav', can_focus: true, child: new St.Icon({icon_name: icon, icon_size: 16})});
            b.connect('clicked', () => {
                this._month = this._month.add_months(by);
                fill();
            });
            return b;
        };
        const head = new St.BoxLayout();
        head.add_child(nav('go-previous-symbolic', -1));
        head.add_child(title);
        head.add_child(nav('go-next-symbolic', 1));
        const layout = new Clutter.GridLayout({column_homogeneous: true});
        const grid = new St.Widget({layout_manager: layout, style_class: 'todav-grid'});
        view.add_child(head);
        view.add_child(grid);

        const fill = () => {
            const first = this._month;
            const name = first.format('%OB %Y');
            title.text = name.charAt(0).toUpperCase() + name.slice(1);
            grid.destroy_all_children();
            // get_day_of_week: Monday 1 … Sunday 7; the week start: Sunday 0 … Saturday 6.
            const offset = (first.get_day_of_week() % 7 - Shell.util_get_week_start() + 7) % 7;
            const days = first.add_months(1).add_days(-1).get_day_of_month();
            const start = first.add_days(-offset);
            for (let i = 0; i < 7; i++) {
                const weekday = label(start.add_days(i).format('%a'), 'todav-weekday', {x_align: CENTER});
                weekday.opacity = DIM;
                layout.attach(weekday, i, 0, 1, 1);
            }
            const weeks = Math.ceil((offset + days) / 7);
            const marks = [];
            const buttons = new Map();
            for (let i = 0; i < weeks * 7; i++) {
                const d = start.add_days(i);
                const [y, m, n] = [d.get_year(), d.get_month(), d.get_day_of_month()];
                const isToday = y === today.get_year() && m === today.get_month() && n === today.get_day_of_month();
                const content = new St.BoxLayout({orientation: VERTICAL, x_align: CENTER, y_align: CENTER});
                content.add_child(new St.Label({text: `${n}`, x_align: CENTER}));
                // Under the number, a dot for each calendar with something that day.
                if (dots) {
                    const box = new St.BoxLayout({style_class: 'todav-day-dots', x_align: CENTER});
                    content.add_child(box);
                    marks.push([box, isToday]);
                }
                const key = dayKey(d);
                const classes = ['todav-day', isToday && 'today', dots && 'dotted', key === this._selected && 'selected'];
                const day = new St.Button({child: content, style_class: classes.filter(Boolean).join(' '), can_focus: true, x_align: CENTER});
                if (m !== first.get_month())
                    day.opacity = DIM;
                buttons.set(key, day);
                day.connect('clicked', () => this._dayClicked(key, buttons));
                layout.attach(day, i % 7, 1 + Math.floor(i / 7), 1, 1);
            }
            if (!dots)
                return;
            this._proxy.GetDaysAsync(dayKey(start), weeks * 7).then(([colors]) => {
                // Turned to another month meanwhile: those days are gone.
                if (this._month !== first)
                    return;
                colors.forEach((list, i) => list.forEach(color => {
                    // On today's fill they take its text colour (see the CSS).
                    const [box, isToday] = marks[i];
                    box.add_child(new St.Widget({style_class: 'todav-day-dot', style: isToday ? '' : `background-color: ${color};`}));
                }));
            }).catch(e => logError(e, 'Todav: cannot read the days'));
        };
        fill();
        return view;
    }

    destroy() {
        this._destroyed = true;
        GLib.source_remove(this._midnightId);
        this._context.destroy();
        if (this._dateMenu) {
            this._clockPlace(() => false);
            this._dateMenu.menu.disconnect(this._clockOpenId);
            this._clockMonth.destroy();
            this._clockBox.destroy();
        }
        if (this._proxy) {
            this._proxy.disconnectSignal(this._signalId);
            this._proxy.disconnect(this._propsId);
            this._proxy.disconnect(this._ownerId);
            this._proxy = null;
        }
        super.destroy();
    }
});

export default class TodavExtension extends Extension {
    enable() {
        this._indicator = new Indicator(this.path);
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        // Where it goes by default, before the options come.
        this._indicator._placeIcon('center');
    }

    disable() {
        this._indicator?.destroy();
        this._indicator = null;
    }
}
