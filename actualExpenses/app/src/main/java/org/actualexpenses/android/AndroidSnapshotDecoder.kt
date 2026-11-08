package org.actualexpenses.android

import android.content.Context
import android.database.Cursor
import android.database.sqlite.SQLiteDatabase
import com.google.gson.JsonNull
import com.google.gson.JsonObject
import org.actualexpenses.core.protocol.BudgetArchive
import org.actualexpenses.core.protocol.SnapshotCompatibility
import org.actualexpenses.core.protocol.SnapshotDecoder
import org.actualexpenses.core.protocol.SnapshotTables
import java.io.File

/** SQLite is opened read-only. App state never migrates or writes the Actual snapshot. */
class AndroidSnapshotDecoder(private val context: Context) : SnapshotDecoder {
    override fun decode(archive: BudgetArchive): SnapshotTables {
        val file = File.createTempFile("actual-bootstrap-", ".sqlite", context.cacheDir)
        try {
            file.writeBytes(archive.sqlite)
            return SQLiteDatabase.openDatabase(file.path, null, SQLiteDatabase.OPEN_READONLY).use { database ->
                val rows = linkedMapOf<String, List<JsonObject>>()
                val columns = linkedMapOf<String, Set<String>>()
                val available = database.rawQuery("SELECT name FROM sqlite_master WHERE type='table'", null).use { cursor ->
                    buildSet { while (cursor.moveToNext()) add(cursor.getString(0)) }
                }
                val tables = listOf("__migrations__", "accounts", "transactions", "payees", "payee_mapping", "categories", "category_mapping", "rules", "preferences", "messages_clock", "messages_crdt")
                for (table in tables.filter { it in available }) {
                    database.rawQuery("SELECT * FROM \"$table\"", null).use { cursor ->
                        columns[table] = cursor.columnNames.toSet()
                        rows[table] = buildList {
                            while (cursor.moveToNext()) add(JsonObject().apply {
                                cursor.columnNames.forEachIndexed { index, name ->
                                    when (cursor.getType(index)) {
                                        Cursor.FIELD_TYPE_NULL -> add(name, JsonNull.INSTANCE)
                                        Cursor.FIELD_TYPE_INTEGER -> addProperty(name, cursor.getLong(index))
                                        Cursor.FIELD_TYPE_FLOAT -> addProperty(name, cursor.getDouble(index))
                                        else -> addProperty(name, cursor.getString(index))
                                    }
                                }
                            })
                        }
                    }
                }
                SnapshotCompatibility.validate(rows, columns)
                val clock = SnapshotCompatibility.clock(rows)
                val changes = rows["messages_crdt"].orEmpty().map(SnapshotCompatibility::decodeChange)
                SnapshotTables(rows, clock, changes)
            }
        } finally {
            file.delete()
        }
    }
}
