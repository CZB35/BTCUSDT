package com.eventsignal.app.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import com.eventsignal.app.MainActivity
import com.eventsignal.app.R

private const val CHANNEL_ID = "event_signal_runtime"

fun runtimeNotification(context: Context, title: String, text: String): Notification {
    ensureNotificationChannel(context)
    val intent = PendingIntent.getActivity(
        context,
        1,
        Intent(context, MainActivity::class.java),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    return NotificationCompat.Builder(context, CHANNEL_ID)
        .setSmallIcon(android.R.drawable.stat_notify_sync)
        .setContentTitle(title)
        .setContentText(text)
        .setContentIntent(intent)
        .setOngoing(true)
        .build()
}

private fun ensureNotificationChannel(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val manager = context.getSystemService(NotificationManager::class.java)
    val channel = NotificationChannel(
        CHANNEL_ID,
        context.getString(R.string.app_name),
        NotificationManager.IMPORTANCE_LOW,
    )
    manager.createNotificationChannel(channel)
}
