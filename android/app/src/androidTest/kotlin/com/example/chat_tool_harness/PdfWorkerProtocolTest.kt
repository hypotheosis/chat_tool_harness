package com.example.chat_tool_harness

import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.nio.charset.StandardCharsets

/** Calls the shipping Android JsonReader decoder with controlled, bounded streams. */
class PdfWorkerProtocolTest {
    private val id = "123e4567-e89b-12d3-a456-426614174000"
    private fun record(seq: Int, kind: String, fields: String = "", requestId: String = id,
        version: Int = 1) = """{"version":$version,"request_id":"$requestId","sequence":$seq,"kind":"$kind"$fields}"""
    private fun frame(json: String): ByteArray = frame(json.toByteArray(StandardCharsets.UTF_8))
    private fun frame(bytes: ByteArray) = ByteArrayOutputStream().apply {
        val size = bytes.size
        write(byteArrayOf(size.toByte(), (size shr 8).toByte(), (size shr 16).toByte(), (size shr 24).toByte()))
        write(bytes)
    }.toByteArray()
    private fun stream(vararg frames: ByteArray) = ByteArrayInputStream(frames.fold(ByteArrayOutputStream()) { out, bytes -> out.apply { write(bytes) } }.toByteArray())
    private val begin get() = frame(record(0, "begin"))
    private val pageBegin get() = frame(record(1, "page_begin", ",\"physical_page\":1,\"status\":\"text\""))
    private val chunk get() = frame(record(2, "text_chunk", ",\"text\":\"a\""))
    private val pageEnd get() = frame(record(3, "page_end", ",\"physical_page\":1,\"status\":\"text\""))
    private val done get() = frame(record(4, "done", ",\"page_count\":1,\"complete\":true"))
    private fun failure(input: InputStream, code: String = "protocol", pages: Int = 500, text: Int = 2 * 1024 * 1024) {
        try { PdfWorkerProtocol.read(input, id, pages, text); fail("accepted invalid protocol") }
        catch (error: PdfWorkerProtocol.Failure) { assertEquals(code, error.code) }
    }

    @Test fun validAndExactTextLimit() {
        val result = PdfWorkerProtocol.read(stream(begin, pageBegin, chunk, pageEnd, done), id, 1, 1)
        assertEquals(true, result["complete"])
        assertEquals("a", (result["pages"] as List<*>).first().let { (it as Map<*, *>)["text"] })
        failure(stream(begin, pageBegin, chunk, pageEnd, done), "limit", text = 0)
    }

    @Test fun oversizedPrefixIsRejectedBeforePayloadReadOrAllocation() {
        val prefixOnly = object : InputStream() {
            private val prefix = byteArrayOf(1, 0, 1, 0) // 65,537
            var offset = 0
            override fun read(): Int = if (offset < 4) prefix[offset++].toInt() and 255 else error("payload read")
            override fun read(buffer: ByteArray, off: Int, len: Int): Int {
                assertTrue("must not allocate payload", buffer.size <= 4)
                if (offset == 4) error("payload read")
                val n = minOf(len, 4 - offset)
                prefix.copyInto(buffer, off, offset, offset + n); offset += n
                return n
            }
        }
        failure(prefixOnly, "limit")
        assertEquals(4, prefixOnly.offset)
    }

    @Test fun exact64KiBFrameIsAccepted() {
        val json = record(0, "begin").padEnd(65_536, ' ')
        assertEquals(65_536 + 4, frame(json).size)
        assertEquals("extracted", PdfWorkerProtocol.read(stream(frame(json), pageBegin, chunk, pageEnd, done), id, 1, 1)["kind"])
        // The prefix-only test above proves one-over rejection without a payload allocation.
    }

