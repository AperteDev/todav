package dev.aperte.todav

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.core.content.edit
import dev.aperte.todav.core.Keyring
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * App passwords of the accounts signed in here, by server and user: encrypted with an AES key that
 * the Android Keystore keeps (and never lets out), the ciphertext in the app's preferences.
 */
class Passwords(context: Context) : Keyring {
    private val prefs = context.getSharedPreferences("passwords", Context.MODE_PRIVATE)

    private fun key(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(ALIAS, null) as SecretKey?)?.let { return it }
        val spec = KeyGenParameterSpec.Builder(
            ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        ).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
            .apply { init(spec) }
            .generateKey()
    }

    private fun name(url: String, user: String) = "$user@$url"

    override fun store(url: String, user: String, password: String): String? = try {
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key()) }
        val data = cipher.iv + cipher.doFinal(password.toByteArray())
        prefs.edit { putString(name(url, user), Base64.encodeToString(data, Base64.NO_WRAP)) }
        null
    } catch (e: Exception) {
        e.message ?: e.toString()
    }

    override fun clear(url: String, user: String) {
        prefs.edit { remove(name(url, user)) }
    }

    fun load(url: String, user: String): String? = try {
        prefs.getString(name(url, user), null)?.let {
            val data = Base64.decode(it, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, data, 0, IV))
            String(cipher.doFinal(data, IV, data.size - IV))
        }
    } catch (_: Exception) {
        null
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "todav-passwords"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV = 12
    }
}
