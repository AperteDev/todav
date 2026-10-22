// Todav panel indicator: a list's tasks and a month calendar, read through the app's D-Bus API;
// clicking one opens Todav there. Holds no state and does no networking of its own. Its options
// are app settings (Todav's Preferences → Extension), synced like the rest.

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
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {BUS_NAME, OBJECT_PATH, TodavProxy} from './dbus.js';

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

        const box = new St.BoxLayout({style_class: 'todav-panel'});
        // Its own copy of the app's symbolic icon (linux/data), found whether or not the app is installed.
        const icon = Gio.icon_new_for_string(`${path}/icons/io.github.selfref.Todav-symbolic.svg`);
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
                this._rebuild();
            }
        });
        this._placeholder('Starting…');
    }

    _updateState() {
        const state = this._proxy?.g_name_owner ? this._proxy.SyncState ?? '' : '';
        this._warning.visible = state.startsWith('error:');
    }

    /// The options, the lists and the one shown (`ext_list`, else the first).
    async _load() {
        const [[settings], [lists]] = await Promise.all([
            this._proxy.GetSettingsAsync(),
            this._proxy.GetListsAsync(),
        ]);
        const pinned = lists.find(l => l[0] === settings.ext_list) ?? lists[0];
        this._count.text = pinned && pinned[2] > 0 ? `${pinned[2]}` : '';
        return {on: key => settings[`ext_${key}`] !== 'false', settings, lists, pinned};
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

    /// Close the menu and have the app show something. Its window asks for attention (the shell
    /// says "is ready" rather than raise a window on a D-Bus call); asked by us, it is raised.
    _open(call) {
        this.menu.close();
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
            this._placeholder('Starting…');
        let data;
        try {
            data = await this._load();
        } catch (e) {
            this._placeholder(`Todav: ${e.message}`);
            return;
        }
        const {on, settings, pinned} = data;
        const tasks = on('tasks') || on('add_task') ? await this._tasksColumn(data) : null;
        const calendar = on('month') || on('agenda') || on('add_event') ? await this._calendarColumn(on) : null;
        this.menu.removeAll();

        // Side by side (tasks left), or one under the other.
        const double = settings.ext_columns !== 'single';
        const columns = new St.BoxLayout({style_class: 'todav-columns', orientation: double ? HORIZONTAL : VERTICAL});
        if (tasks)
            columns.add_child(tasks);
        if (tasks && calendar)
            columns.add_child(new St.Widget({style_class: double ? 'todav-vsep' : 'todav-hsep'}));
        if (calendar)
            columns.add_child(calendar);
        if (tasks || calendar) {
            // Reactive, or the shell dims it as insensitive; its own rows take the clicks.
            const item = new PopupMenu.PopupBaseMenuItem({activate: false, hover: false, can_focus: false, style_class: 'todav-item'});
            item.add_child(columns);
            this.menu.addMenuItem(item);
        }
        if (on('open')) {
            if (tasks || calendar)
                this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
            const open = new PopupMenu.PopupMenuItem('Open Todav');
            open.connect('activate', () => this._open(p => p.ShowListAsync(pinned?.[0] ?? '')));
            this.menu.addMenuItem(open);
        }
        if (!tasks && !calendar && !on('open'))
            this._placeholder('Everything is turned off in Todav’s Preferences');
    }

    /// The list's tasks, or (its header clicked) the lists to pick another; then a field to add one.
    async _tasksColumn({on, lists, pinned}) {
        const col = new St.BoxLayout({style_class: 'todav-column', orientation: VERTICAL, x_expand: true});
        if (!pinned) {
            col.add_child(label('No task lists — sign in in Todav', 'todav-empty'));
            return col;
        }
        const [href, name, , color] = pinned;
        const body = new St.ScrollView({
            style_class: 'todav-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
            x_expand: true,
        });
        const fill = async () => {
            const content = new St.BoxLayout({orientation: VERTICAL, x_expand: true});
            if (this._choosing)
                lists.forEach(l => content.add_child(this._listRow(l, l[0] === href)));
            else
                await this._fillTasks(content, href, on('subtasks'));
            body.child = content;
        };
        if (on('tasks')) {
            const arrow = new St.Icon({icon_name: 'pan-down-symbolic', style_class: 'todav-arrow'});
            const header = row([dot(color), label(name, 'todav-header-name', {x_expand: true}), arrow], 'todav-row todav-header');
            header.accessible_name = `${name}, choose another list`;
            header.connect('clicked', () => {
                this._choosing = !this._choosing;
                arrow.icon_name = this._choosing ? 'pan-up-symbolic' : 'pan-down-symbolic';
                fill().catch(e => logError(e, 'Todav: cannot show the list'));
            });
            col.add_child(header);
            col.add_child(body);
            await fill();
        }
        if (on('add_task')) {
            col.add_child(entry('Add a task…', text => {
                this._proxy.AddTaskAsync(href, text)
                    .then(() => on('tasks') && !this._choosing && fill())
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
                .then(() => this._rebuild())
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
                content.add_child(label(category || 'Other', 'todav-category'));
            }
            content.add_child(this._taskRow(t));
        }
        if (tasks.length > MAX_TASKS)
            content.add_child(label(`+ ${tasks.length - MAX_TASKS} more`, 'todav-empty'));
        else if (tasks.length === 0)
            content.add_child(label('All done', 'todav-empty'));
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
            col.add_child(entry('Add an event…', text => this._open(p => p.NewEventAsync(text))));
        return col;
    }

    /// The coming week's events and dated tasks under their days; one opens the calendar there.
    async _agenda() {
        const [items] = await this._proxy.GetAgendaAsync(AGENDA_DAYS).catch(e => {
            logError(e, 'Todav: cannot read the agenda');
            return [[]];
        });
        const content = new St.BoxLayout({orientation: VERTICAL, x_expand: true});
        const today = dayKey(GLib.DateTime.new_now_local());
        let shown = null;
        for (const [day, time, title, color, task, done] of items) {
            if (day !== shown) {
                shown = day;
                const date = GLib.DateTime.new_from_unix_utc(day).format('%A %-d %B');
                const when = ['Today', 'Tomorrow'][(day - today) / DAY];
                const heading = when ? `${when} · ${date}` : date.charAt(0).toUpperCase() + date.slice(1);
                content.add_child(label(heading, 'todav-category'));
            }
            content.add_child(this._agendaRow(day, time, title, color, task, done));
        }
        if (items.length === 0)
            content.add_child(label('Nothing planned this week', 'todav-empty'));
        const scroll = new St.ScrollView({
            style_class: 'todav-agenda',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
            x_expand: true,
        });
        scroll.child = content;
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

    /// This month (arrows turn it), with dots for what the days hold when `dots`; a day opens
    /// the calendar there.
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
                const classes = ['todav-day', isToday && 'today', dots && 'dotted'].filter(Boolean);
                const day = new St.Button({child: content, style_class: classes.join(' '), can_focus: true, x_align: CENTER});
                if (m !== first.get_month())
                    day.opacity = DIM;
                day.connect('clicked', () => this._open(p => p.ShowDayAsync(dayKey(d))));
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
    }

    disable() {
        this._indicator?.destroy();
        this._indicator = null;
    }
}
