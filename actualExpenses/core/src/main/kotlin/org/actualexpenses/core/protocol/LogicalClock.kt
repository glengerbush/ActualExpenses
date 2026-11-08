package org.actualexpenses.core.protocol

import java.time.Instant
import java.time.format.DateTimeFormatterBuilder
import java.util.UUID

/** Actual's hybrid logical ordering. Persist [state] with every locally saved mutation. */
class LogicalClock(
    private val node: String = UUID.randomUUID().toString().replace("-", "").takeLast(16),
    lastTimestamp: String = ZERO_TIMESTAMP,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private var millis = parse(lastTimestamp).first
    private var counter = parse(lastTimestamp).second
    init { require(node.matches(Regex("[a-zA-Z0-9]{16}"))) { "Clock node must contain 16 alphanumeric characters" } }

    val state: String get() = format(millis, counter, node)

    @Synchronized fun next(): String {
        val physical = now()
        val nextMillis = maxOf(millis, physical)
        val nextCounter = if (nextMillis == millis) counter + 1 else 0
        update(nextMillis, nextCounter, physical)
        return state
    }

    @Synchronized fun observe(timestamp: String) {
        val (remoteMillis, remoteCounter) = parse(timestamp)
        val physical = now()
        val nextMillis = maxOf(millis, physical, remoteMillis)
        val nextCounter = when {
            nextMillis == millis && nextMillis == remoteMillis -> maxOf(counter, remoteCounter) + 1
            nextMillis == millis -> counter + 1
            nextMillis == remoteMillis -> remoteCounter + 1
            else -> 0
        }
        update(nextMillis, nextCounter, physical)
    }

    private fun update(nextMillis: Long, nextCounter: Int, physical: Long) {
        if (nextMillis - physical > 300_000) throw ActualException(ActualException.Kind.CLOCK, "Device clocks differ by more than five minutes. Correct the device time and retry.")
        if (nextCounter > 65535) throw ActualException(ActualException.Kind.CLOCK, "Logical clock counter exceeded its limit. Retry after the clock advances.")
        millis = nextMillis
        counter = nextCounter
    }

    companion object {
        private val iso = DateTimeFormatterBuilder().appendInstant(3).toFormatter()
        private val pattern = Regex("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}\\.\\d{3}Z-[0-9A-F]{4}-[A-Za-z0-9]{16}$")
        fun validate(timestamp: String) { parse(timestamp) }
        private fun parse(timestamp: String): Pair<Long, Int> {
            if (!pattern.matches(timestamp)) throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned an invalid logical timestamp")
            return try { Instant.parse(timestamp.take(24)).toEpochMilli() to timestamp.substring(25, 29).toInt(16) }
            catch (e: RuntimeException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned an invalid logical timestamp", e) }
        }
        private fun format(millis: Long, counter: Int, node: String) = "${iso.format(Instant.ofEpochMilli(millis))}-${counter.toString(16).uppercase().padStart(4, '0')}-$node"
    }
}
