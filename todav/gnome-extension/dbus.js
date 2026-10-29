import Gio from 'gi://Gio';

export const BUS_NAME = 'dev.aperte.Todav';
export const OBJECT_PATH = '/dev/aperte/Todav';
/// Where to get the app when it is missing.
export const RELEASES = 'https://github.com/AperteDev/todav/releases/latest';

// Keep in sync with linux/app/src/dbus.rs.
const XML = `<node>
<interface name="dev.aperte.Todav1">
  <method name="GetLists"><arg direction="out" type="a(ssus)"/></method>
  <method name="GetTasks"><arg direction="in" name="list" type="s"/><arg direction="out" type="a(sssbu)"/></method>
  <method name="AddTask"><arg direction="in" name="list" type="s"/><arg direction="in" name="summary" type="s"/><arg direction="out" type="s"/></method>
  <method name="ShowList"><arg direction="in" name="list" type="s"/></method>
  <method name="NewTask"><arg direction="in" name="list" type="s"/></method>
  <method name="ShowTask"><arg direction="in" name="uid" type="s"/></method>
  <method name="GetDays"><arg direction="in" name="from" type="x"/><arg direction="in" name="days" type="u"/><arg direction="out" type="aas"/></method>
  <method name="GetAgenda"><arg direction="in" name="days" type="u"/><arg direction="out" type="a(xsssbb)"/></method>
  <method name="GetAgendaFrom"><arg direction="in" name="from" type="x"/><arg direction="in" name="days" type="u"/><arg direction="out" type="a(xsssbbsx)"/></method>
  <method name="EditItem"><arg direction="in" name="uid" type="s"/><arg direction="in" name="task" type="b"/><arg direction="in" name="start" type="x"/></method>
  <method name="GetCounts"><arg direction="out" type="a{su}"/></method>
  <method name="ShowDay"><arg direction="in" name="at" type="x"/></method>
  <method name="NewEvent"><arg direction="in" name="summary" type="s"/></method>
  <method name="NewEventOn"><arg direction="in" name="summary" type="s"/><arg direction="in" name="at" type="x"/></method>
  <method name="GetSettings"><arg direction="out" type="a{ss}"/></method>
  <method name="SetSetting"><arg direction="in" name="key" type="s"/><arg direction="in" name="value" type="s"/></method>
  <method name="Sync"/>
  <signal name="Changed"><arg name="list" type="s"/></signal>
  <property name="SyncState" type="s" access="read"/>
</interface>
</node>`;

export const TodavProxy = Gio.DBusProxy.makeProxyWrapper(XML);
