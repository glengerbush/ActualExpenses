package org.actualexpenses.core.api

import com.google.gson.JsonParser
import java.time.LocalDate
import java.util.UUID
import org.actualexpenses.core.protocol.LogicalClock

/** Foreground system work is independent of expense projection and delivery jobs. */
internal data class ScheduledCommand(
    val id: String,
    val server: String,
    val budget: String,
    val path: String,
    val body: String,
    val clock: String,
    val epoch: String,
    var attempted: Boolean = false,
    var reviewMessage: String? = null,
) {
    fun validate() {
        require(
            UUID.fromString(id).toString() == id && UUID.fromString(budget).toString() == budget
        )
        require(BudgetApi.normalized(server) == server)
        require(path == "/budgets/$budget/schedules/commands")
        require(epoch.matches(Regex("[0-9a-f]{32}")))
        LogicalClock.validate(clock)
        val payload = JsonParser.parseString(body).asJsonObject
        require(
            payload.keySet() == setOf("action", "through") &&
                payload["action"].asString == "catch-up"
        )
        val through = payload["through"].asString
        require(LocalDate.parse(through).toString() == through)
    }
}
