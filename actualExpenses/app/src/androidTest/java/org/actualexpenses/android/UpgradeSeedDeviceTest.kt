package org.actualexpenses.android

import com.google.gson.JsonParser
import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.security.MessageDigest
import org.actualexpenses.core.StateStore
import org.junit.Assert.*
import org.junit.Test

/** Run against the released APK before installing this update. Synthetic fixture only. */
class UpgradeSeedDeviceTest {
    @Test
    fun seedReleasedEncryptedState() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val selected = InstrumentationRegistry.getArguments().getString("upgradeCase")
        org.junit.Assume.assumeTrue("Explicit disposable upgradeCase required", selected != null)
        val case = requireNotNull(selected)
        require(case in setOf("pending", "lost-create", "lost-edit", "lost-delete"))
        val raw =
            instrumentation.context.assets.open("$case.state.json").bufferedReader().use {
                it.readText()
            }
        // Released 2b25b07 has the one-argument constructor. Use that APK's actual
        // AES/GCM + AtomicFile implementation and its actual Keystore identity.
        val clazz = Class.forName("org.actualexpenses.android.EncryptedStateStore")
        val store = clazz.getConstructor(Context::class.java).newInstance(context) as StateStore
        store.write(raw)
        assertEquals(raw, store.read())
        // This seed check runs against the released APK's actual encrypted store.
        // Decoding/migrating the original payload is qualified by native upgrade tests.
        val pendingCount = JsonParser.parseString(raw).asJsonObject.getAsJsonArray("pending")
            .map { it.asJsonObject["transactionId"].asString }.distinct().size
        assertTrue(pendingCount > 0)
        val original = File(context.noBackupFilesDir, "expenses.state").readBytes()
        assertEquals(1, original.first().toInt())
        val hash =
            MessageDigest.getInstance("SHA-256").digest(original).joinToString("") {
                "%02x".format(it.toInt() and 255)
            }
        val review = File(context.getExternalFilesDir(null), "review").apply { mkdirs() }
        File(review, "android20-original.sha256").writeText(hash)
        File(review, "android20-case.txt").writeText(case)
        println(
            "RELEASED_STATE_SEEDED case=$case pending=${pendingCount} encrypted_sha256=$hash"
        )
    }
}
