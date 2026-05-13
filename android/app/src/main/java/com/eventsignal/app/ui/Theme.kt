package com.eventsignal.app.ui

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val BinanceYellow = Color(0xFFFCD535)
val BinanceYellowActive = Color(0xFFF0B90B)
val CanvasDark = Color(0xFF0B0E11)
val SurfaceCardDark = Color(0xFF1E2329)
val SurfaceElevatedDark = Color(0xFF2B3139)
val HairlineDark = Color(0xFF2B3139)
val BodyOnDark = Color(0xFFEAECEF)
val MutedText = Color(0xFF929AA5)
val TradingUp = Color(0xFF0ECB81)
val TradingDown = Color(0xFFF6465D)

private val AppColors: ColorScheme = darkColorScheme(
    primary = BinanceYellow,
    onPrimary = Color(0xFF181A20),
    secondary = SurfaceElevatedDark,
    tertiary = BinanceYellowActive,
    background = CanvasDark,
    onBackground = BodyOnDark,
    surface = SurfaceCardDark,
    onSurface = BodyOnDark,
    surfaceVariant = SurfaceElevatedDark,
    onSurfaceVariant = MutedText,
    outline = HairlineDark,
    error = TradingDown,
    onError = Color.White,
)

@Composable
fun EventSignalTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = AppColors,
        content = content,
    )
}
