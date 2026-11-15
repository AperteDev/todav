package dev.aperte.todav

import android.Manifest
import android.app.AlarmManager
import android.app.BroadcastOptions
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dev.aperte.todav.core.Event
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.meetingLink
import dev.aperte.todav.core.reminderOccurrence
import dev.aperte.todav.core.remindsOfStart
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

private const val TAG = "Todav"

/** A week: how long the server keeps a push registration (dav_push's maximum). */
private val PUSH_EXPIRY = (7 * 86_400).toULong()

/** Run `f` after the app is ready, within a receiver's ~10 s, keeping the process alive meanwhile. */
private fun BroadcastReceiver.background(context: Context, f: suspend (TodavApp) -> Unit) {
    val app = context.applicationContext as TodavApp
    val pending = goAsync()
    app.scope.launch {
        try {
            withTimeoutOrNull(9_000) {
                app.awaitReady()
                f(app)
            }
        } catch (e: Exception) {
            Log.w(TAG, "background", e)
        } finally {
            pending.finish()
        }
    }
}

// --- push ------------------------------------------------------------------------------------

/**
 * UnifiedPush (spec AND_3), spoken directly: the distributor (ntfy, or another) gets a token to
 * register, answers with an endpoint the core registers with Nextcloud's DAV Push, and forwards
 * its pushes as they came (Web Push, encrypted for the core's keys, which decrypts them).
 */
class Push(private val app: TodavApp) {
    private val prefs = app.getSharedPreferences("push", Context.MODE_PRIVATE)

    /** Installed distributors, by package. */
    private fun distributors(): List<String> =
        app.packageManager.queryBroadcastReceivers(Intent(REGISTER), PackageManager.GET_RESOLVED_FILTER)
            .map { it.activityInfo.packageName }
            .filter { it != app.packageName }
            .distinct()

    private fun current(): String? {
        val all = distributors()
        return prefs.getString("distributor", null)?.takeIf { it in all } ?: all.firstOrNull()
    }

    /** The distributor's name, or null without one. */
    fun distributor(): String? = current()?.let { p ->
        runCatching { app.packageManager.getApplicationLabel(app.packageManager.getApplicationInfo(p, 0)).toString() }
            .getOrDefault(p)
    }

    val token: String? get() = prefs.getString("token", null)

    /** Register with the distributor (again: it answers with the same endpoint); nothing without one. */
    fun register() {
        val d = current() ?: return
        val token = token ?: UUID.randomUUID().toString().also { prefs.edit().putString("token", it).apply() }
        prefs.edit().putString("distributor", d).apply()
        send(Intent(REGISTER).setPackage(d).putExtra("token", token).putExtra("message", "Todav").putExtra("application", app.packageName))
    }

    fun ack(id: String?) {
        val d = prefs.getString("distributor", null) ?: return
        if (id != null) send(Intent(ACK).setPackage(d).putExtra("token", token).putExtra("id", id))
    }

    fun endpoint(e: String?) = prefs.edit().putString("endpoint", e).apply()

    val endpoint: String? get() = prefs.getString("endpoint", null)

    /** A token refused: the next registration takes a new one. */
    fun forget() = prefs.edit().remove("token").remove("endpoint").apply()

    /** The distributor learns who asks: shared identity (Android 14), else a pending intent. */
    private fun send(i: Intent) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            app.sendBroadcast(i, null, BroadcastOptions.makeBasic().setShareIdentityEnabled(true).toBundle())
        } else {
            val pi = PendingIntent.getBroadcast(app, 0, Intent("org.unifiedpush.dummy_app"), PendingIntent.FLAG_IMMUTABLE)
            app.sendBroadcast(i.putExtra("pi", pi))
        }
    }

    companion object {
        const val REGISTER = "org.unifiedpush.android.distributor.REGISTER"
        const val ACK = "org.unifiedpush.android.distributor.MESSAGE_ACK"
    }
}

