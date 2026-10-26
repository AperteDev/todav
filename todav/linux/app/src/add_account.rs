//! Adding an account: first its kind (GNOME Online Accounts, a Nextcloud server, this device),
//! then the account to connect. The welcome page shows it for the first account, with this
//! device's ntfy server and whether settings sync through the account; Preferences → Accounts
//! shows it in a dialog for more.

use crate::core;
use crate::i18n::gettext;
use crate::window::{browser_login, cancel_login, get, no_server, note, url_text, view};
use crate::{Target, goa};
use adw::prelude::*;
use gtk::{gio, glib};
use std::rc::Rc;
use todav_core::AccountKind;

pub struct AddAccount {
    /// What to show: the steps, with their own toasts.
    pub root: adw::ToastOverlay,
    view: adw::NavigationView,
    /// GNOME Online Accounts' Nextcloud accounts not added yet.
    goa: Vec<goa::Account>,
    goa_account: adw::ComboRow,
    server: (gtk::DropDown, adw::EntryRow),
    ntfy: (gtk::DropDown, adw::EntryRow),
    sync_settings: adw::SwitchRow,
    connect_goa: gtk::Button,
    /// The ways to sign in to a server.
    sign_in: gtk::Box,
    login: gtk::Button,
    cancel_login: gtk::Button,
    welcome: bool,
    /// Runs with the new account's id.
    done: Box<dyn Fn(i64)>,
}

/// The steps for the welcome page (`welcome`: the first account) or Preferences; `done` runs
/// once an account is added.
pub fn new(welcome: bool, done: impl Fn(i64) + 'static) -> Rc<AddAccount> {
    let b = view!("add-account");
    let added: Vec<String> = core().accounts().into_iter().map(|a| a.source).collect();
    let a = Rc::new(AddAccount {
        root: get(&b, "root"),
        view: get(&b, "view"),
        goa: goa::accounts()
            .into_iter()
            .filter(|g| !added.contains(&g.id))
            .collect(),
        goa_account: get(&b, "goa_account"),
        server: (get(&b, "server_scheme"), get(&b, "server")),
        ntfy: (get(&b, "ntfy_scheme"), get(&b, "ntfy")),
        sync_settings: get(&b, "sync_settings"),
        connect_goa: get(&b, "connect_goa"),
        sign_in: get(&b, "sign_in"),
        login: get(&b, "login"),
        cancel_login: get(&b, "cancel_login"),
        welcome,
        done: Box::new(done),
    });
    let names: gtk::StringList = get(&b, "goa_names");
    for g in &a.goa {
        names.append(&g.name);
    }

    let use_goa: adw::ActionRow = get(&b, "use_goa");
    let names: Vec<&str> = a.goa.iter().map(|g| g.name.as_str()).collect();
    use_goa.set_subtitle(&if !names.is_empty() {
        names.join(", ")
    } else if added.iter().any(|s| !s.is_empty()) {
        gettext("Its Nextcloud accounts are all added")
    } else {
        gettext("No Nextcloud account in Settings → Online Accounts")
    });
    use_goa.set_sensitive(!a.goa.is_empty());
    let local = core()
        .accounts()
        .iter()
        .any(|a| a.kind == AccountKind::Local);
    get::<adw::ActionRow>(&b, "use_local").set_visible(!local);
    get::<gtk::Widget>(&b, "later").set_visible(welcome);
    get::<gtk::Widget>(&b, "welcome_options").set_visible(welcome);
    if !welcome {
        let intro: adw::StatusPage = get(&b, "intro");
        intro.set_icon_name(None);
        intro.set_title(&gettext("Add Account"));
        intro.set_description(Some(&gettext(
            "Tasks and calendars of another Nextcloud account, or kept on this device",
        )));
        get::<adw::NavigationPage>(&b, "choose_page").set_title(&gettext("Add Account"));
    }

    // The second step, for GNOME Online Accounts or a server.
    let page: adw::NavigationPage = get(&b, "connect_page");
    let status: adw::StatusPage = get(&b, "connect_status");
    let open = {
        let weak = Rc::downgrade(&a);
        move |goa: bool| {
            let Some(a) = weak.upgrade() else { return };
            a.goa_account.set_visible(goa);
            a.server.1.set_visible(!goa);
            a.connect_goa.set_visible(goa);
            a.sign_in.set_visible(!goa);
            if goa {
                page.set_title(&gettext("GNOME Online Accounts"));
                status.set_icon_name(Some("goa-panel-symbolic"));
                status.set_description(Some(&gettext(
                    "Connect with a Nextcloud account from GNOME Settings",
                )));
            } else {
                page.set_title(&gettext("Nextcloud Server"));
                status.set_icon_name(Some("network-server-symbolic"));
                status.set_description(Some(&gettext(
                    "Sign in to your Nextcloud; Todav gets an app password of its own",
                )));
            }
            a.view.push_by_tag("connect");
        }
    };
    let o = open.clone();
    use_goa.connect_activated(move |_| o(true));
    get::<adw::ActionRow>(&b, "use_server").connect_activated(move |_| open(false));
    let weak = Rc::downgrade(&a);
    get::<adw::ActionRow>(&b, "use_local").connect_activated(move |_| {
        let Some(a) = weak.upgrade() else { return };
        match crate::add_local() {
            Ok(id) => (a.done)(id),
            Err(e) => a.root.add_toast(note(&e)),
        }
    });

    let weak = Rc::downgrade(&a);
    a.connect_goa.connect_clicked(move |_| {
        if let Some(a) = weak.upgrade() {
            a.connect_goa();
        }
    });
    let weak = Rc::downgrade(&a);
    a.login.connect_clicked(move |_| {
        if let Some(a) = weak.upgrade() {
            a.start_login(false);
        }
    });
    let weak = Rc::downgrade(&a);
    a.cancel_login.connect_clicked(move |_| {
        cancel_login(); // the running sign-in sees it is stale and stops
        if let Some(a) = weak.upgrade() {
            a.reset_login();
        }
    });
    let actions = gio::SimpleActionGroup::new();
    let weak = Rc::downgrade(&a);
    let copy = gio::SimpleAction::new("copy-login", None);
    copy.connect_activate(move |_, _| {
        if let Some(a) = weak.upgrade() {
            a.start_login(true);
        }
    });
    actions.add_action(&copy);
    let weak = Rc::downgrade(&a);
    let password = gio::SimpleAction::new("password-login", None);
    password.connect_activate(move |_, _| {
        let Some(a) = weak.upgrade() else { return };
        let server = url_text(&a.server);
        if server.is_empty() {
            a.root.add_toast(note(&no_server()));
            return;
        }
        let weak = Rc::downgrade(&a);
        crate::window::password_login(&a.root, server, None, move |c| {
            if let Some(a) = weak.upgrade() {
                glib::spawn_future_local(async move { a.connect_with(Target::Manual(c)).await });
            }
        });
    });
    actions.add_action(&password);
    a.root.insert_action_group("add", Some(&actions));

    // Back to the first step: a sign-in waiting for the browser stops.
    let weak = Rc::downgrade(&a);
    a.view.connect_popped(move |_, _| {
        cancel_login();
        if let Some(a) = weak.upgrade() {
            a.reset_login();
        }
    });
    a
}

