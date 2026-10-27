//! GNOME Online Accounts: a Nextcloud or Google account set up in Settings, used instead of
//! signing in here.

use gtk::{gio, glib};
use todav_core::AccountKind;

const BUS: &str = "org.gnome.OnlineAccounts";
const ACCOUNT: &str = "org.gnome.OnlineAccounts.Account";

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    /// GOA's account id, kept as the device-only `goa_account` setting.
    pub id: String,
    /// "user@host", as GNOME shows it.
    pub name: String,
    pub kind: AccountKind,
    /// Nextcloud: the server; Google: the user's CalDAV principal.
    pub url: String,
    pub user: String,
}

fn bus() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
}

/// Nextcloud accounts with calendars and tasks turned on and Google accounts with calendars
/// turned on, by name. Empty without GOA.
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
        let kind = match text(ACCOUNT, "ProviderType").as_deref() {
            Some("owncloud") if !off("TodoDisabled") => AccountKind::Nextcloud,
            Some("google") => AccountKind::Google,
            _ => continue,
        };
        if off("CalendarDisabled") {
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
            url: match kind {
                AccountKind::Google => uri,
                _ => server_url(&uri),
            },
            kind,
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

/// What account `id` signs in with: Nextcloud's app password (GOA keeps it in the keyring), or
/// Google's access token, valid for at least ten more minutes (GOA renews it).
pub fn password(id: &str, kind: AccountKind) -> Option<String> {
    let (iface, method, args, reply) = match kind {
        AccountKind::Google => ("OAuth2Based", "GetAccessToken", None, "(si)"),
        _ => (
            "PasswordBased",
            "GetPassword",
            Some(glib::Variant::from(("",))),
            "(s)",
        ),
    };
    bus()?
        .call_sync(
            Some(BUS),
            &format!("/org/gnome/OnlineAccounts/Accounts/{id}"),
            &format!("org.gnome.OnlineAccounts.{iface}"),
            method,
            args.as_ref(),
            glib::VariantTy::new(reply).ok(),
            gio::DBusCallFlags::NONE,
            30000,
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