/** What the distributor tells Todav: its endpoint, pushes, and that a registration ended. */
class PushReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val app = context.applicationContext as TodavApp
        val push = app.push
        if (intent.getStringExtra("token") != push.token) return
        when (intent.action) {
            "org.unifiedpush.android.connector.NEW_ENDPOINT" -> {
                push.ack(intent.getStringExtra("id"))
                val endpoint = intent.getStringExtra("endpoint") ?: return
                background(context) {
                    push.endpoint(endpoint)
                    it.core.pushRegister(endpoint, PUSH_EXPIRY)
                }
            }
            "org.unifiedpush.android.connector.MESSAGE" -> {
                push.ack(intent.getStringExtra("id"))
                val bytes = intent.getByteArrayExtra("bytesMessage") ?: return
                background(context) { a ->
                    for (topic in a.core.pushDecode(bytes)) a.core.syncTopic(topic)
                    Reminders.schedule(a)
                }
            }
            "org.unifiedpush.android.connector.UNREGISTERED" -> background(context) {
                push.forget()
                it.core.pushUnregister()
            }
            "org.unifiedpush.android.connector.REGISTRATION_FAILED" -> push.forget()
        }
    }
}

/** The service a distributor may bind to, to raise Todav to the foreground for a push (AND_3). */
class RaiseService : Service() {
    override fun onBind(intent: Intent?): IBinder = Binder()
}

// --- sync -------------------------------------------------------------------------------------

/**
 * The safety net for lost pushes: every 15 minutes on a network, a sync of the accounts whose own
 * interval has passed (`Client::sync_due`); it also renews the push registration (the core skips calendars whose registration has time left) and the reminder alarm.
 */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as TodavApp
        app.awaitReady()
        return try {
            app.core.syncDue()
            app.push.endpoint?.let { app.core.pushRegister(it, PUSH_EXPIRY) } ?: app.pushSetup()
            Reminders.schedule(app)
            Result.success()
        } catch (e: Exception) {
            Log.w(TAG, "sync", e)
            Result.retry()
        }
    }

    companion object {
        fun start(context: Context) {
            // As often as Android lets work run (15 minutes); each account is checked when its own
            // interval (Settings → its page) has passed. UPDATE: the old 30 minutes become 15.
            val request = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork("sync", ExistingPeriodicWorkPolicy.UPDATE, request)
        }
    }
}

// --- reminders --------------------------------------------------------------------------------

/**
 * Reminders: an exact alarm for the core's next one (within a week); when it goes off, a
 * notification for each that is due, then the alarm for the next. Set again after every change
 * and sync, at boot and when the time zone changes.
 */
object Reminders {
    const val CHANNEL = "reminders"
    const val DONE = "dev.aperte.todav.DONE"
    const val SNOOZE = "dev.aperte.todav.SNOOZE"
    const val SNOOZED = "dev.aperte.todav.SNOOZED"

    /** Stops an alarm's sound (the Stop button, and the end of its ten minutes). */
    const val STOP = "dev.aperte.todav.STOP"

    /** Sent when a reminder's alarm is over (stopped, snoozed, done, gone), to close its screen. */
    const val OVER = "dev.aperte.todav.ALARM_OVER"

    // ponytail: a fixed delay, and an alarm that a reboot forgets; a choice of delays, kept
    // snoozes, if they turn out to be wanted.
    private const val SNOOZE_SECONDS = 600L

    /**
     * Those of important tasks and events (and all, with the setting on), on a channel of their
     * own: the user can give it another sound or let it through Do Not Disturb. Android has no
     * banner that stays without a full-screen intent, which is for alarms and calls.
     */
    const val URGENT = "urgent_reminders"

    /**
     * Those that ring (`Client::alarmReminder`): the alarm sound, on the alarm volume and
     * through Do Not Disturb's Alarms exception, repeating until stopped.
     */
    const val ALARM = "alarm_reminders"

