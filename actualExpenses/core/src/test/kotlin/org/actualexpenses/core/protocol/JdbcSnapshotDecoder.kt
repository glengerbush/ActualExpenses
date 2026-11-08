package org.actualexpenses.core.protocol

import com.google.gson.JsonNull
import com.google.gson.JsonObject
import java.nio.file.Files
import java.sql.DriverManager

/** Test adapter only: production reads the same archive through Android's read-only SQLite API. */
class JdbcSnapshotDecoder : SnapshotDecoder {
    override fun decode(archive: BudgetArchive): SnapshotTables {
        val file = Files.createTempFile("actual-readonly-", ".sqlite")
        try {
            Files.write(file, archive.sqlite)
            DriverManager.getConnection("jdbc:sqlite:file:$file?mode=ro").use { db ->
                val rows = linkedMapOf<String, List<JsonObject>>()
                val columns = linkedMapOf<String, Set<String>>()
                SnapshotCompatibility.requiredColumns.keys.forEach { table ->
                    db.createStatement().use { statement ->
                        statement.executeQuery("SELECT * FROM \"$table\"").use { result ->
                            val metadata = result.metaData
                            columns[table] = (1..metadata.columnCount).map { metadata.getColumnName(it) }.toSet()
                            val records = mutableListOf<JsonObject>()
                            while (result.next()) {
                                val row = JsonObject()
                                (1..metadata.columnCount).forEach { index ->
                                    val name = metadata.getColumnName(index)
                                    when (val value = result.getObject(index)) {
                                        null -> row.add(name, JsonNull.INSTANCE)
                                        is Number -> row.addProperty(name, value)
                                        is ByteArray -> row.addProperty(name, value.toString(Charsets.UTF_8))
                                        else -> row.addProperty(name, value.toString())
                                    }
                                }
                                records += row
                            }
                            rows[table] = records
                        }
                    }
                }
                SnapshotCompatibility.validate(rows, columns)
                return SnapshotTables(rows, SnapshotCompatibility.clock(rows), rows["messages_crdt"].orEmpty().map(SnapshotCompatibility::decodeChange))
            }
        } finally { Files.deleteIfExists(file) }
    }
}
