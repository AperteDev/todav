//! Adding, reconnecting and removing accounts, with the app passwords the front-end keeps in
//! its platform's keyring. Blocking: call from a background thread.

use todav_core::{Account, AccountKind, Client};

/// Where a front-end keeps the app passwords of accounts signed in in Todav (an empty
/// `Account::source`), by server and user.
pub trait Keyring {
    fn store(&self, url: &str, user: &str, password: &str) -> Result<(), String>;
    fn clear(&self, url: &str, user: &str);
}

/// Why an account could not be added, reconnected or removed.
#[derive(Debug)]
pub enum Error {
    /// The account is here already.
    Exists,
    /// It is gone (removed meanwhile).
    Gone,
    /// The keyring did not take the password.
    SavePassword(String),
    /// Its edits not on the server yet could not go up first; they would be lost.
    Upload(todav_core::Error),
    /// It has edits not on the server yet and is not connected to upload them.
    NotUploaded,
    Core(todav_core::Error),
}

impl From<todav_core::Error> for Error {
    fn from(e: todav_core::Error) -> Self {
        match e {
            todav_core::Error::Exists => Error::Exists,
            e => Error::Core(e),
        }
    }
}

/// Add a Nextcloud account at `url` and connect it; `source` says where its password comes from
/// (empty: signed in in Todav, the password goes into the keyring). With `settings`, settings
/// sync through it.
pub fn add(
    c: &Client,
    keyring: &dyn Keyring,
    url: &str,
    user: &str,
    password: String,
    source: String,
    settings: bool,
) -> Result<i64, Error> {
    let url = url.trim_end_matches('/').to_string();
    let empty = source.is_empty();
    let id = c.add_account(AccountKind::Nextcloud, url.clone(), user.into(), source)?;
    if empty && let Err(e) = keyring.store(&url, user, &password) {
        let _ = c.remove_account(id);
        return Err(Error::SavePassword(e));
    }
    c.connect(id, password)?;
    if settings {
        let _ = c.set_settings_account(Some(id));
    }
    Ok(id)
}

/// Account `id` (signed in in Todav) at `url` with `password`: a new app password, or the same
/// user on a moved server (then its lists come again from there, after the edits waiting for
/// the old one went up).
pub fn reconnect(
    c: &Client,
    keyring: &dyn Keyring,
    id: i64,
    url: &str,
    password: String,
) -> Result<(), Error> {
    let a = c.account(id).ok_or(Error::Gone)?;
    let url = url.trim_end_matches('/').to_string();
    keyring
        .store(&url, &a.user, &password)
        .map_err(Error::SavePassword)?;
    if url == a.url {
        return Ok(c.connect(id, password)?);
    }
    if c.connected(id) || c.pending(Some(id)) {
        c.sync_account(id).map_err(Error::Upload)?;
    }
    c.move_account(id, url, password)?;
    keyring.clear(&a.url, &a.user);
    Ok(())
}

/// Remove account `id` from this device, with its saved password; edits waiting for its server
/// go up first, unless `force` (then they are lost).
pub fn remove(c: &Client, keyring: &dyn Keyring, id: i64, force: bool) -> Result<(), Error> {
    let Some(a) = c.account(id) else {
        return Ok(());
    };
    if a.kind == AccountKind::Nextcloud && !force {
        if c.connected(id) {
            c.sync_account(id).map_err(Error::Upload)?;
        } else if c.pending(Some(id)) {
            return Err(Error::NotUploaded);
        }
    }
    if a.kind == AccountKind::Nextcloud && a.source.is_empty() {
        keyring.clear(&a.url, &a.user);
    }
    Ok(c.remove_account(id)?)
}

/// Switched on and connected: an account that syncs.
pub fn syncing(c: &Client, a: &Account) -> bool {
    a.enabled && c.connected(a.id)
}

/// The host of a Nextcloud account's server, as its default name shows it ("user@host").
pub fn host(a: &Account) -> &str {
    let host = a.url.split_once("://").map_or(a.url.as_str(), |(_, h)| h);
    host.trim_end_matches('/')
}
