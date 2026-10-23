//! The account choice shared by the welcome page and Preferences → General: a GNOME Online
//! Accounts account (the default when there is one) or a server to sign in to with the browser.

use crate::goa;
use crate::i18n::gettext;
use crate::window::{get, view};
use adw::prelude::*;
use std::rc::Rc;

pub struct Connection {
    pub goa: adw::SwitchRow,
    pub account: adw::ComboRow,
    pub server: (gtk::DropDown, adw::EntryRow),
    accounts: Vec<goa::Account>,
}

impl Connection {
    /// GOA starts switched on when `use_goa` and an account exists, with account `selected`
    /// picked if it is still there.
    pub fn new(use_goa: bool, selected: Option<&str>) -> Rc<Connection> {
        let b = view!("connection");
        let c = Rc::new(Connection {
            goa: get(&b, "goa"),
            account: get(&b, "goa_account"),
            server: (get(&b, "server_scheme"), get(&b, "server")),
            accounts: goa::accounts(),
        });
        let names: gtk::StringList = get(&b, "goa_names");
        for a in &c.accounts {
            names.append(&a.name);
        }
        let pos = c
            .accounts
            .iter()
            .position(|a| Some(a.id.as_str()) == selected);
        c.account.set_selected(pos.unwrap_or(0) as u32);
        match c.accounts.as_slice() {
            [] => c.goa.set_subtitle(&gettext(
                "No Nextcloud account in Settings → Online Accounts",
            )),
            [one] => c.goa.set_subtitle(&one.name),
            _ => {}
        }
        c.goa.set_sensitive(!c.accounts.is_empty());
        c.goa.set_active(use_goa && !c.accounts.is_empty());
        c.update();
        let weak = Rc::downgrade(&c);
        c.goa.connect_active_notify(move |_| {
            if let Some(c) = weak.upgrade() {
                c.update();
            }
        });
        c
    }

    /// Show the account picker (with more than one account) or the server field.
    fn update(&self) {
        let goa = self.goa.is_active();
        self.account.set_visible(goa && self.accounts.len() > 1);
        self.server.1.set_visible(!goa);
    }

    pub fn attach(&self, group: &adw::PreferencesGroup) {
        group.add(&self.goa);
        group.add(&self.account);
        group.add(&self.server.1);
    }

    pub fn has_accounts(&self) -> bool {
        !self.accounts.is_empty()
    }

    /// The GOA accounts' names, for the welcome page's choice.
    pub fn account_names(&self) -> Vec<String> {
        self.accounts.iter().map(|a| a.name.clone()).collect()
    }

    /// The chosen GOA account; None means a server signed in to here.
    pub fn chosen(&self) -> Option<goa::Account> {
        self.goa
            .is_active()
            .then(|| self.accounts.get(self.account.selected() as usize).cloned())
            .flatten()
    }

    /// Run `f` when the choice changes (GOA on/off or another account).
    pub fn connect_changed(&self, f: impl Fn() + Clone + 'static) {
        let g = f.clone();
        self.goa.connect_active_notify(move |_| g());
        self.account.connect_selected_notify(move |_| f());
    }
}
