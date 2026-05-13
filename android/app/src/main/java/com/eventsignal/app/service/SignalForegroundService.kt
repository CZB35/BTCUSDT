package com.eventsignal.app.service

import android.app.Service
import android.content.Intent
import android.os.IBinder

class SignalForegroundService : Service() {
    override fun onCreate() {
        super.onCreate()
        startForeground(
            1001,
            runtimeNotification(this, "Signal service", "Receiving BTCUSDT signals"),
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_STICKY

    override fun onBind(intent: Intent?): IBinder? = null
}
