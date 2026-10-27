package dev.aperte.todav

import android.accounts.Account
import android.accounts.AccountManager
import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.IBinder
import android.os.ParcelFileDescriptor
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import com.nextcloud.android.sso.QueryParam
import com.nextcloud.android.sso.aidl.IInputStreamService
import com.nextcloud.android.sso.aidl.NextcloudRequest
import dev.aperte.todav.core.HttpHeader
import dev.aperte.todav.core.HttpReply
import dev.aperte.todav.ui.Choice
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.ObjectInputStream
import java.io.ObjectOutputStream
import java.net.URI
import java.net.URLDecoder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import org.json.JSONArray

/** `Account::source` of an account whose requests go through the Nextcloud app: then its account name. */
const val NEXTCLOUD_APP = "nextcloud-app:"

/** What came back for a request: its status, headers (lower-case names) and body. */
class Reply(val status: Int, val headers: Map<String, String>, val body: String)

/**
 * Accounts of the Nextcloud app (its Single Sign-On): the user picks one of the app's accounts
 * and grants Todav access; every request then goes through the app, which holds the
 * credentials. It proxies GET, POST, PUT, DELETE, PATCH, HEAD, MKCOL (without a body) and
 * PROPFIND (always depth 1), not REPORT, MKCALENDAR or PROPPATCH: the core syncs these accounts
 * without REPORT and cannot create, rename or recolour their lists. It also gives every
 * request a query string, which Nextcloud refuses push registrations with: no push for them.
 */
object NextcloudApp {
    /** The Nextcloud apps (release first) and their account types. */
    private val APPS = listOf("com.nextcloud.client" to "nextcloud", "com.nextcloud.android.beta" to "nextcloud.beta")

    fun installed(context: Context): List<Pair<String, String>> = APPS.filter { (p, _) ->
        runCatching { context.packageManager.getPackageInfo(p, 0) }.isSuccess
    }

    fun packageOf(type: String) = APPS.firstOrNull { it.second == type }?.first ?: APPS.first().first

    /** The account picker of the installed Nextcloud apps' accounts. */
    fun chooser(context: Context): Intent = AccountManager.newChooseAccountIntent(
        null, null, installed(context).map { it.second }.toTypedArray(), null, null, null, null,
    )

    /** The Nextcloud app asking whether Todav may use `account`. */
    fun grant(account: Account): Intent = Intent()
        .setComponent(ComponentName(packageOf(account.type), "com.owncloud.android.ui.activity.SsoGrantPermissionActivity"))
        .putExtra("NextcloudFilesAccount", account)

    /** The granted account from the grant's result: account name, user, token, server, type. */
    data class Granted(val name: String, val user: String, val token: String, val server: String, val type: String)

    fun granted(data: Intent?): Granted? {
        val b = data?.getBundleExtra("NextcloudSSO") ?: return null
        return Granted(
            b.getString(AccountManager.KEY_ACCOUNT_NAME) ?: return null,
            b.getString("user_id") ?: b.getString("username") ?: return null,
            b.getString("token") ?: return null,
            b.getString("server_url") ?: return null,
            b.getString("accountType") ?: "nextcloud",
        )
    }

    private fun prefs(context: Context) = context.getSharedPreferences("nextcloud_app", Context.MODE_PRIVATE)

    /** Remember an account's type (which app has it); its token goes to the keyring. */
    fun remember(context: Context, g: Granted) = prefs(context).edit().putString(g.name, g.type).apply()

    fun client(app: TodavApp, a: dev.aperte.todav.core.Account): Client? {
        val name = a.source.removePrefix(NEXTCLOUD_APP)
        val token = app.passwords.load(a.url, a.user) ?: return null
        return Client(app, name, prefs(app).getString(name, "nextcloud")!!, token, a.url)
    }

    /** Connect account `a` through the Nextcloud app (at start, or after it was added). */
    fun connect(app: TodavApp, a: dev.aperte.todav.core.Account) {
        val client = client(app, a) ?: return
        app.core.connectTransport(a.id, client)
    }

    /**
     * Requests through one of the Nextcloud app's accounts. Blocking, for the core's threads; the
     * service is bound on first use and again if the app goes away.
     */
    class Client(
        private val context: Context,
        private val account: String,
        type: String,
        private val token: String,
        private val server: String,
    ) : dev.aperte.todav.core.Transport {
        private val pkg = packageOf(type)
        @Volatile private var service: IInputStreamService? = null
        @Volatile private var bound: CountDownLatch? = null

        private val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName, binder: IBinder) {
                service = IInputStreamService.Stub.asInterface(binder)
                bound?.countDown()
            }