    fun channel(context: Context) {
        val m = context.getSystemService(NotificationManager::class.java)
        m.createNotificationChannel(
            NotificationChannel(CHANNEL, tr("Reminders"), NotificationManager.IMPORTANCE_HIGH),
        )
        m.createNotificationChannel(
            NotificationChannel(URGENT, tr("Urgent reminders"), NotificationManager.IMPORTANCE_HIGH),
        )
        m.createNotificationChannel(
            NotificationChannel(ALARM, tr("Alarm reminders"), NotificationManager.IMPORTANCE_HIGH).apply {
                setSound(
                    RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM),
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_ALARM)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build(),
                )
                enableVibration(true)
                vibrationPattern = longArrayOf(0, 600, 400, 600, 400)
                lockscreenVisibility = android.app.Notification.VISIBILITY_PUBLIC
            },
        )
    }

    /** Whether alarms may take over the screen (Android 14 and later ask the user). */
    fun canTakeScreen(context: Context): Boolean = Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE ||
        context.getSystemService(NotificationManager::class.java).canUseFullScreenIntent()

    /** A reminder of something of `priority`, on its channel: `ring` (an alarm), else urgent or not. */
    private fun builder(app: TodavApp, priority: Long?, ring: Boolean) =
        NotificationCompat.Builder(app, if (ring) ALARM else if (app.core.urgentReminder(priority)) URGENT else CHANNEL)

    /** A task reminder's task UID and the occurrence it is for (`reminderOccurrence`), in its extras. */
    private const val EXTRA_TASK = "dev.aperte.todav.task"
    private const val EXTRA_AT = "dev.aperte.todav.at"

    /** The occurrence intent `i` of a task reminder names, if it does (older ones do not). */
    fun at(i: Intent): Long? = if (i.hasExtra("at")) i.getLongExtra("at", 0) else null

    /** Take back the reminders of tasks done, moved on or deleted since, here or elsewhere. */
    private fun tidy(app: TodavApp) {
        val m = app.getSystemService(NotificationManager::class.java)
        for (n in m.activeNotifications) {
            val extras = n.notification.extras
            val uid = extras.getString(EXTRA_TASK) ?: continue
            val at = if (extras.containsKey(EXTRA_AT)) extras.getLong(EXTRA_AT) else null
            if (app.core.reminderStale(uid, at)) {
                m.cancel(n.tag, n.id)
                hush(app, n.id)
            }
        }
    }

    /** The next reminder's alarm, after the reminders shown that no longer apply are gone. */
    fun schedule(app: TodavApp) {
        tidy(app)
        val alarms = app.getSystemService(AlarmManager::class.java)
        val intent = PendingIntent.getBroadcast(
            app, 0, Intent(app, ReminderReceiver::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val next = app.core.nextReminder()
        if (next == null) {
            alarms.cancel(intent)
            return
        }
        // The core's times are local wall-clock; the alarm's are UTC.
        val offset = app.core.localNow() - System.currentTimeMillis() / 1000
        wake(app, (next - offset) * 1000, intent)
    }

    /** `intent` at `at` (UTC milliseconds), on the minute where Android lets the app, waking the device. */
    private fun wake(context: Context, at: Long, intent: PendingIntent) {
        val alarms = context.getSystemService(AlarmManager::class.java)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarms.canScheduleExactAlarms()) {
            alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
        } else {
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
        }
    }

    /** Show what went off since the last time. */
    fun show(app: TodavApp) {
        if (ContextCompat.checkSelfPermission(app, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED &&
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
        ) {
            app.core.dueReminders() // past, either way
            return
        }
        val due = app.core.dueReminders()
        val now = app.core.localNow()
        val m = NotificationManagerCompat.from(app)
        for (t in due.tasks) post(app, t, now)
        for (e in due.events) post(app, e, now)
    }

    /**
     * A reminder of task `t` shown: an alarm when it is one and not `quiet`, which also rings
     * for `Client::alarmSeconds` at most; then it goes on as a silent urgent one (`stop`).
     */
    private fun post(app: TodavApp, t: TodavTask, now: Long, quiet: Boolean = false) {
        val ring = !quiet && app.core.alarmReminder(t.priority)
        val id = t.uid.hashCode()
        NotificationManagerCompat.from(app).notify(id, task(app, t, now, ring, quiet))
        if (ring) startAlarm(app, id) { putExtra("task", t.uid); reminderOccurrence(t)?.let { putExtra("at", it) } }
    }

    /** The same for occurrence `e` of an event. */
    private fun post(app: TodavApp, e: Event, now: Long, quiet: Boolean = false) {
        val ring = !quiet && app.core.alarmReminder(e.priority)
        val id = (e.uid + e.start).hashCode()
        NotificationManagerCompat.from(app).notify(id, event(app, e, now, ring, quiet))
        if (ring) startAlarm(app, id) { putExtra("event", e.uid).putExtra("start", e.start) }
    }

    /** What `Stop` of notification `id` is: also the alarm that stops the sound after its ten minutes. */
    private fun stopIntent(context: Context, id: Int, flags: Int, f: Intent.() -> Unit = {}): PendingIntent? =
        PendingIntent.getBroadcast(
            context, id, Intent(context, ReminderReceiver::class.java).setAction(STOP).putExtra("id", id).apply(f),
            PendingIntent.FLAG_IMMUTABLE or flags,
        )

    private fun startAlarm(app: TodavApp, id: Int, f: Intent.() -> Unit) {
        val end = System.currentTimeMillis() + app.core.alarmSeconds() * 1000
        wake(app, end, stopIntent(app, id, PendingIntent.FLAG_UPDATE_CURRENT, f)!!)
    }

    /** Notification `id`'s alarm is over (acted on, stopped or gone): no end to wait for, its screen closed. */
    fun hush(context: Context, id: Int) {
        stopIntent(context, id, PendingIntent.FLAG_NO_CREATE)?.let {
            context.getSystemService(AlarmManager::class.java).cancel(it)
            it.cancel()
        }
        context.sendBroadcast(Intent(OVER).setPackage(context.packageName).putExtra("id", id))
    }

    /**
     * Stop an alarm, by the button or at its end: the sound gone, the reminder left as a silent
     * urgent one (its task or event as it is, unless done, moved on or gone meanwhile).
     */
    fun stop(app: TodavApp, intent: Intent) {
        val id = intent.getIntExtra("id", 0)
        hush(app, id)
        // Taken back first: a notification replaced in place may go on ringing.
        NotificationManagerCompat.from(app).cancel(id)
        again(app, intent, quiet = true)
    }

    /**
     * Snooze: notification `intent`'s reminder gone, and back in ten minutes, the item as it is
     * then (its task or event left as they are).
     */
    fun snooze(context: Context, intent: Intent) {
        val id = intent.getIntExtra("id", 0)
        NotificationManagerCompat.from(context).cancel(id)
        hush(context, id)
        val again = PendingIntent.getBroadcast(
            context, id, Intent(intent).setAction(SNOOZED),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        wake(context, System.currentTimeMillis() + SNOOZE_SECONDS * 1000, again)
    }

    /** A snoozed reminder again, unless its task is done or moved on or its event gone meanwhile. */
    fun again(app: TodavApp, intent: Intent, quiet: Boolean = false) {
        val now = app.core.localNow()
        val m = NotificationManagerCompat.from(app)
        val id = intent.getIntExtra("id", 0)
        var shown = false
        intent.getStringExtra("task")?.takeIf { !app.core.reminderStale(it, at(intent)) }?.let { uid ->
            app.core.task(uid)?.let { post(app, it, now, quiet); shown = true }
        }
        intent.getStringExtra("event")?.let { uid ->
            val start = intent.getLongExtra("start", 0)
            app.core.events(start - 86_400, start + 86_400)
                .firstOrNull { it.uid == uid && it.start == start }
                ?.let { post(app, it, now, quiet); shown = true }
        }
        // Gone meanwhile: nothing left to ring or to keep.
        if (!shown) m.cancel(id)
    }

    private fun snoozeAction(app: TodavApp, id: Int, f: Intent.() -> Unit) = NotificationCompat.Action(
        0,
        tr("Snooze"),
        PendingIntent.getBroadcast(
            app, id, Intent(app, ReminderReceiver::class.java).setAction(SNOOZE).putExtra("id", id).apply(f),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        ),
    )

    private fun open(app: TodavApp, request: Int, f: Intent.() -> Unit): PendingIntent = PendingIntent.getActivity(
        app, request, Intent(app, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP).apply(f),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    /**
     * What an alarm adds to its reminder's notification: it stays until acted on, repeats its
     * sound (`FLAG_INSISTENT`), takes the screen where Android allows (`AlarmActivity`, with
     * the same choices) and has Stop. `quiet`: a silent one, as after a Stop.
     */
    private fun NotificationCompat.Builder.alarm(
        app: TodavApp,
        id: Int,
        ring: Boolean,
        quiet: Boolean,
        title: String,
        text: String,
        task: Boolean,
        open: PendingIntent,
        f: Intent.() -> Unit,
    ): android.app.Notification {
        if (quiet) setSilent(true)
        if (!ring) return build()
        setCategory(NotificationCompat.CATEGORY_ALARM)
        setOngoing(true)
        setAutoCancel(false)
        setPriority(NotificationCompat.PRIORITY_MAX)
        setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
        val screen = PendingIntent.getActivity(
            app, id,
            Intent(app, AlarmActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_USER_ACTION)
                .putExtra("id", id).putExtra("title", title).putExtra("text", text)
                .putExtra("is_task", task).putExtra("open", open).apply(f),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        setFullScreenIntent(screen, true)
        addAction(0, tr("Stop"), stopIntent(app, id, PendingIntent.FLAG_UPDATE_CURRENT, f))
        return build().apply { flags = flags or android.app.Notification.FLAG_INSISTENT }
    }

    private fun task(app: TodavApp, t: TodavTask, now: Long, ring: Boolean, quiet: Boolean): android.app.Notification {
        // The occurrence it is for, so that it goes, and "Mark as Done" does nothing, once the
        // task has moved on to another.
        val at = reminderOccurrence(t)
        val occurrence: Intent.() -> Unit = {
            putExtra("task", t.uid)
            at?.let { putExtra("at", it) }
        }
        val text = t.start?.takeIf { remindsOfStart(t, now) }
            ?.let { tr("Starts {date}").replace("{date}", dayLabel(it, t.startAllDay, now)) }
            ?: dueLabel(t, now)
        val open = open(app, t.uid.hashCode()) { putExtra("task", t.uid) }
        return builder(app, t.priority, ring)
            .addExtras(android.os.Bundle().apply { putString(EXTRA_TASK, t.uid); at?.let { putLong(EXTRA_AT, it) } })
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(t.summary)
            .setContentText(text)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .setAutoCancel(true)
            .setContentIntent(open)
            .addAction(
                0,
                tr("Mark as Done"),
                PendingIntent.getBroadcast(
                    app, t.uid.hashCode(), Intent(app, ReminderReceiver::class.java).setAction(DONE).apply(occurrence),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
            .addAction(snoozeAction(app, t.uid.hashCode(), occurrence))
            .alarm(app, t.uid.hashCode(), ring, quiet, t.summary, text.orEmpty(), true, open, occurrence)
    }

    private fun event(app: TodavApp, e: Event, now: Long, ring: Boolean, quiet: Boolean): android.app.Notification {
        val id = (e.uid + e.start).hashCode()
        val text = listOfNotNull(dayLabel(e.start, e.allDay, now), e.location?.takeIf { it.isNotBlank() }).joinToString(" · ")
        val open = open(app, id) { putExtra("day", e.start) }
        return builder(app, e.priority, ring)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(e.summary)
            .setContentText(text)
            .setCategory(NotificationCompat.CATEGORY_EVENT)
            .setAutoCancel(true)
            .setContentIntent(open)
            .addAction(snoozeAction(app, id) { putExtra("event", e.uid).putExtra("start", e.start) })
            .apply {
                // Its video call (`todav_app::meeting`), in the browser or the call's app.
                meetingLink(e)?.let { url ->
                    addAction(
                        0,
                        tr("Join Call"),
                        PendingIntent.getActivity(
                            app, id, Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url)),
                            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                        ),
                    )
                }
            }
            .alarm(app, id, ring, quiet, e.summary, text, false, open) { putExtra("event", e.uid).putExtra("start", e.start) }
    }
}

/** The alarm going off, and "Mark as Done" and "Snooze" on a reminder. */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Reminders.SNOOZE -> return Reminders.snooze(context, intent)
            Reminders.SNOOZED -> return background(context) { Reminders.again(it, intent) }
            Reminders.STOP -> return background(context) { Reminders.stop(it, intent) }
        }
        if (intent.action == Reminders.DONE) {
            val uid = intent.getStringExtra("task") ?: return
            NotificationManagerCompat.from(context).cancel(uid.hashCode())
            Reminders.hush(context, uid.hashCode())
            // As the Linux app's: it cannot ask about subtasks, so they stay open unless set to
            // complete. An old one does nothing (a repeating task already moved on).
            background(context) {
                it.core.completeFromReminder(uid, Reminders.at(intent))
                it.core.requestSync()
            }
            return
        }
        background(context) {
            Reminders.show(it)
            Reminders.schedule(it)
        }
    }
}

/** At boot and when the clock or the time zone changes: the alarm again, in the new zone, and the widgets. */
class ClockReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        background(context) {
            it.core.setLocalZone(java.time.ZoneId.systemDefault().id)
            Reminders.schedule(it)
            WidgetClock.tick(it)
        }
    }
}

