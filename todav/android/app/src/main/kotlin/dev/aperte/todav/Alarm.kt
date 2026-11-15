package dev.aperte.todav

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import dev.aperte.todav.ui.TodavTheme

/**
 * An alarm reminder over the lock screen and whatever is in front (the notification's full-screen
 * intent): the task or event, with the notification's own choices, which stop the sound and close
 * it. It closes by itself when the alarm is over (`Reminders.OVER`): stopped, snoozed, done,
 * gone, or after its ten minutes.
 */
class AlarmActivity : ComponentActivity() {
    private val over = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.getIntExtra("id", 0) == this@AlarmActivity.intent.getIntExtra("id", 1)) finish()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        I18n.load(this)
        enableEdgeToEdge()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            setShowWhenLocked(true)
            setTurnScreenOn(true)
        }
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        ContextCompat.registerReceiver(this, over, IntentFilter(Reminders.OVER), ContextCompat.RECEIVER_NOT_EXPORTED)
        val app = application as TodavApp
        val title = intent.getStringExtra("title").orEmpty()
        val text = intent.getStringExtra("text").orEmpty()
        val isTask = intent.getBooleanExtra("is_task", false)
        // The notification's own buttons: the same intents, to the receiver.
        val act = { action: String ->
            val i = Intent(this, ReminderReceiver::class.java).setAction(action).putExtras(intent)
            i.removeExtra("open")
            sendBroadcast(i)
            finish()
        }
        setContent {
            val scheme = androidx.compose.runtime.remember { app.core.setting("color_scheme") }
            TodavTheme(scheme) {
                Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
                    Column(
                        Modifier.fillMaxSize().safeDrawingPadding().padding(24.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        // Opens the item in the app (the notification's own tap).
                        Column(
                            Modifier.fillMaxWidth().clickable { openItem() }.padding(bottom = 24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Text(title, style = MaterialTheme.typography.headlineLarge, textAlign = TextAlign.Center)
                            if (text.isNotEmpty()) {
                                Text(text, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
                            }
                        }
                        if (isTask) {
                            Button({ act(Reminders.DONE) }, Modifier.fillMaxWidth()) { Text(tr("Mark as Done")) }
                        }
                        FilledTonalButton({ act(Reminders.SNOOZE) }, Modifier.fillMaxWidth()) { Text(tr("Snooze")) }
                        OutlinedButton({ act(Reminders.STOP) }, Modifier.fillMaxWidth()) { Text(tr("Stop")) }
                    }
                }
            }
        }
    }

    private fun openItem() {
        intent.getParcelableExtra<PendingIntent>("open")?.send()
        finish()
    }

    override fun onDestroy() {
        unregisterReceiver(over)
        super.onDestroy()
    }
}
