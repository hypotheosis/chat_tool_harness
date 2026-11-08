package com.example.chat_tool_harness

import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.IBinder
import android.os.ParcelFileDescriptor
import androidx.test.InstrumentationRegistry
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.TimeUnit

/** Concrete binding/pipe/clock seams; no worker process or wall-clock sleeps. */
class PdfWorkerHostTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = instrumentation.targetContext
    private val id = "123e4567-e89b-12d3-a456-426614174777"
    private fun fixture(): File = File(context.filesDir, "pdf_staging/host_test.pdf").apply {
        parentFile!!.mkdirs(); writeBytes(byteArrayOf(37, 80, 68, 70))
    }
    private fun args(file: File) = mapOf(
        "requestId" to id, "snapshotPath" to file.canonicalPath,
        "maxInputBytes" to 10 * 1024 * 1024, "maxPages" to 500,
        "maxTextBytes" to 2 * 1024 * 1024, "maxDecompressedBytes" to 4 * 1024 * 1024,
    )
    private class FakeBinding(context: Context) : PdfWorkerHost.Binding(context) {
        var connection: ServiceConnection? = null
        var binds = 0
        var unbinds = 0
        override fun bind(intent: Intent, connection: ServiceConnection): Boolean {
            this.connection = connection; binds++; return true
        }
        override fun unbind(connection: ServiceConnection) { unbinds++ }
    }
    private class FakeClock : PdfWorkerHost.Clock() {
        var time = 0L
        override fun now() = time
    }
    private class ManualScheduler : ScheduledThreadPoolExecutor(1) {
        private val actions = mutableListOf<Runnable>()
        fun count(): Int = actions.size
        override fun schedule(command: Runnable, delay: Long, unit: TimeUnit): java.util.concurrent.ScheduledFuture<*> {
            actions.add(command)
            return super.schedule({}, 1, TimeUnit.DAYS)
        }
        fun fire(delayIndex: Int) { actions[delayIndex].run() }
    }
    private fun onMain(block: () -> Unit) = instrumentation.runOnMainSync(block)
    private fun engine(): FlutterEngine {
        lateinit var created: FlutterEngine
        onMain { created = FlutterEngine(context, null, false) }
        return created
    }
    private fun close(engine: FlutterEngine) = onMain { engine.destroy() }
    private fun onMainHost(engine: FlutterEngine, binding: FakeBinding,
        scheduler: java.util.concurrent.ScheduledExecutorService = ManualScheduler(),
        open: (File) -> ParcelFileDescriptor = { ParcelFileDescriptor.open(it, ParcelFileDescriptor.MODE_READ_ONLY) },
    ): PdfWorkerHost {
        lateinit var host: PdfWorkerHost
        onMain { host = PdfWorkerHost(context, engine, binding = binding, scheduler = scheduler, open = open) }
        return host
    }
    private fun result() = object : MethodChannel.Result {
        val latch = CountDownLatch(1)
        @Volatile var reply: Map<*, *>? = null
        override fun success(value: Any?) { reply = value as Map<*, *>; latch.countDown() }
        override fun error(code: String, message: String?, details: Any?) = fail(code)
        override fun notImplemented() = fail("unimplemented")
        fun await(): Map<*, *> { assertTrue(latch.await(3, TimeUnit.SECONDS)); return reply!! }
    }
    private class ControlledWorker(private val blockStart: Boolean = false) : IPdfWorker.Stub() {
        val entered = CountDownLatch(1)
        val releaseStart = CountDownLatch(1)
        var output: ParcelFileDescriptor? = null
        var starts = 0
        var cancels = 0
        var acknowledges = 0
        @Volatile var alive = true
        var recipient: IBinder.DeathRecipient? = null
        val binder = object : IBinder by asBinder() {
            override fun queryLocalInterface(descriptor: String) = this@ControlledWorker
            override fun isBinderAlive() = alive
            override fun linkToDeath(recipient: IBinder.DeathRecipient, flags: Int) { this@ControlledWorker.recipient = recipient }
            override fun unlinkToDeath(recipient: IBinder.DeathRecipient, flags: Int) = true
        }
        override fun start(requestId: String, generation: Long, inputFd: ParcelFileDescriptor,
            outputFd: ParcelFileDescriptor, maxInputBytes: Int, maxPages: Int, maxTextBytes: Int,
            maxDecompressedBytes: Int, events: IPdfWorkerEvents) {
            starts++; output = ParcelFileDescriptor.dup(outputFd.fileDescriptor); entered.countDown()
            if (blockStart) assertTrue(releaseStart.await(10, TimeUnit.SECONDS))
        }
        override fun cancel(requestId: String, generation: Long, events: IPdfWorkerEvents) { cancels++ }
        override fun acknowledge(requestId: String, generation: Long, events: IPdfWorkerEvents) { acknowledges++ }
        fun die() { alive = false; recipient!!.binderDied() }
        fun write(bytes: ByteArray) { java.io.FileOutputStream(output!!.fileDescriptor).write(bytes) }
        fun eof() { output?.close(); output = null }
    }
    private fun validOutput(): ByteArray {
        val records = listOf(
            "\"kind\":\"begin\"",
            "\"kind\":\"page_begin\",\"physical_page\":1,\"status\":\"no_extractable_text\"",
            "\"kind\":\"page_end\",\"physical_page\":1,\"status\":\"no_extractable_text\"",
            "\"kind\":\"done\",\"page_count\":1,\"complete\":true",
        )
        return java.io.ByteArrayOutputStream().apply {
            records.forEachIndexed { sequence, fields ->
                val bytes = "{\"version\":1,\"request_id\":\"$id\",\"sequence\":$sequence,$fields}".toByteArray()
                write(java.nio.ByteBuffer.allocate(4).order(java.nio.ByteOrder.LITTLE_ENDIAN).putInt(bytes.size).array())
                write(bytes)
            }
        }.toByteArray()
    }
    private fun connect(binding: FakeBinding, worker: ControlledWorker) {
        onMain { binding.connection!!.onServiceConnected(android.content.ComponentName(context, PdfWorkerService::class.java), worker.binder) }
        assertTrue(worker.entered.await(3, TimeUnit.SECONDS))
    }
    private fun drain() { onMain {}; onMain {} }

    @Test fun validationPastDeadlineCannotBeatQueuedTimer() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val clock = FakeClock(); val scheduler = ManualScheduler()
        val readFinished = CountDownLatch(1)
        val reader = object : java.util.concurrent.ThreadPoolExecutor(1, 1, 0, TimeUnit.SECONDS, java.util.concurrent.LinkedBlockingQueue<Runnable>()) {
            override fun afterExecute(r: Runnable, error: Throwable?) { readFinished.countDown() }
        }
        lateinit var host: PdfWorkerHost
        onMain { host = PdfWorkerHost(context, engine, binding, clock, scheduler, reader = reader) }
        val reply = result(); val worker = ControlledWorker()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            onMain {
                worker.write(validOutput()); worker.eof()
                assertTrue(readFinished.await(3, TimeUnit.SECONDS))
                clock.time = 60_001
            }
            drain(); worker.die(); drain()
            assertEquals("timeout", reply.await()["code"])
        } finally { worker.releaseStart.countDown(); worker.eof(); reader.shutdownNow(); scheduler.shutdownNow(); close(engine); file.delete() }
    }

    @Test fun deathBeforeEofCannotBecomeSuccess() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val host = onMainHost(engine, binding); val reply = result(); val worker = ControlledWorker()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            worker.write(validOutput()) // done received, but EOF has not happened.
            worker.die(); drain(); worker.eof(); drain()
            assertEquals("worker_died", reply.await()["code"])
        } finally { worker.eof(); close(engine); file.delete() }
    }

    @Test fun blockedStartCannotBlockDeadlineOrMissingDeathGrace() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val clock = FakeClock(); val scheduler = ManualScheduler(); val worker = ControlledWorker(true)
        val control = java.util.concurrent.Executors.newFixedThreadPool(2)
        lateinit var host: PdfWorkerHost
        onMain { host = PdfWorkerHost(context, engine, binding, clock, scheduler, controlExecutor = control) }
        val reply = result()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            onMain { clock.time = 60_000; scheduler.fire(0) }; drain()
            onMain { assertEquals(2, scheduler.count()); clock.time += 5_000; scheduler.fire(1) }; drain()
            assertEquals("unavailable", reply.await()["code"])
            val next = result(); onMain { host.extract(args(file), next) }
            assertEquals("unavailable", next.await()["code"]); assertEquals(1, binding.binds)
            assertEquals(1L, worker.releaseStart.count) // start remains blocked throughout local response.
        } finally { worker.releaseStart.countDown(); worker.eof(); control.shutdownNow(); scheduler.shutdownNow(); close(engine); file.delete() }
    }

    @Test fun blockedRealCallbackCannotBlockFakeClockDeadlineOrConfirmedCleanup() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val clock = FakeClock(); val scheduler = ManualScheduler()
        val callbackEntered = CountDownLatch(1); val releaseCallback = CountDownLatch(1)
        val connected = CountDownLatch(1)
        var remote: IPdfWorker? = null
        val realConnection = object : ServiceConnection {
            override fun onServiceConnected(name: android.content.ComponentName, service: IBinder) {
                remote = IPdfWorker.Stub.asInterface(service); connected.countDown()
            }
            override fun onServiceDisconnected(name: android.content.ComponentName) = Unit
        }
        assertTrue(context.bindService(Intent(context, PdfWorkerService::class.java).putExtra("pdfFault", "withhold"), realConnection, Context.BIND_AUTO_CREATE))
        val control = java.util.concurrent.Executors.newFixedThreadPool(2)
        val reader = java.util.concurrent.Executors.newSingleThreadExecutor()
        lateinit var host: PdfWorkerHost
        onMain { host = PdfWorkerHost(context, engine, binding, clock, scheduler, control, reader) }
        val reply = result()
        try {
            assertTrue(connected.await(30, TimeUnit.SECONDS))
            val capability = object : IPdfWorkerEvents.Stub() {
                override fun started(requestId: String, generation: Long): Boolean {
                    callbackEntered.countDown()
                    assertTrue(releaseCallback.await(10, TimeUnit.SECONDS))
                    return false
                }
            }
            val forwarding = object : IPdfWorker.Stub() {
                override fun start(requestId: String, generation: Long, inputFd: ParcelFileDescriptor,
                    outputFd: ParcelFileDescriptor, maxInputBytes: Int, maxPages: Int, maxTextBytes: Int,
                    maxDecompressedBytes: Int, events: IPdfWorkerEvents) = remote!!.start(requestId, generation,
                        inputFd, outputFd, maxInputBytes, maxPages, maxTextBytes, maxDecompressedBytes, capability)
                override fun cancel(requestId: String, generation: Long, events: IPdfWorkerEvents) = remote!!.cancel(requestId, generation, capability)
                override fun acknowledge(requestId: String, generation: Long, events: IPdfWorkerEvents) = remote!!.acknowledge(requestId, generation, capability)
            }
            val binder = object : IBinder by remote!!.asBinder() {
                override fun queryLocalInterface(descriptor: String) = forwarding
            }
            onMain {
                host.extract(args(file), reply)
                binding.connection!!.onServiceConnected(android.content.ComponentName(context, PdfWorkerService::class.java), binder)
            }
            assertTrue(callbackEntered.await(3, TimeUnit.SECONDS))
            onMain { clock.time = 60_000; scheduler.fire(0) }; drain()
            assertEquals("timeout", reply.await()["code"])
            assertEquals(1L, releaseCallback.count)
            assertEquals("binderDeath", host.diagnostics()["lastDeath"])
            assertEquals(false, host.diagnostics()["unavailable"])
        } finally {
            releaseCallback.countDown(); control.shutdownNow(); reader.shutdownNow(); scheduler.shutdownNow()
            context.unbindService(realConnection); close(engine); file.delete()
        }
    }

    @Test fun deathQueuedBeforeEofCannotBecomeSuccessEvenWhenMainDelayed() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val readFinished = CountDownLatch(1)
        val reader = object : java.util.concurrent.ThreadPoolExecutor(1, 1, 0, TimeUnit.SECONDS, java.util.concurrent.LinkedBlockingQueue<Runnable>()) {
            override fun afterExecute(r: Runnable, error: Throwable?) { readFinished.countDown() }
        }
        lateinit var host: PdfWorkerHost
        onMain { host = PdfWorkerHost(context, engine, binding = binding, reader = reader) }
        val reply = result(); val worker = ControlledWorker()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            onMain {
                worker.die() // Binder death is observed before transport completion, even if main is delayed.
                worker.write(validOutput()); worker.eof()
                assertTrue(readFinished.await(3, TimeUnit.SECONDS))
            }
            assertEquals("worker_died", reply.await()["code"])
        } finally { worker.eof(); reader.shutdownNow(); close(engine); file.delete() }
    }

    @Test fun validEofQueuedBeforeBinderDeathPreservesResult() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val readFinished = CountDownLatch(1)
        val reader = object : java.util.concurrent.ThreadPoolExecutor(1, 1, 0, TimeUnit.SECONDS, java.util.concurrent.LinkedBlockingQueue<Runnable>()) {
            override fun afterExecute(r: Runnable, error: Throwable?) { readFinished.countDown() }
        }
        lateinit var host: PdfWorkerHost
        onMain { host = PdfWorkerHost(context, engine, binding = binding, reader = reader) }
        val reply = result(); val worker = ControlledWorker()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            onMain {
                worker.write(validOutput()); worker.eof()
                assertTrue(readFinished.await(3, TimeUnit.SECONDS))
                worker.die() // EOF/reader delivery is queued, but main validation has not run.
            }
            assertEquals("extracted", reply.await()["kind"])
        } finally { worker.eof(); reader.shutdownNow(); close(engine); file.delete() }
    }

    @Test fun validatedOutputStillRequiresDeathWithinCleanupGrace() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val scheduler = ManualScheduler(); val readFinished = CountDownLatch(1)
        val reader = object : java.util.concurrent.ThreadPoolExecutor(1, 1, 0, TimeUnit.SECONDS, java.util.concurrent.LinkedBlockingQueue<Runnable>()) {
            override fun afterExecute(r: Runnable, error: Throwable?) { readFinished.countDown() }
        }
        lateinit var host: PdfWorkerHost
        onMain { host = PdfWorkerHost(context, engine, binding = binding, scheduler = scheduler, reader = reader) }
        val reply = result(); val worker = ControlledWorker()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            worker.write(validOutput()); worker.eof()
            assertTrue(readFinished.await(3, TimeUnit.SECONDS)); drain()
            onMain { assertEquals("validated output must start the death grace", 2, scheduler.count()); scheduler.fire(1) }; drain()
            assertEquals("unavailable", reply.await()["code"])
            val next = result(); onMain { host.extract(args(file), next) }
            assertEquals("unavailable", next.await()["code"]); assertEquals(1, binding.binds)
        } finally { worker.eof(); reader.shutdownNow(); scheduler.shutdownNow(); close(engine); file.delete() }
    }

    @Test fun disconnectInvalidatesIncompleteOutputAndLateRebindNeverStarts() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val host = onMainHost(engine, binding); val reply = result(); val worker = ControlledWorker()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            onMain { binding.connection!!.onServiceDisconnected(android.content.ComponentName(context, PdfWorkerService::class.java)) }; drain()
            assertEquals(1, binding.unbinds)
            try { worker.write(validOutput()) } catch (_: java.io.IOException) { /* Response abandoned on disconnect. */ }
            worker.eof()
            onMain { binding.connection!!.onServiceConnected(android.content.ComponentName(context, PdfWorkerService::class.java), worker.binder) }; drain()
            worker.die(); drain()
            assertEquals("worker_died", reply.await()["code"])
            assertEquals(1, worker.starts); assertEquals(1, binding.binds)
        } finally { worker.eof(); close(engine); file.delete() }
    }

    @Test fun cancelPartialOutputClosesResponseAndDisconnectNeverRedelivers() {
        val file = fixture(); val binding = FakeBinding(context); val engine = engine()
        val scheduler = ManualScheduler(); val worker = ControlledWorker()
        val host = onMainHost(engine, binding, scheduler); val reply = result()
        try {
            onMain { host.extract(args(file), reply) }; connect(binding, worker)
            worker.write(byteArrayOf(4, 0))
            onMain { assertTrue(host.cancel(id)) }
            onMain { binding.connection!!.onServiceDisconnected(android.content.ComponentName(context, PdfWorkerService::class.java)) }; drain()
            assertEquals(1, binding.unbinds)
            onMain { binding.connection!!.onServiceConnected(android.content.ComponentName(context, PdfWorkerService::class.java), worker.binder) }; drain()
            assertEquals(1, worker.starts)
            worker.die(); worker.eof()
            assertEquals("cancelled", reply.await()["code"])
        } finally { worker.eof(); scheduler.shutdownNow(); close(engine); file.delete() }
    }

    @Test fun replacementBetweenPathCheckAndOpenIsRejected() {
        val file = fixture()
        val binding = FakeBinding(context)
        val engine = engine()
        val original = file.readBytes()
        val request = args(file)
        val host = onMainHost(engine, binding = binding, open = { path ->
            val moved = File(path.parentFile, "moved_${path.name}")
            moved.delete()
            assertTrue(path.renameTo(moved))
            path.writeBytes(original)
            ParcelFileDescriptor.open(path, ParcelFileDescriptor.MODE_READ_ONLY)
        })
        val reply = result()
        onMain { host.extract(request, reply) }
        assertEquals("invalid_input", reply.await()["code"])
        assertEquals(0, binding.binds)
        close(engine); file.delete()
    }

    @Test fun cancelBeforeConnectionUnbindsAndLateConnectionNeverStarts() {
        val file = fixture()
        val binding = FakeBinding(context)
        val engine = engine()
        val host = onMainHost(engine, binding = binding)
        val reply = result()
        onMain { host.extract(args(file), reply); host.cancel(id) }
        assertEquals("cancelled", reply.await()["code"])
        assertEquals(1, binding.unbinds)
        val worker = object : IPdfWorker.Stub() {
            var starts = 0
            override fun start(requestId: String, generation: Long, inputFd: ParcelFileDescriptor,
                outputFd: ParcelFileDescriptor, maxInputBytes: Int, maxPages: Int, maxTextBytes: Int,
                maxDecompressedBytes: Int, events: IPdfWorkerEvents) { starts++ }
            override fun acknowledge(requestId: String, generation: Long, events: IPdfWorkerEvents) = Unit
            override fun cancel(requestId: String, generation: Long, events: IPdfWorkerEvents) = Unit
        }
        onMain { binding.connection!!.onServiceConnected(android.content.ComponentName(context, PdfWorkerService::class.java), worker) }
        assertEquals(0, worker.starts)
        close(engine); file.delete()
    }

    @Test fun missingDeathGraceLatchesUnavailableAndRefusesRebind() {
        val file = fixture()
        val binding = FakeBinding(context)
        val scheduler = ManualScheduler()
        val engine = engine()
        val host = onMainHost(engine, binding = binding, scheduler = scheduler)
        val first = result()
        onMain { host.extract(args(file), first) }
        val live = object : IPdfWorker.Stub() {
            override fun start(requestId: String, generation: Long, inputFd: ParcelFileDescriptor,
                outputFd: ParcelFileDescriptor, maxInputBytes: Int, maxPages: Int, maxTextBytes: Int,
                maxDecompressedBytes: Int, events: IPdfWorkerEvents) = Unit
            override fun acknowledge(requestId: String, generation: Long, events: IPdfWorkerEvents) = Unit
            override fun cancel(requestId: String, generation: Long, events: IPdfWorkerEvents) = Unit
        }
        onMain {
            binding.connection!!.onServiceConnected(android.content.ComponentName(context, PdfWorkerService::class.java), live)
        }
        onMain { host.cancel(id) }
        onMain { assertTrue("grace timer missing", scheduler.count() >= 2); scheduler.fire(1) }
        assertEquals("unavailable", first.await()["code"])
        val next = result()
        onMain { host.extract(args(file), next) }
        assertEquals("unavailable", next.await()["code"])
        assertEquals(1, binding.binds)
        close(engine); file.delete(); scheduler.shutdownNow()
    }
}
