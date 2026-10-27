package dev.aperte.todav.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

/**
 * Light or dark as `scheme` says ("light", "dark", else the system's); the wallpaper's colours
 * where Android has them (12 and later), else the icon's blue and teal.
 */
@Composable
fun TodavTheme(scheme: String?, content: @Composable () -> Unit) {
    val dark = when (scheme) {
        "light" -> false
        "dark" -> true
        else -> isSystemInDarkTheme()
    }
    val context = LocalContext.current
    val colors = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        dark -> darkColorScheme(primary = Color(0xFF8EC4EE), secondary = Color(0xFFA5E1D0))
        else -> lightColorScheme(primary = Color(0xFF2F6CB6), secondary = Color(0xFF1B7275))
    }
    MaterialTheme(colorScheme = colors, content = content)
}

/** "#rrggbb" (maybe with alpha) as a colour; grey when it is not one. */
fun parseColor(hex: String?): Color = try {
    Color(android.graphics.Color.parseColor(hex?.take(7)))
} catch (_: Exception) {
    Color.Gray
}
