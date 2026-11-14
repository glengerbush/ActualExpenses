package org.actualexpenses.core

import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.*
import org.actualexpenses.core.api.LegacyUpgrade
import org.actualexpenses.core.protocol.SnapshotDecoder
import org.junit.jupiter.api.Test

class LegacyStateDecoderTest {
    @Test
    fun `migration projection matches released client for original durable fixtures`() {
        val root = Path.of(System.getenv("ACTUALBUDGET_FIXTURE_DIR") ?: "../../actualBudget/tests/fixtures").resolve("android-upgrade")
        for (case in listOf("pending", "lost-create", "lost-edit", "lost-delete")) {
            val raw = Files.readString(root.resolve("$case.state.json"))
            val store = object : StateStore {
                override fun read() = raw
                override fun write(value: String): Unit = error("Oracle must not write released state")
            }
            val original = ExpenseService(store, SnapshotDecoder { error("Oracle must not download") }).state()
            val migration = LegacyUpgrade(raw)
            val decoded = migration.view
            assertEquals(original.expenses, decoded.expenses, case)
            assertEquals(original.payees, decoded.payees, case)
            assertEquals(original.categories, decoded.categories, case)
            assertEquals(original.currency, decoded.currency, case)
            assertEquals(original.payeeCategoryAssociations, decoded.payeeCategoryAssociations, case)
            assertEquals(raw, migration.raw)
        }
    }
}
