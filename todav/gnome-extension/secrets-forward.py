#!/usr/bin/env python3
"""Serves org.freedesktop.secrets on this session bus by forwarding to the keyring on the host
bus (address in argv[1]), so the app and Online Accounts in the nested shell get their passwords.
"""
import sys

from gi.repository import Gio, GLib

NAME = "org.freedesktop.secrets"
host = Gio.DBusConnection.new_for_address_sync(
    sys.argv[1],
    Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
    | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
    None,
    None,
)
nested = Gio.bus_get_sync(Gio.BusType.SESSION)


def forward(call):
    out = call.copy()
    out.set_destination(NAME)

    def done(_, res):
        try:
            reply = host.send_message_with_reply_finish(res).copy()
            reply.set_reply_serial(call.get_serial())
            reply.set_destination(call.get_sender())
        except GLib.Error as e:
            reply = call.new_method_error_literal("org.freedesktop.DBus.Error.Failed", e.message)
        nested.send_message(reply, Gio.DBusSendMessageFlags.NONE)

    host.send_message_with_reply(out, Gio.DBusSendMessageFlags.NONE, -1, None, done)


def on_message(_, msg, incoming):
    if incoming and msg.get_message_type() == Gio.DBusMessageType.METHOD_CALL:
        GLib.idle_add(forward, msg)  # filters run on GDBus's thread
        return None
    return msg


nested.add_filter(on_message)
# Prompt completion, collection changes, property updates.
host.signal_subscribe(
    NAME, None, None, None, None, Gio.DBusSignalFlags.NONE,
    lambda _, _sender, path, iface, signal, params: nested.emit_signal(None, path, iface, signal, params),
)
Gio.bus_own_name_on_connection(nested, NAME, Gio.BusNameOwnerFlags.NONE, None, None)
GLib.MainLoop().run()
