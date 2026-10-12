//! GNOME Online Accounts: a Nextcloud account set up in Settings, used instead of signing in here.

use gtk::{gio, glib};

const BUS: &str = "org.gnome.OnlineAccounts";
const ACCOUNT: &str = "org.gnome.OnlineAccounts.Account";

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    /// GOA's account id, kept as the device-only `goa_account` setting.
    pub id: String,
    /// "user@host", as GNOME shows it.
    pub name: String,
    pub url: String,
    pub user: String,
}

fn bus() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
}

/// Nextcloud accounts with calendars and tasks turned on, by name. Empty without GOA.
pub fn accounts() -> Vec<Account> {
    let Some(reply) = bus().and_then(|b| {
        b.call_sync(
            Some(BUS),
            "/org/gnome/OnlineAccounts",
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
            None,
            glib::VariantTy::new("(a{oa{sa{sv}}})").ok(),
            gio::DBusCallFlags::NONE,
            5000,
            gio::Cancellable::NONE,
        )
        .ok()
    }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for object in reply.child_value(0).iter() {
        let ifaces = object.child_value(1);
        let prop = |iface: &str, name: &str| {
            field(&field(&ifaces, iface)?, name)?.as_variant() // a{sv} values are boxed
        };
        let text = |iface: &str, name: &str| prop(iface, name).and_then(|v| v.get::<String>());
        let off = |name: &str| {
            prop(ACCOUNT, name)
                .and_then(|v| v.get::<bool>())
                .unwrap_or(false)
        };
        // "owncloud" is GOA's provider for Nextcloud.
        if text(ACCOUNT, "ProviderType").as_deref() != Some("owncloud")
            || off("CalendarDisabled")
            || off("TodoDisabled")
        {
            continue;
        }
        let (Some(id), Some(user), Some(uri)) = (
            text(ACCOUNT, "Id"),
            text(ACCOUNT, "Identity"),
            text("org.gnome.OnlineAccounts.Calendar", "Uri"),
        ) else {
            continue;
        };
        out.push(Account {
            name: text(ACCOUNT, "PresentationIdentity").unwrap_or_else(|| user.clone()),
            url: server_url(&uri),
            id,
            user,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Value under `key` in a dictionary variant (`a{s…}`).
fn field(dict: &glib::Variant, key: &str) -> Option<glib::Variant> {
    dict.iter()
        .find(|e| e.child_value(0).str() == Some(key))
        .map(|e| e.child_value(1))
}

/// The account's app password, from GOA (which keeps it in the keyring).
pub fn password(id: &str) -> Option<String> {
    bus()?
        .call_sync(
            Some(BUS),
            &format!("/org/gnome/OnlineAccounts/Accounts/{id}"),
            "org.gnome.OnlineAccounts.PasswordBased",
            "GetPassword",
            Some(&glib::Variant::from(("",))),
            glib::VariantTy::new("(s)").ok(),
            gio::DBusCallFlags::NONE,
            5000,
            gio::Cancellable::NONE,
        )
        .ok()?
        .child_value(0)
        .get::<String>()
}

/// Server address from GOA's DAV URI: "https://user@host:443/nc/remote.php/dav/" → "https://host/nc".
fn server_url(uri: &str) -> String {
    let Ok(u) = glib::Uri::parse(uri, glib::UriFlags::NONE) else {
        return String::new();
    };
    let scheme = u.scheme();
    let port = match (scheme.as_str(), u.port()) {
        (_, -1) | ("https", 443) | ("http", 80) => String::new(),
        (_, p) => format!(":{p}"),
    };
    let path = u.path();
    let path = path
        .trim_end_matches('/')
        .trim_end_matches("/remote.php/dav");
    format!("{scheme}://{}{port}{path}", u.host().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_url_drops_user_default_port_and_dav_path() {
        let s = super::server_url;
        assert_eq!(
            s("https://tm@cloud.example:443/remote.php/dav/"),
            "https://cloud.example"
        );
        assert_eq!(
            s("https://u@host:8443/nc/remote.php/dav/"),
            "https://host:8443/nc"
        );
        assert_eq!(s("http://host/remote.php/dav"), "http://host");
    }
}