impl AddAccount {
    /// Sign in with the browser (Login Flow v2); `copy` puts the login link on the clipboard
    /// instead of opening the browser.
    fn start_login(self: &Rc<Self>, copy: bool) {
        let server = url_text(&self.server);
        if server.is_empty() {
            self.root.add_toast(note(&no_server()));
            return;
        }
        self.sign_in.set_visible(false);
        self.cancel_login.set_visible(true);
        let a = self.clone();
        glib::spawn_future_local(async move {
            match browser_login(server, copy).await {
                Ok(Some(c)) => a.connect_with(Target::Manual(c)).await,
                // Cancelled: Cancel already reset the page, which may be waiting for a new one.
                Ok(None) => return,
                Err(e) => a.root.add_toast(note(&e)),
            }
            a.reset_login();
        });
    }

    fn reset_login(&self) {
        self.sign_in.set_visible(true); // the GOA step hides it again when opened
        self.cancel_login.set_visible(false);
    }

    /// Connect the chosen GNOME Online Accounts account.
    fn connect_goa(self: &Rc<Self>) {
        let Some(account) = self.goa.get(self.goa_account.selected() as usize).cloned() else {
            return;
        };
        self.connect_goa.set_sensitive(false);
        let a = self.clone();
        glib::spawn_future_local(async move {
            a.connect_with(Target::Goa(account)).await;
            a.connect_goa.set_sensitive(true);
        });
    }

    async fn connect_with(&self, target: Target) {
        // Left empty: a synced ntfy server, if any, comes with the first sync.
        let ntfy = url_text(&self.ntfy);
        if self.welcome && !ntfy.is_empty() {
            let _ = core().set_setting("ntfy_url".into(), ntfy);
        }
        let settings = self.welcome && self.sync_settings.is_active();
        match crate::add_account(target, settings).await {
            Ok(id) => (self.done)(id),
            Err(e) => self.root.add_toast(note(&e)),
        }
    }
}

/// The steps in a dialog over Preferences; `done` runs once an account is added.
pub fn dialog(parent: &impl IsA<gtk::Widget>, done: impl Fn(i64) + 'static) {
    let dialog = adw::Dialog::builder()
        .content_width(460)
        .content_height(620)
        .build();
    let d = dialog.downgrade();
    let add = new(false, move |id| {
        if let Some(d) = d.upgrade() {
            d.close();
        }
        done(id);
    });
    dialog.set_child(Some(&add.root));
    // A sign-in still waiting for the browser stops with the dialog.
    dialog.connect_closed(move |_| {
        let _ = &add;
        cancel_login();
    });
    dialog.present(Some(parent));
}
