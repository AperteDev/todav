package dev.aperte.todav

import android.Manifest
import android.app.AlarmManager
import android.app.BroadcastOptions
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
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
 * The safety net for lost pushes: a sync every 30 minutes on a network; it also renews the push
 * registration (the core skips calendars whose registration has time left) and the reminder alarm.
 */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as TodavApp
        app.awaitReady()
        return try {
            app.core.sync()
            app.push.endpoint?.let { app.core.pushRegister(it, PUSH_EXPIRY) } ?: app.push.register()
            Reminders.schedule(app)
            Result.success()
        } catch (e: Exception) {
            Log.w(TAG, "sync", e)
            Result.retry()
        }
    }

    companion object {
        fun start(context: Context) {
            val request = PeriodicWorkRequestBuilder<SyncWorker>(30, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork("sync", ExistingPeriodicWorkPolicy.KEEP, request)
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

    // ponytail: a fixed delay, and an alarm that a reboot forgets; a choice of delays, kept
    // snoozes, if they turn out to be wanted.
    private const val SNOOZE_SECONDS = 600L

    fun channel(context: Context) {
        val m = context.getSystemService(NotificationManager::class.java)
        m.createNotificationChannel(
            NotificationChannel(CHANNEL, tr("Reminders"), NotificationManager.IMPORTANCE_HIGH),
        )
    }

    fun schedule(app: TodavApp) {
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
        val at = (next - offset) * 1000
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
        for (t in due.tasks) m.notify(t.uid.hashCode(), task(app, t, now))
        for (e in due.events) m.notify((e.uid + e.start).hashCode(), event(app, e, now))
    }

    /**
     * Snooze: notification `intent`'s reminder gone, and back in ten minutes, the item as it is
     * then (its task or event left as they are).
     */
    fun snooze(context: Context, intent: Intent) {
        val id = intent.getIntExtra("id", 0)
        NotificationManagerCompat.from(context).cancel(id)
        val again = PendingIntent.getBroadcast(
            context, id, Intent(intent).setAction(SNOOZED),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val alarms = context.getSystemService(AlarmManager::class.java)
        val at = System.currentTimeMillis() + SNOOZE_SECONDS * 1000
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarms.canScheduleExactAlarms()) {
            alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, again)
        } else {
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, again)
        }
    }

    /** A snoozed reminder again, unless its task is done or its event gone meanwhile. */
    fun again(app: TodavApp, intent: Intent) {
        val id = intent.getIntExtra("id", 0)
        val now = app.core.localNow()
        val m = NotificationManagerCompat.from(app)
        intent.getStringExtra("task")?.let { uid ->
            app.core.task(uid)?.takeIf { !it.done }?.let { m.notify(id, task(app, it, now)) }
        }
        intent.getStringExtra("event")?.let { uid ->
            val start = intent.getLongExtra("start", 0)
            app.core.events(start - 86_400, start + 86_400)
                .firstOrNull { it.uid == uid && it.start == start }
                ?.let { m.notify(id, event(app, it, now)) }
        }
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

    private fun task(app: TodavApp, t: TodavTask, now: Long) = NotificationCompat.Builder(app, CHANNEL)
        .setSmallIcon(R.drawable.ic_notification)
        .setContentTitle(t.summary)
        .setContentText(dueLabel(t, now))
        .setCategory(NotificationCompat.CATEGORY_REMINDER)
        .setAutoCancel(true)
        .setContentIntent(open(app, t.uid.hashCode()) { putExtra("task", t.uid) })
        .addAction(
            0,
            tr("Mark as Done"),
            PendingIntent.getBroadcast(
                app, t.uid.hashCode(), Intent(app, ReminderReceiver::class.java).setAction(DONE).putExtra("task", t.uid),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            ),
        )
        .addAction(snoozeAction(app, t.uid.hashCode()) { putExtra("task", t.uid) })
        .build()

    private fun event(app: TodavApp, e: Event, now: Long) = NotificationCompat.Builder(app, CHANNEL)
        .setSmallIcon(R.drawable.ic_notification)
        .setContentTitle(e.summary)
        .setContentText(listOfNotNull(dayLabel(e.start, e.allDay, now), e.location?.takeIf { it.isNotBlank() }).joinToString(" · "))
        .setCategory(NotificationCompat.CATEGORY_EVENT)
        .setAutoCancel(true)
        .setContentIntent(open(app, (e.uid + e.start).hashCode()) { putExtra("day", e.start) })
        .addAction(snoozeAction(app, (e.uid + e.start).hashCode()) { putExtra("event", e.uid).putExtra("start", e.start) })
        .build()
}

/** The alarm going off, and "Mark as Done" and "Snooze" on a reminder. */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Reminders.SNOOZE -> return Reminders.snooze(context, intent)
            Reminders.SNOOZED -> return background(context) { Reminders.again(it, intent) }
        }
        if (intent.action == Reminders.DONE) {
            val uid = intent.getStringExtra("task") ?: return
            NotificationManagerCompat.from(context).cancel(uid.hashCode())
            // As the Linux app's: it cannot ask about subtasks, so they stay open.
            background(context) {
                it.core.setDoneTree(uid, true, false, false)
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

/** At boot and when the clock or the time zone changes: the alarm again, in the new zone. */
class ClockReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        background(context) {
            it.core.setLocalZone(java.time.ZoneId.systemDefault().id)
            Reminders.schedule(it)
        }
    }
}
