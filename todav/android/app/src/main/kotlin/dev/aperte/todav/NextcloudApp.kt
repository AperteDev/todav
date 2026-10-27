package dev.aperte.todav

import androidx.compose.runtime.Composable
import dev.aperte.todav.core.Account
import dev.aperte.todav.core.Todav

/** Accounts whose requests go through the Nextcloud app (its Single Sign-On). */
object NextcloudApp {
    fun connect(app: TodavApp, a: Account) {}
}

/** `Account::source` of an account whose requests go through the Nextcloud app. */
const val NEXTCLOUD_APP = "nextcloud-app:"

/** Lists of every account can be created, renamed and recoloured for now. */
fun Todav.canManageLists(account: Long) = true

@Composable
fun NextcloudAppChoice(model: Main) {}

@Composable
fun NextcloudAppReconnect(model: Main, account: Account) {}