            override fun onServiceDisconnected(name: ComponentName) {
                service = null
            }
        }

        private fun service(): IInputStreamService {
            service?.let { return it }
            synchronized(this) {
                service?.let { return it }
                val latch = CountDownLatch(1)
                bound = latch
                val intent = Intent().setComponent(ComponentName(pkg, "com.owncloud.android.services.AccountManagerService"))
                if (!context.bindService(intent, connection, Context.BIND_AUTO_CREATE)) {
                    throw IllegalStateException(tr("The Nextcloud app is not installed"))
                }
                if (!latch.await(10, TimeUnit.SECONDS)) throw IllegalStateException(tr("The Nextcloud app does not answer"))
                return service!!
            }
        }

        override fun send(method: String, url: String, headers: List<HttpHeader>, body: String): HttpReply {
            val r = request(method, url, headers.map { it.name to it.value }, body)
            return HttpReply(r.status.toUShort(), r.headers.map { HttpHeader(it.key, it.value) }, r.body)
        }

        fun request(method: String, url: String, headers: List<Pair<String, String>>, body: String): Reply {
            // The app puts its server before the path; the query goes as parameters.
            val base = server.trimEnd('/')
            require(url.startsWith(base)) { "$url is not on $base" }
            val uri = URI(url.substring(base.length).ifEmpty { "/" })
            val params = uri.rawQuery?.split('&')?.filter { it.isNotEmpty() }?.map {
                val (k, v) = (it.split('=', limit = 2) + "").take(2)
                QueryParam(URLDecoder.decode(k, "UTF-8"), URLDecoder.decode(v, "UTF-8"))
            } ?: emptyList()
            // Its PROPFIND sets depth 1 itself; a second Depth header would contradict it.
            val h = headers.filterNot { method == "PROPFIND" && it.first.equals("depth", true) }
                .groupBy({ it.first }, { it.second })
            // A body as text goes out as JSON (PROPFIND's as XML); as a stream, with our Content-Type.
            val streamed = body.isNotEmpty() && method != "PROPFIND"
            val request = NextcloudRequest(
                method, uri.rawPath, HashMap(h), body.takeIf { it.isNotEmpty() && !streamed }, params, account, token,
                context.packageName,
            )
            val bytes = ByteArrayOutputStream().also { ObjectOutputStream(it).use { o -> o.writeObject(request) } }.toByteArray()
            val input = pipe(bytes.inputStream())
            val output = if (streamed) {
                service().performNextcloudRequestAndBodyStreamV2(input, pipe(body.byteInputStream()))
            } else {
                service().performNextcloudRequestV2(input)
            }
            ParcelFileDescriptor.AutoCloseInputStream(output).use { stream ->
                val o = ObjectInputStream(stream)
                val error = try {
                    o.readObject() as Exception?
                } catch (e: ClassNotFoundException) {
                    IllegalStateException(e.message)
                }
                if (error != null) return failure(error)
                val replyHeaders = JSONArray(o.readObject() as String).let { a ->
                    (0 until a.length()).associate { a.getJSONObject(it).getString("name").lowercase() to a.getJSONObject(it).getString("value") }
                }
                val text = stream.readBytes().decodeToString()
                // It answers 2xx without the code; the core checks the one each method gives.
                val status = when {
                    method == "PROPFIND" -> 207
                    method == "MKCOL" || (method == "POST" && "location" in replyHeaders) -> 201
                    method == "DELETE" -> 204
                    else -> 200
                }
                return Reply(status, replyHeaders, text)
            }
        }

        /**
         * An error as the app reports it: a failed HTTP request ("CE_5") carries its status and
         * body in its causes; anything else (no network, access withdrawn) is the core's network error.
         */
        private fun failure(e: Exception): Reply {
            if (e.message == "CE_5") {
                val status = e.cause?.message?.toIntOrNull()
                if (status != null) return Reply(status, emptyMap(), e.cause?.cause?.message ?: "")
            }
            val reason = when (e.message) {
                "CE_1" -> tr("The Nextcloud app no longer lets Todav use this account; sign in again")
                "CE_2" -> tr("The account is no longer in the Nextcloud app")
                "CE_6" -> tr("The Nextcloud app no longer lets Todav use this account; sign in again")
                else -> e.message ?: e.toString()
            }
            throw IllegalStateException(reason)
        }

        /** A pipe the service reads `data` from. */
        private fun pipe(data: InputStream): ParcelFileDescriptor {
            val (read, write) = ParcelFileDescriptor.createPipe()
            thread(name = "sso-request") {
                ParcelFileDescriptor.AutoCloseOutputStream(write).use { out -> data.copyTo(out) }
            }
            return read
        }
    }
}

/**
 * The welcome page's (and Add Account's) first choice when a Nextcloud app is installed: one of
 * its accounts, Todav asking it for access. The GOA counterpart.
 */
@Composable
fun NextcloudAppChoice(model: Main) {
    val context = LocalContext.current
    if (remember { NextcloudApp.installed(context) }.isEmpty()) return
    var failed by remember { mutableStateOf<String?>(null) }
    val grant = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val g = NextcloudApp.granted(r.data)
        if (r.resultCode == Activity.RESULT_OK && g != null) {
            NextcloudApp.remember(context, g)
            model.addNextcloudApp(g)
        } else if (r.resultCode != Activity.RESULT_CANCELED) {
            failed = r.data?.getStringExtra("NextcloudSsoException") ?: tr("The Nextcloud app did not grant access")
        }
    }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val name = r.data?.getStringExtra(AccountManager.KEY_ACCOUNT_NAME)
        val type = r.data?.getStringExtra(AccountManager.KEY_ACCOUNT_TYPE)
        if (r.resultCode == Activity.RESULT_OK && name != null && type != null) grant.launch(NextcloudApp.grant(Account(name, type)))
    }
    Choice(tr("Nextcloud App"), tr("Use an account of the Nextcloud app on this device; it signs in for Todav")) {
        failed = null
        pick.launch(NextcloudApp.chooser(context))
    }
    failed?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

/** Asking the Nextcloud app again for an account it no longer lets Todav use. */
@Composable
fun NextcloudAppReconnect(model: Main, account: dev.aperte.todav.core.Account) {
    val context = LocalContext.current
    val grant = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        NextcloudApp.granted(r.data)?.takeIf { r.resultCode == Activity.RESULT_OK }?.let { g ->
            NextcloudApp.remember(context, g)
            model.reconnectNextcloudApp(account, g)
        }
    }
    val type = remember { context.getSharedPreferences("nextcloud_app", Context.MODE_PRIVATE).getString(account.source.removePrefix(NEXTCLOUD_APP), "nextcloud")!! }
    TextButton(onClick = { grant.launch(NextcloudApp.grant(Account(account.source.removePrefix(NEXTCLOUD_APP), type))) }) {
        Text(tr("Sign In Again"))
    }
}
