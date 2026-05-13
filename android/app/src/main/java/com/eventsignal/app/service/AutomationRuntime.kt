package com.eventsignal.app.service

import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

object AutomationRuntime {
    private val active = AtomicBoolean(false)
    private val calibrationRequests = AtomicLong(0)
    private val executionRequests = AtomicLong(0)

    var running: Boolean
        get() = active.get()
        set(value) {
            active.set(value)
        }

    fun stop() {
        running = false
    }

    fun requestCalibration(): Long =
        calibrationRequests.incrementAndGet()

    fun calibrationRequestVersion(): Long =
        calibrationRequests.get()

    fun requestExecution(): Long =
        executionRequests.incrementAndGet()

    fun executionRequestVersion(): Long =
        executionRequests.get()
}
