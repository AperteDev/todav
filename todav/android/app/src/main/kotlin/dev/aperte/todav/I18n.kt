package dev.aperte.todav

import android.content.Context
import dev.aperte.todav.core.Translations
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap

/**
 * Translations from the `todav` gettext catalogs, as the Linux app has them (assets
 * locale/<language>/todav.mo, made from po/ at build time). Mark text with [tr], [ntr] for
 * anything counted and [ptr] for a short word that could translate two ways; xgettext finds these
 * calls. Placeholders are named (`{n}`) and filled with `replace`.
 */
object I18n {
    @Volatile
    private var catalog: Translations = Translations.english()
    private val cache = ConcurrentHashMap<String, String>()

    /**
     * The catalog of the context's first language that has one (the app's own language, when set).
     * English, the source, has none but counts as one, so a language after it doesn't win.
     */
    fun load(context: Context) {
        val locales = context.resources.configuration.locales
        val names = (0 until locales.size()).flatMap {
            val l = locales[it]
            listOf("${l.language}_${l.country}", l.language)
        }
        catalog = names.firstNotNullOfOrNull { name ->
            if (name == "en") return@firstNotNullOfOrNull Translations.english()
            try {
                context.assets.open("locale/$name/todav.mo").use { Translations(it.readBytes()) }
            } catch (_: IOException) {
                null
            }
        } ?: Translations.english()
        cache.clear()
    }

    internal fun gettext(msgid: String) = cache.getOrPut(msgid) { catalog.gettext(msgid) }

    internal fun ngettext(one: String, many: String, n: Long) =
        catalog.ngettext(one, many, n.coerceAtLeast(0).toULong())

    internal fun pgettext(context: String, msgid: String) =
        cache.getOrPut("$context\u0004$msgid") { catalog.pgettext(context, msgid) }
}

fun tr(msgid: String) = I18n.gettext(msgid)

fun ntr(one: String, many: String, n: Long) = I18n.ngettext(one, many, n)

fun ptr(context: String, msgid: String) = I18n.pgettext(context, msgid)
