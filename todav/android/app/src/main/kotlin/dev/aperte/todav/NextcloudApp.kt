package dev.aperte.todav

import android.accounts.AccountManager
import android.content.Context
import android.content.Intent

/**
 * Accounts of the Nextcloud app on this device, to fill in a sign-in: Android's account picker
 * gives the one chosen as "user@host/path", which is all another app may read of it. The
 * sign-in itself stays Todav's (an app password of its own).
 */
object NextcloudApp {
    /** The Nextcloud app's account types: release and beta. */
    private val APPS = listOf("com.nextcloud.client" to "nextcloud", "com.nextcloud.android.beta" to "nextcloud.beta")

    private fun types(context: Context) = APPS.filter { (p, _) ->
        runCatching { context.packageManager.getPackageInfo(p, 0) }.isSuccess
    }.map { it.second }

    fun installed(context: Context) = types(context).isNotEmpty()

    /** The picker of the Nextcloud app's accounts. */
    fun chooser(context: Context): Intent =
        AccountManager.newChooseAccountIntent(null, null, types(context).toTypedArray(), null, null, null, null)

    /**
     * The server and login name of the account picked: its name is "user@host/path" (a login
     * name may itself hold an "@", so the last one splits them); the server is taken as https.
     */
    fun server(data: Intent?): Pair<String, String>? {
        val name = data?.getStringExtra(AccountManager.KEY_ACCOUNT_NAME) ?: return null
        val at = name.lastIndexOf('@')
        if (at <= 0 || at == name.lastIndex) return null
        return "https://" + name.substring(at + 1) to name.substring(0, at)
    }
}
