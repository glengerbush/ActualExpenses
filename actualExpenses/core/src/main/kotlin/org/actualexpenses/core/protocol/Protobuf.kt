package org.actualexpenses.core.protocol

import com.google.gson.JsonElement
import com.google.gson.JsonNull
import com.google.gson.JsonParser
import com.google.gson.JsonPrimitive
import java.io.ByteArrayOutputStream
import java.math.BigDecimal

/** The small, versioned Actual sync wire contract; unknown protobuf fields are skipped. */
internal object Protobuf {
    fun request(budget: RemoteBudget, messages: List<ProtocolChange>, since: String): ByteArray = Writer().apply {
        messages.forEach { change ->
            LogicalClock.validate(change.timestamp)
            bytes(1, Writer().apply {
                string(1, change.timestamp)
                bytes(3, Writer().apply {
                    string(1, change.dataset); string(2, change.row); string(3, change.column)
                    string(4, encodeValue(change.value))
                }.result())
            }.result())
        }
        string(2, budget.fileId); string(3, budget.groupId); string(6, since)
    }.result()

    fun response(bytes: ByteArray): SyncBatch {
        val messages = mutableListOf<ProtocolChange>()
        var merkle: String? = null
        Reader(bytes).fields { field, wire ->
            when (field) {
                1 -> {
                    var timestamp = ""
                    var encrypted = false
                    var content = byteArrayOf()
                    Reader(data(wire)).fields { nested, type -> when (nested) {
                        1 -> timestamp = text(type)
                        2 -> encrypted = number(type) != 0L
                        3 -> content = data(type)
                        else -> skip(type)
                    } }
                    if (encrypted) throw ActualException(ActualException.Kind.UNSUPPORTED, "Encrypted budgets are not supported. Pending changes were retained.")
                    LogicalClock.validate(timestamp)
                    var dataset = ""; var row = ""; var column = ""; var value = ""
                    Reader(content).fields { nested, type -> when (nested) {
                        1 -> dataset = text(type)
                        2 -> row = text(type)
                        3 -> column = text(type)
                        4 -> value = text(type)
                        else -> skip(type)
                    } }
                    if (dataset.isEmpty() || row.isEmpty() || column.isEmpty()) invalid()
                    messages += ProtocolChange(timestamp, dataset, row, column, decodeValue(value))
                }
                2 -> merkle = text(wire)
                else -> skip(wire)
            }
        }
        val tree = try { JsonParser.parseString(merkle ?: throw IllegalArgumentException()).asJsonObject }
        catch (e: RuntimeException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned invalid sync state", e) }
        return SyncBatch(messages, tree)
    }

    fun encodeValue(value: JsonElement): String = when {
        value.isJsonNull -> "0:"
        value.isJsonPrimitive && value.asJsonPrimitive.isString -> "S:${value.asString}"
        value.isJsonPrimitive && value.asJsonPrimitive.isNumber -> "N:${value.asBigDecimal.stripTrailingZeros().toPlainString()}"
        else -> throw ActualException(ActualException.Kind.INVALID_DATA, "Actual sync supports only string, number, and null field values")
    }

    fun decodeValue(value: String): JsonElement = when {
        value == "0:" -> JsonNull.INSTANCE
        value.startsWith("S:") -> JsonPrimitive(value.substring(2))
        value.startsWith("N:") -> try { JsonPrimitive(BigDecimal(value.substring(2))) }
            catch (e: NumberFormatException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned an invalid numeric value", e) }
        else -> throw ActualException(ActualException.Kind.UNSUPPORTED, "This budget uses a newer field format. Update ActualExpenses before syncing.")
    }

    private fun invalid(): Nothing = throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned malformed sync data")

    private class Writer {
        private val output = ByteArrayOutputStream()
        private fun number(number: Long) {
            var value = number
            while (value >= 128) { output.write((value.toInt() and 127) or 128); value = value ushr 7 }
            output.write(value.toInt())
        }
        fun bytes(field: Int, bytes: ByteArray) { number((field * 8 + 2).toLong()); number(bytes.size.toLong()); output.write(bytes) }
        fun string(field: Int, value: String) = bytes(field, value.toByteArray(Charsets.UTF_8))
        fun result(): ByteArray = output.toByteArray()
    }

    private class Reader(private val bytes: ByteArray) {
        private var position = 0
        fun fields(consume: Reader.(Int, Int) -> Unit) {
            while (position < bytes.size) {
                val tag = varint()
                if (tag <= 0 || tag > Int.MAX_VALUE) invalid()
                consume((tag ushr 3).toInt(), (tag and 7).toInt())
            }
        }
        private fun varint(): Long {
            var value = 0L
            for (shift in 0..63 step 7) {
                if (position >= bytes.size) invalid()
                val byte = bytes[position++].toInt() and 255
                if (shift == 63 && byte > 1) invalid()
                value = value or ((byte and 127).toLong() shl shift)
                if (byte < 128) return value
            }
            invalid()
        }
        fun data(wire: Int): ByteArray {
            if (wire != 2) invalid()
            val length = varint()
            if (length < 0 || length > bytes.size - position) invalid()
            return bytes.copyOfRange(position, position + length.toInt()).also { position += length.toInt() }
        }
        fun text(wire: Int): String = data(wire).toString(Charsets.UTF_8)
        fun number(wire: Int): Long { if (wire != 0) invalid(); return varint() }
        fun skip(wire: Int) { when (wire) {
            0 -> varint()
            1 -> advance(8)
            2 -> { val length = varint(); if (length < 0 || length > Int.MAX_VALUE) invalid(); advance(length.toInt()) }
            5 -> advance(4)
            else -> invalid()
        } }
        private fun advance(length: Int) { if (length > bytes.size - position) invalid(); position += length }
    }
}
