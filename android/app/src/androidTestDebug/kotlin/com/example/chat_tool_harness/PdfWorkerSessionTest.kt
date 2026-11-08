package com.example.chat_tool_harness

import android.content.ComponentName
import android.content.Intent
import android.content.ServiceConnection
import android.os.IBinder
import android.os.ParcelFileDescriptor
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Direct Binder contract, not a mocked channel. */
class PdfWorkerSessionTest {
    private val id = "123e4567-e89b-12d3-a456-426614174000"
    private val context get() = androidx.test.InstrumentationRegistry.getInstrumentation().targetContext
    private fun connected(test: (IPdfWorker) -> Unit) {
        val ready = CountDownLatch(1)
        var control: IPdfWorker? = null
        val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName, binder: IBinder) {
                control = IPdfWorker.Stub.asInterface(binder); ready.countDown()
            }
            override fun onServiceDisconnected(name: ComponentName) = Unit
        }
        assertTrue(context.bindService(Intent(context, PdfWorkerService::class.java).putExtra("pdfFault", "withhold"), connection, 1))
        try {
            assertTrue(ready.await(30, TimeUnit.SECONDS))
            test(control!!)
        } finally { context.unbindService(connection) }
    }
    private fun start(control: IPdfWorker, requestId: String = id, generation: Long = 1,
        maxInput: Int = 10 * 1024 * 1024, maxPages: Int = 500, maxText: Int = 2 * 1024 * 1024,
        maxDecompressed: Int = 4 * 1024 * 1024, events: IPdfWorkerEvents): Boolean {
        val input = ParcelFileDescriptor.createPipe()
        val output = ParcelFileDescriptor.createPipe()
        return try {
            control.start(requestId, generation, input[0], output[1], maxInput, maxPages, maxText,
                maxDecompressed, events)
            true
        } finally { input.forEach { it.close() }; output.forEach { it.close() } }
    }

    private fun client(type: Class<out android.app.Service>, test: (IPdfWorkerTestClient) -> Unit) {
        val ready = CountDownLatch(1)
        var client: IPdfWorkerTestClient? = null
        val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName, binder: IBinder) {
                client = IPdfWorkerTestClient.Stub.asInterface(binder); ready.countDown()
            }
            override fun onServiceDisconnected(name: ComponentName) = Unit
        }
        assertTrue(context.bindService(Intent(context, type), connection, 1))
        try { assertTrue(ready.await(30, TimeUnit.SECONDS)); test(client!!) }
        finally {
            if (type == PdfWorkerTestClientService::class.java && client?.asBinder()?.isBinderAlive == true) {
                val dead = CountDownLatch(1)
                client!!.asBinder().linkToDeath({ dead.countDown() }, 0)
                try { client!!.terminate() } catch (_: android.os.DeadObjectException) { /* Self-kill can precede the synchronous reply. */ }
                assertTrue("test client did not terminate", dead.await(5, TimeUnit.SECONDS))
            }
            context.unbindService(connection)
        }
    }

    @Test fun exactCallbackCapabilityFromWrongPidCannotControlSession() = connected { worker ->
        val dead = CountDownLatch(1)
        worker.asBinder().linkToDeath({ dead.countDown() }, 0)
        val callback = object : IPdfWorkerEvents.Stub() {
            override fun started(requestId: String, generation: Long) = true
        }
        start(worker, events = callback)
        client(PdfWorkerBarrierService::class.java) { barrier ->
            client(PdfWorkerTestClientService::class.java) { auxiliary ->
                val workerPid = barrier.mainBarrier()
                val otherPid = auxiliary.mainBarrier()
                assertNotEquals(android.os.Process.myPid(), otherPid)
                assertNotEquals(workerPid, otherPid)
                assertFalse(auxiliary.chatInitialized())
                assertFalse(barrier.chatInitialized())
                auxiliary.wrongControls(worker, id, 1, callback)
                // This synchronous main-looper barrier runs after any erroneous queued end.
                assertEquals(workerPid, barrier.mainBarrier())
                assertEquals(1L, dead.count)
            }
            worker.cancel(id, 1, callback)
            assertTrue("correct owner did not terminate worker", dead.await(5, TimeUnit.SECONDS))
        }
    }

    @Test fun actualCallbackBinderDeathTerminatesAcceptedWorker() = connected { worker ->
        val dead = CountDownLatch(1)
        worker.asBinder().linkToDeath({ dead.countDown() }, 0)
        client(PdfWorkerTestClientService::class.java) { auxiliary ->
            val auxDead = CountDownLatch(1)
            auxiliary.asBinder().linkToDeath({ auxDead.countDown() }, 0)
            val input = ParcelFileDescriptor.createPipe(); val output = ParcelFileDescriptor.createPipe()
            try {
                auxiliary.start(worker, id, 1, input[0], output[1])
                client(PdfWorkerBarrierService::class.java) { barrier ->
                    assertNotEquals(barrier.mainBarrier(), auxiliary.mainBarrier())
                    assertFalse(barrier.chatInitialized())
                    try { auxiliary.terminate() } catch (_: android.os.DeadObjectException) { /* Confirm death below. */ }
                    assertTrue("callback process death missing", auxDead.await(5, TimeUnit.SECONDS))
                    assertTrue("callback Binder death did not terminate worker", dead.await(5, TimeUnit.SECONDS))
                }
            } finally { input.forEach { it.close() }; output.forEach { it.close() } }
        }
    }

    @Test fun invalidStartDoesNotInvokeCallbackOrReserveSession() = connected { control ->
        var calls = 0
        val callback = object : IPdfWorkerEvents.Stub() {
            override fun started(requestId: String, generation: Long): Boolean { calls++; return false }
        }
        start(control, requestId = "bad", events = callback)
        start(control, maxPages = 0, events = callback)
        start(control, maxInput = 10 * 1024 * 1024 + 1, events = callback)
        start(control, generation = -1, events = callback)
        assertEquals(0, calls)
    }

    @Test fun callbackBinderIsRequiredForControlAndStaleGenerationCannotEndSession() = connected { control ->
        val latch = CountDownLatch(1)
        val callback = object : IPdfWorkerEvents.Stub() {
            override fun started(requestId: String, generation: Long): Boolean { latch.countDown(); return true }
        }
        start(control, events = callback)
        assertTrue(latch.await(3, TimeUnit.SECONDS))
        control.cancel(id, 2, callback)
        control.acknowledge("123e4567-e89b-12d3-a456-426614174999", 1, callback)
        // A second start must remain rejected while the original session is active.
        val second = object : IPdfWorkerEvents.Stub() {
            override fun started(requestId: String, generation: Long): Boolean { fail("accepted a second start"); return false }
        }
        start(control, generation = 2, events = second)
        control.cancel(id, 1, second) // Same UID/PID but wrong session binder.
        start(control, generation = 3, events = second)
        val dead = CountDownLatch(1)
        control.asBinder().linkToDeath({ dead.countDown() }, 0)
        control.cancel(id, 1, callback)
        assertTrue(dead.await(5, TimeUnit.SECONDS))
    }
}
