package com.example.chat_tool_harness;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.os.Binder;
import android.os.IBinder;
import android.os.Parcel;
import android.os.ParcelFileDescriptor;
import android.os.Process;
import android.os.RemoteException;
import androidx.test.InstrumentationRegistry;
import org.junit.Test;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import static org.junit.Assert.*;

/** Normal optimized lib/main.dart APK, without debug classes or app keep rules.
 * Uses the declared AIDL parcel contract because release R8 inlines internal Stubs.
 * Java keeps the probe independent of Kotlin helpers removed from the release app.
 */
public class PdfWorkerEntrypointTest {
    private static final String CONTROL = "com.example.chat_tool_harness.IPdfWorker";
    private static final String EVENTS = "com.example.chat_tool_harness.IPdfWorkerEvents";
    private final Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();

    private static final class Callback extends Binder {
        final String id;
        final CountDownLatch entered = new CountDownLatch(1);
        final CountDownLatch release;
        int pid;
        Callback(String id, CountDownLatch release) { this.id = id; this.release = release; }
        @Override protected boolean onTransact(int code, Parcel data, Parcel reply, int flags) throws RemoteException {
            if (code == INTERFACE_TRANSACTION) { reply.writeString(EVENTS); return true; }
            if (code != FIRST_CALL_TRANSACTION) return super.onTransact(code, data, reply, flags);
            data.enforceInterface(EVENTS);
            assertEquals(id, data.readString()); assertEquals(77L, data.readLong());
            assertEquals(Process.myUid(), getCallingUid());
            pid = getCallingPid(); assertNotEquals(Process.myPid(), pid);
            entered.countDown();
            if (release != null) {
                try { assertTrue(release.await(10, TimeUnit.SECONDS)); }
                catch (InterruptedException e) { Thread.currentThread().interrupt(); throw new AssertionError(e); }
            }
            reply.writeNoException(); reply.writeInt(1); return true;
        }
    }
    private static void start(IBinder worker, String id, ParcelFileDescriptor input,
                              ParcelFileDescriptor output, IBinder events) throws RemoteException {
        Parcel data = Parcel.obtain(), reply = Parcel.obtain();
        try {
            data.writeInterfaceToken(CONTROL); data.writeString(id); data.writeLong(77);
            data.writeInt(1); input.writeToParcel(data, 0);
            data.writeInt(1); output.writeToParcel(data, 0);
            data.writeInt(10 * 1024 * 1024); data.writeInt(500);
            data.writeInt(2 * 1024 * 1024); data.writeInt(4 * 1024 * 1024); data.writeStrongBinder(events);
            assertTrue(worker.transact(IBinder.FIRST_CALL_TRANSACTION, data, reply, 0)); reply.readException();
        } finally { data.recycle(); reply.recycle(); }
    }
    private static void end(IBinder worker, String id, IBinder events, boolean cancel) throws RemoteException {
        Parcel data = Parcel.obtain(), reply = Parcel.obtain();
        try {
            data.writeInterfaceToken(CONTROL); data.writeString(id); data.writeLong(77); data.writeStrongBinder(events);
            assertTrue(worker.transact(IBinder.FIRST_CALL_TRANSACTION + (cancel ? 2 : 1), data, reply, 0)); reply.readException();
        } finally { data.recycle(); reply.recycle(); }
    }
    private final class BoundWorker implements AutoCloseable {
        final CountDownLatch connected = new CountDownLatch(1);
        IBinder worker;
        final ServiceConnection connection = new ServiceConnection() {
            @Override public void onServiceConnected(ComponentName name, IBinder binder) { worker = binder; connected.countDown(); }
            @Override public void onServiceDisconnected(ComponentName name) { }
        };
        BoundWorker() throws Exception {
            Intent intent = new Intent().setComponent(new ComponentName(context.getPackageName(), context.getPackageName() + ".PdfWorkerService"));
            assertTrue(context.bindService(intent, connection, Context.BIND_AUTO_CREATE));
            assertTrue(connected.await(30, TimeUnit.SECONDS));
        }
        @Override public void close() { context.unbindService(connection); }
    }
    private int extract() throws Exception {
        try (BoundWorker bound = new BoundWorker()) {
            IBinder worker = bound.worker;
            String id = UUID.randomUUID().toString(); Callback callback = new Callback(id, null);
            CountDownLatch dead = new CountDownLatch(1); worker.linkToDeath(dead::countDown, 0);
            File file = new File(context.getCacheDir(), UUID.randomUUID() + ".pdf");
            try (InputStream source = InstrumentationRegistry.getInstrumentation().getContext().getAssets().open("two_pages.pdf");
                 FileOutputStream destination = new FileOutputStream(file)) {
                byte[] buffer = new byte[1024]; int count;
                while ((count = source.read(buffer)) >= 0) destination.write(buffer, 0, count);
            }
            ParcelFileDescriptor input = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY);
            ParcelFileDescriptor[] output = ParcelFileDescriptor.createPipe();
            ExecutorService reader = Executors.newSingleThreadExecutor();
            try {
                Future<byte[]> wire = reader.submit(() -> {
                    try (InputStream stream = new ParcelFileDescriptor.AutoCloseInputStream(output[0])) {
                        ByteArrayOutputStream bytes = new ByteArrayOutputStream(); byte[] buffer = new byte[1024]; int count;
                        while ((count = stream.read(buffer)) >= 0) {
                            assertTrue("small fixture response grew unexpectedly", bytes.size() + count <= 16 * 1024);
                            bytes.write(buffer, 0, count);
                        }
                        return bytes.toByteArray();
                    }
                });
                start(worker, id, input, output[1], callback); input.close(); output[1].close();
                String response = new String(wire.get(60, TimeUnit.SECONDS), StandardCharsets.UTF_8);
                assertTrue(response.contains("First")); assertTrue(response.contains("Second"));
                assertTrue(response.contains("\"kind\":\"done\",\"complete\":true,\"page_count\":2"));
                assertEquals(0L, callback.entered.getCount());
                end(worker, id, callback, false);
                assertTrue("acknowledged worker death missing", dead.await(5, TimeUnit.SECONDS));
                return callback.pid;
            } finally { input.close(); output[0].close(); output[1].close(); reader.shutdownNow(); file.delete(); }
        }
    }
    @Test public void normalEntrypointExtractionAndAcceptedCancelThenExplicitRetry() throws Exception {
        int mainPid = Process.myPid();
        int firstPid = extract();
        try (BoundWorker bound = new BoundWorker()) {
            IBinder worker = bound.worker;
            CountDownLatch dead = new CountDownLatch(1); worker.linkToDeath(dead::countDown, 0);
            CountDownLatch release = new CountDownLatch(1);
            String id = UUID.randomUUID().toString(); Callback callback = new Callback(id, release);
            ParcelFileDescriptor[] input = ParcelFileDescriptor.createPipe(), output = ParcelFileDescriptor.createPipe();
            ExecutorService executor = Executors.newSingleThreadExecutor();
            try {
                Future<?> transaction = executor.submit(() -> {
                    try { start(worker, id, input[0], output[1], callback); }
                    catch (android.os.DeadObjectException expected) { /* Self-kill may precede the start reply. */ }
                    catch (RemoteException e) { throw new AssertionError(e); }
                });
                assertTrue(callback.entered.await(30, TimeUnit.SECONDS));
                end(worker, id, callback, true);
                assertTrue("accepted worker cancel did not confirm death", dead.await(5, TimeUnit.SECONDS));
                release.countDown(); transaction.get(5, TimeUnit.SECONDS);
            } finally { release.countDown(); executor.shutdownNow(); for (ParcelFileDescriptor fd : input) fd.close(); for (ParcelFileDescriptor fd : output) fd.close(); }
        }
        int finalPid = extract(); // Explicit request only after confirmed worker death.
        assertEquals(mainPid, Process.myPid());
        System.out.println("PDF_NORMAL_ENTRYPOINT_OK main=" + mainPid + " firstWorker=" + firstPid + " finalWorker=" + finalPid);
    }
}