/**
 * The widgets drawn again as time goes on: when the day turns (today, the coming days), and every
 * quarter hour while a Day widget shows its line at the current time.
 */
object WidgetClock {
    private const val QUARTER = 15 * 60 * 1000L

    fun schedule(context: Context) {
        val alarms = context.getSystemService(AlarmManager::class.java)
        val intent = PendingIntent.getBroadcast(
            context, 0, Intent(context, WidgetClockReceiver::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val placed = WidgetKind.placed(context)
        if (placed.isEmpty()) return alarms.cancel(intent)
        val zone = java.time.ZoneId.systemDefault()
        val now = System.currentTimeMillis()
        var at = java.time.LocalDate.now(zone).plusDays(1).atStartOfDay(zone).toInstant().toEpochMilli()
        if (placed.any { it.first == WidgetKind.DAY }) at = minOf(at, now - now % QUARTER + QUARTER)
        // Not waking the device: a screen off shows no widget. Gone off meanwhile, it rings once
        // the device wakes, before the home screen is seen. On time where it may (as reminders).
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarms.canScheduleExactAlarms()) {
            alarms.setExact(AlarmManager.RTC, at + 1_000, intent)
        } else {
            alarms.setWindow(AlarmManager.RTC, at + 1_000, 60_000, intent)
        }
    }

    /** Everything read again (today has moved on), the widgets drawn, the next time set. */
    suspend fun tick(app: TodavApp) {
        app.changed()
        WidgetKind.refreshAll(app)
        schedule(app)
    }
}

class WidgetClockReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = background(context) { WidgetClock.tick(it) }
}
