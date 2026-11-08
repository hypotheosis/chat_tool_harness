package com.example.chat_tool_harness

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Looper
import androidx.test.platform.app.InstrumentationRegistry
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.StandardMethodCodec
import io.flutter.plugin.common.FlutterException
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FilterInputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.nio.ByteBuffer
import java.util.Collections
import java.util.UUID
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executor
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

class DocumentAccessTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private fun main(action: () -> Unit) = instrumentation.runOnMainSync(action)
    private fun await(latch: CountDownLatch) = assertTrue("controlled I/O timed out", latch.await(5, TimeUnit.SECONDS))

    private class Host(context: Context) : Activity() {
        init { attachBaseContext(context) }
        val launches = mutableListOf<Intent>()
        override fun startActivityForResult(intent: Intent, requestCode: Int) { launches.add(intent) }
        override fun startActivity(intent: Intent) { launches.add(intent) }
    }
    private class Tasks : Executor {
        private val tasks = CopyOnWriteArrayList<FutureTask<Unit>>()
        override fun execute(command: Runnable) {
            val task = FutureTask<Unit> { command.run() }
            tasks.add(task)
            Thread(task, "task10-controlled-io").start()
        }
        fun finish() {
            var index = 0
            while (index < tasks.size) tasks[index++].get(5, TimeUnit.SECONDS)
        }
    }
    private class Reply : BinaryMessenger.BinaryReply {
        val values = Collections.synchronizedList(mutableListOf<String>())
        override fun reply(message: ByteBuffer?) {
            checkNotNull(message).rewind()
            values.add(try { StandardMethodCodec.INSTANCE.decodeEnvelope(message).toString() }
                catch (error: FlutterException) { "error:${error.code}" })
        }
    }
    private class Messenger : BinaryMessenger {
        var handler: BinaryMessenger.BinaryMessageHandler? = null
        override fun send(channel: String, message: ByteBuffer?) = Unit
        override fun send(channel: String, message: ByteBuffer?, callback: BinaryMessenger.BinaryReply?) = Unit
        override fun setMessageHandler(channel: String, handler: BinaryMessenger.BinaryMessageHandler?) { this.handler = handler }
        fun call(method: String, file: File): Reply {
            val reply = Reply()
            val message = StandardMethodCodec.INSTANCE.encodeMethodCall(MethodCall(method, mapOf("privatePath" to file.path, "name" to "fixture.md")))
            message.rewind()
            checkNotNull(handler).onMessage(message, reply)
            return reply
        }
    }
    private class Output(private val blockWrite: Boolean = false, private val fail: Boolean = false, private val blockClose: Boolean = false) : OutputStream() {
        val writing = CountDownLatch(1)
        val releaseWrite = CountDownLatch(if (blockWrite) 1 else 0)
        val closing = CountDownLatch(1)
        val releaseClose = CountDownLatch(if (blockClose) 1 else 0)
        val closes = AtomicInteger()
        val bytes = ByteArrayOutputStream()
        @Volatile var closedOnMain = false
        override fun write(value: Int) = write(byteArrayOf(value.toByte()), 0, 1)
        override fun write(buffer: ByteArray, offset: Int, length: Int) {
            writing.countDown()
            check(releaseWrite.await(5, TimeUnit.SECONDS))
            if (fail) throw IOException("controlled provider failure")
            bytes.write(buffer, offset, length)
        }
        override fun close() {
            closedOnMain = Looper.myLooper() == Looper.getMainLooper()
            closes.incrementAndGet()
            closing.countDown()
            check(releaseClose.await(5, TimeUnit.SECONDS))
        }
    }
    private class Io(val output: Output, val blockOpen: Boolean = false, blockInput: Boolean = false, blockInputClose: Boolean = false) : DocumentIo {
        val opening = CountDownLatch(1)
        val releaseOpen = CountDownLatch(if (blockOpen) 1 else 0)
        val inputCloses = AtomicInteger()
        val inputOpening = CountDownLatch(1)
        val releaseInput = CountDownLatch(if (blockInput) 1 else 0)
        val inputClosing = CountDownLatch(1)
        val inputCloseStarted = CountDownLatch(1)
        val releaseInputClose = CountDownLatch(if (blockInputClose) 1 else 0)
        @Volatile var inputClosedOnMain = false
        override fun openInput(file: File): InputStream {
            inputOpening.countDown()
            check(releaseInput.await(5, TimeUnit.SECONDS))
            return object : FilterInputStream(file.inputStream()) {
                override fun close() {
                    inputClosedOnMain = Looper.myLooper() == Looper.getMainLooper()
                    inputCloses.incrementAndGet()
                    inputCloseStarted.countDown()
                    check(releaseInputClose.await(5, TimeUnit.SECONDS))
                    super.close()
                    inputClosing.countDown()
                }
            }
        }
        override fun openOutput(uri: Uri): OutputStream {
            assertEquals("content://task10-controlled/destination", uri.toString())
            opening.countDown()
            check(releaseOpen.await(5, TimeUnit.SECONDS))
            return output
        }
    }
    private inner class Fixture(val io: Io) {
        val file = File(File(instrumentation.targetContext.filesDir, "agent/artifacts").apply { mkdirs() }, "${UUID.randomUUID()}.txt").apply { writeText("# generated é") }
        val tasks = Tasks()
        val messenger = Messenger()
        lateinit var documents: DocumentAccess
        lateinit var host: Host
        init { main {
            host = Host(instrumentation.targetContext)
            documents = DocumentAccess(host, MethodChannel(messenger, "task10-documents"), io, tasks)
        } }
        fun save(): Reply {
            lateinit var reply: Reply
            main { reply = messenger.call("saveArtifact", file) }
            return reply
        }
        fun picked() = main { assertTrue(documents.onActivityResult(DocumentAccess.REQUEST_SAVE, Activity.RESULT_OK, Intent().setData(Uri.parse("content://task10-controlled/destination")))) }
        fun close() = main { documents.close() }
        fun drain() { tasks.finish(); main {} }
        fun dispose() {
            io.releaseOpen.countDown(); io.releaseInput.countDown(); io.releaseInputClose.countDown(); io.output.releaseWrite.countDown(); io.output.releaseClose.countDown()
            close(); drain(); file.delete()
        }
    }

    @Test fun copyRetainsBusyAndHostCloseCancelsExactlyOnceWithoutBlockingMain() {
        val output = Output(blockWrite = true, blockClose = true)
        val f = Fixture(Io(output))
        try {
            val result = f.save(); f.picked(); await(output.writing)
            lateinit var second: Reply
            main { second = f.messenger.call("shareArtifact", f.file) }
            f.close(); f.close()
            // A broken synchronous close cannot pass: releaseClose is still held here.
            val afterClose = result.values.toList()
            assertEquals("busy until copy terminal", listOf("error:unavailable"), second.values)
            assertEquals("close settles immediately", listOf("cancelled"), afterClose)
            await(output.closing); await(f.io.inputClosing)
            assertFalse("resource closure cannot block the main looper", output.closedOnMain)
            output.releaseWrite.countDown(); output.releaseClose.countDown(); f.drain()
            assertEquals("no saved after close or repeated terminal", listOf("cancelled"), result.values)
            assertEquals(1, output.closes.get()); assertEquals(1, f.io.inputCloses.get())
            assertFalse(output.closedOnMain); assertFalse(f.io.inputClosedOnMain)
            assertNull(f.messenger.handler)
        } finally { f.dispose() }
    }

    @Test fun lateOutputOpenAfterCloseIsClosedAndNeverReportsSaved() {
        val output = Output()
        val f = Fixture(Io(output, blockOpen = true))
        try {
            val result = f.save(); f.picked(); await(f.io.opening)
            f.close(); val afterClose = result.values.toList()
            f.io.releaseOpen.countDown(); f.drain()
            assertEquals(listOf("cancelled"), afterClose)
            assertEquals(listOf("cancelled"), result.values)
            assertEquals(0, output.bytes.size()); assertEquals(1, output.closes.get())
            assertFalse(output.closedOnMain)
        } finally { f.dispose() }
    }

    @Test fun lateInputOpenAfterCloseIsClosedWithoutWritingOrLateSaved() {
        val output = Output()
        val f = Fixture(Io(output, blockInput = true))
        try {
            val result = f.save(); f.picked(); await(f.io.inputOpening)
            f.close(); await(output.closing)
            assertEquals(listOf("cancelled"), result.values)
            f.io.releaseInput.countDown(); f.drain()
            assertEquals(listOf("cancelled"), result.values)
            assertEquals(0, output.bytes.size()); assertEquals(1, output.closes.get())
            assertEquals(1, f.io.inputCloses.get()); assertFalse(f.io.inputClosedOnMain)
        } finally { f.dispose() }
    }

    @Test fun successfulWorkerQueuedBeforeCloseCannotReportLateSaved() {
        val f = Fixture(Io(Output()))
        try {
            val result = f.save()
            main {
                f.documents.onActivityResult(DocumentAccess.REQUEST_SAVE, Activity.RESULT_OK, Intent().setData(Uri.parse("content://task10-controlled/destination")))
                // Hold the main queue until the successful worker has enqueued its completion.
                f.tasks.finish()
                f.documents.close()
            }
            f.drain()
            assertEquals(listOf("cancelled"), result.values)
            assertEquals("# generated é", f.io.output.bytes.toString("UTF-8"))
            assertEquals(1, f.io.output.closes.get()); assertEquals(1, f.io.inputCloses.get())
        } finally { f.dispose() }
    }

    @Test fun cancellationClosesOutputEvenWhenWorkerAlreadyBlockedClosingInput() {
        val f = Fixture(Io(Output(), blockInputClose = true))
        try {
            val result = f.save(); f.picked(); await(f.io.inputCloseStarted)
            f.close()
            assertEquals(listOf("cancelled"), result.values)
            assertTrue("output cleanup must not wait for blocked input close", f.io.output.closing.await(2, TimeUnit.SECONDS))
            f.io.releaseInputClose.countDown(); f.drain()
            assertEquals(listOf("cancelled"), result.values)
            assertEquals(1, f.io.output.closes.get()); assertEquals(1, f.io.inputCloses.get())
            assertFalse(f.io.output.closedOnMain); assertFalse(f.io.inputClosedOnMain)
        } finally { f.dispose() }
    }

    @Test fun pickerHostCloseCancelsOnceAndIgnoresLatePickerResult() {
        val f = Fixture(Io(Output()))
        try {
            val result = f.save(); f.close(); f.close(); f.picked(); f.drain()
            assertEquals(listOf("cancelled"), result.values)
            assertEquals(1L, f.io.opening.count); assertEquals(1L, f.io.inputOpening.count)
        } finally { f.dispose() }
    }

    @Test fun normalCopyAndFailureEachSettleOnceCloseResourcesAndReleaseBusy() {
        for (fail in listOf(false, true)) {
            val output = Output(fail = fail)
            val f = Fixture(Io(output))
            try {
                val result = f.save(); f.picked(); f.drain()
                assertEquals(listOf(if (fail) "error:unavailable" else "saved"), result.values)
                if (!fail) assertEquals("# generated é", output.bytes.toString("UTF-8"))
                assertEquals(1, output.closes.get()); assertEquals(1, f.io.inputCloses.get())
                lateinit var next: Reply
                main { next = f.messenger.call("saveArtifact", f.file) }
                assertTrue(next.values.isEmpty())
                main { f.documents.onActivityResult(DocumentAccess.REQUEST_SAVE, Activity.RESULT_CANCELED, null) }
                assertEquals(listOf("cancelled"), next.values)
                f.close(); assertEquals(1, result.values.size)
            } finally { f.dispose() }
        }
    }
}
