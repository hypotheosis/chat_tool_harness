package com.example.chat_tool_harness

import android.util.JsonReader
import android.util.JsonToken
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.io.InputStreamReader
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.CodingErrorAction
import java.nio.charset.StandardCharsets

/** Incremental scalar-only protocol decoder; retains one frame and bounded accepted pages. */
internal object PdfWorkerProtocol {
    private const val FRAME = 65_536
    private const val WIRE = 16 * 1024 * 1024
    private const val TEXT = 2 * 1024 * 1024
    private val keys = setOf("version", "request_id", "sequence", "kind", "physical_page",
        "status", "text", "complete", "page_count", "error_code")
    private val codes = setOf("busy", "limit", "invalid_input", "unsupported_encrypted", "timeout",
        "worker_died", "protocol", "unavailable", "cancelled")
    class Failure(val code: String) : Exception(code)
    private fun invalid(): Nothing = throw Failure("protocol")
    private fun limit(): Nothing = throw Failure("limit")

    fun read(input: InputStream, id: String, maxPages: Int, maxTextBytes: Int): Map<String, Any> {
        if (id.length != 36 || id.withIndex().any { (i, c) ->
                if (i in listOf(8, 13, 18, 23)) c != '-'
                else !((c in '0'..'9') || (c in 'a'..'f') || (c in 'A'..'F'))
            }) invalid()
        var wire = 0
        var sequence = 0L
        var begun = false
        var textBytes = 0
        val pages = ArrayList<Map<String, Any>>()
        var active: MutableMap<String, Any>? = null
        while (true) {
            val prefix = ByteArray(4)
            if (!exact(input, prefix)) invalid()
            val size = ByteBuffer.wrap(prefix).order(ByteOrder.LITTLE_ENDIAN).int.toLong() and 0xffffffffL
            if (size == 0L || size > FRAME) limit()
            wire += 4 + size.toInt()
            if (wire > WIRE) limit()
            val bytes = ByteArray(size.toInt())
            if (!exact(input, bytes)) invalid()
            val record = parse(bytes)
            if (record["version"] != 1L || record["request_id"] != id ||
                record["sequence"] != sequence || sequence > 200_000L) invalid()
            sequence++
            val kind = record["kind"] as? String ?: invalid()
            val allowed = when (kind) {
                "begin" -> setOf("version", "request_id", "sequence", "kind")
                "page_begin", "page_end" -> setOf("version", "request_id", "sequence", "kind", "physical_page", "status")
                "text_chunk" -> setOf("version", "request_id", "sequence", "kind", "text")
                "done" -> setOf("version", "request_id", "sequence", "kind", "page_count", "complete")
                "error" -> setOf("version", "request_id", "sequence", "kind", "error_code")
                else -> invalid()
            }
            if (record.keys != allowed) invalid()
            when (kind) {
                "begin" -> { if (sequence != 1L) invalid(); begun = true }
                "error" -> {
                    if (sequence != 1L || input.read() != -1) invalid()
                    val code = record["error_code"] as? String ?: invalid()
                    if (code !in codes) invalid()
                    throw Failure(code)
                }
                "page_begin" -> {
                    if (!begun || active != null || pages.size >= maxPages) invalid()
                    val page = record["physical_page"] as? Long ?: invalid()
                    val status = record["status"] as? String ?: invalid()
                    if (page != pages.size + 1L || status !in setOf("text", "no_extractable_text", "failed")) invalid()
                    active = mutableMapOf("physicalPage" to page.toInt(), "status" to status, "text" to "")
                }
                "text_chunk" -> {
                    val page = active ?: invalid()
                    if (page["status"] != "text") invalid()
                    val chunk = record["text"] as? String ?: invalid()
                    val count = chunk.toByteArray(StandardCharsets.UTF_8).size
                    if (count == 0 || count > 8192) limit()
                    textBytes += count
                    if (textBytes > maxTextBytes || textBytes > TEXT) limit()
                    // Bound the UTF-16 builder before copying an untrusted chunk.
                    page["text"] = (page["text"] as String) + chunk
                }
                "page_end" -> {
                    val page = active ?: invalid()
                    if (record["physical_page"] != (page["physicalPage"] as Int).toLong() ||
                        record["status"] != page["status"]) invalid()
                    if ((page["status"] == "text") == (page["text"] == "")) invalid()
                    pages.add(page)
                    active = null
                }
                "done" -> {
                    if (!begun || active != null || pages.isEmpty() ||
                        record["page_count"] != pages.size.toLong() || input.read() != -1) invalid()
                    val complete = record["complete"] as? Boolean ?: invalid()
                    if (complete != pages.none { it["status"] == "failed" }) invalid()
                    return mapOf("kind" to "extracted", "requestId" to id,
                        "complete" to complete, "pages" to pages)
                }
            }
        }
    }

    private fun exact(input: InputStream, bytes: ByteArray): Boolean {
        var offset = 0
        while (offset < bytes.size) {
            val count = input.read(bytes, offset, bytes.size - offset)
            if (count < 0) return false
            if (count == 0) continue
            offset += count
        }
        return true
    }

    private fun parse(bytes: ByteArray): Map<String, Any> {
        val text = try {
            StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT)
                .decode(ByteBuffer.wrap(bytes)).toString()
        } catch (_: Exception) { invalid() }
        val reader = JsonReader(InputStreamReader(ByteArrayInputStream(text.toByteArray(StandardCharsets.UTF_8)), StandardCharsets.UTF_8))
        reader.isLenient = false
        val fields = HashMap<String, Any>()
        try {
            reader.beginObject()
            while (reader.hasNext()) {
                val key = reader.nextName()
                if (key !in keys || fields.containsKey(key)) invalid()
                fields[key] = when (reader.peek()) {
                    JsonToken.STRING -> reader.nextString()
                    JsonToken.BOOLEAN -> reader.nextBoolean()
                    JsonToken.NUMBER -> {
                        val literal = reader.nextString()
                        if (literal.isEmpty() || literal.any { !it.isDigit() } || literal.length > 10) invalid()
                        literal.toLongOrNull() ?: invalid()
                    }
                    else -> invalid() // Reject nested values before descending.
                }
            }
            reader.endObject()
            if (reader.peek() != JsonToken.END_DOCUMENT) invalid()
        } catch (_: Failure) { throw Failure("protocol")
        } catch (_: Exception) { invalid()
        } finally { reader.close() }
        return fields
    }
}