    /** Builds only one <=64KiB frame at a time; never buffers the 16MiB wire. */
    private inner class LazyWire(private val target: Int) : InputStream() {
        private var index = 0
        private var current = ByteArray(0)
        private var offset = 0
        var consumed = 0
        var peakFrame = 0
        private var padding = target - (0..259).sumOf { json(it).toByteArray().size + 4 }
        private fun json(i: Int): String = when (i) {
            0 -> record(i, "begin")
            1 -> record(i, "page_begin", ",\"physical_page\":1,\"status\":\"text\"")
            258 -> record(i, "page_end", ",\"physical_page\":1,\"status\":\"text\"")
            259 -> record(i, "done", ",\"page_count\":1,\"complete\":true")
            else -> record(i, "text_chunk", ",\"text\":\"x\"")
        }
        private fun availableFrame(): Boolean {
            if (offset < current.size) return true
            if (index == 260) return false
            val body = json(index++)
            val extra = minOf(padding, 65_536 - body.toByteArray().size)
            padding -= extra
            current = frame(body.padEnd(body.length + extra, ' ')); offset = 0
            peakFrame = maxOf(peakFrame, current.size)
            return true
        }
        override fun read(): Int {
            if (!availableFrame()) return -1
            consumed++; return current[offset++].toInt() and 255
        }
        override fun read(bytes: ByteArray, off: Int, len: Int): Int {
            if (!availableFrame()) return -1
            val n = minOf(len, current.size - offset)
            current.copyInto(bytes, off, offset, offset + n); offset += n; consumed += n
            return n
        }
    }
    @Test fun exact16MiBFramedWireAndOneOverUseBoundedLazyStream() {
        val exact = LazyWire(16 * 1024 * 1024)
        val result = PdfWorkerProtocol.read(exact, id, 1, 256)
        assertEquals("x".repeat(256), ((result["pages"] as List<*>)[0] as Map<*, *>)["text"])
        assertEquals(16 * 1024 * 1024, exact.consumed)
        assertEquals(65_536 + 4, exact.peakFrame)
        val over = LazyWire(16 * 1024 * 1024 + 1)
        failure(over, "limit", text = 256)
        assertTrue("over-limit payload must not be read", over.consumed < 16 * 1024 * 1024 + 1)
        assertEquals(65_536 + 4, over.peakFrame)
    }

    @Test fun malformedUtf8FlatSchemaAndNumbersFail() {
        val base = record(0, "begin")
        for (bad in listOf(
            frame(byteArrayOf(0xc3.toByte(), 0x28)),
            frame(base.replace("\"kind\"", "\"kind\":\"begin\",\"kind\"")),
            frame(base.replace("\"kind\":\"begin\"", "\"kind\":{}")),
            frame(base.replace("\"kind\":\"begin\"", "\"kind\":[]")),
            frame(base + " true"),
            frame(base.replace("\"kind\"", "\"unknown\":1,\"kind\"")),
            frame(base.replace("\"sequence\":0", "\"sequence\":-1")),
            frame(base.replace("\"sequence\":0", "\"sequence\":99999999999")),
        )) failure(stream(bad))
    }

    @Test fun versionIdentitySequenceAndOrderFail() {
        val wrongId = "123e4567-e89b-12d3-a456-426614174999"
        for (bad in listOf(
            frame(record(0, "begin", requestId = wrongId)),
            frame(record(0, "begin", version = 2)),
            frame(record(1, "begin")),
            frame(record(0, "page_begin", ",\"physical_page\":1,\"status\":\"text\"")),
        )) failure(stream(bad))
        failure(stream(begin, frame(record(1, "page_begin", ",\"physical_page\":2,\"status\":\"text\""))))
        failure(stream(begin, pageBegin, frame(record(2, "text_chunk", ",\"text\":\"a\"")),
            frame(record(3, "page_end", ",\"physical_page\":1,\"status\":\"failed\""))))
        failure(stream(begin, pageBegin, frame(record(4, "text_chunk", ",\"text\":\"a\""))))
    }

    @Test fun terminalRequiresEofAndCompleteFrame() {
        failure(stream(begin, pageBegin, chunk, pageEnd, done, begin))
        failure(stream(begin, pageBegin, chunk, pageEnd, done, byteArrayOf(1)))
        failure(stream(begin, pageBegin, chunk, pageEnd, frame(record(4, "done", ",\"page_count\":1,\"complete\":true")).copyOf(8)))
        failure(stream(begin, pageBegin, chunk, pageEnd))
        failure(stream(begin, pageBegin, chunk, pageEnd, done, byteArrayOf(0, 0, 0, 0)))
    }

    @Test fun chunkExactAndOverAndFrameExactAndOver() {
        val exact = "x".repeat(8192)
        val textFrame = frame(record(2, "text_chunk", ",\"text\":\"$exact\""))
        assertEquals(8192, (PdfWorkerProtocol.read(stream(begin, pageBegin, textFrame, pageEnd, done), id, 1, 8192)["pages"] as List<*>).let { ((it.first() as Map<*, *>)["text"] as String).length })
        failure(stream(begin, pageBegin, frame(record(2, "text_chunk", ",\"text\":\"${exact}x\""))), "limit")
        // Oversized frame prefix, even if its body is never delivered.
        failure(stream(byteArrayOf(1, 0, 1, 0)), "limit")
    }
}
