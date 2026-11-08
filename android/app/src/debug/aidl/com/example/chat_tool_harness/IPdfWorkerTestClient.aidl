package com.example.chat_tool_harness;
import android.os.ParcelFileDescriptor;
import com.example.chat_tool_harness.IPdfWorker;
import com.example.chat_tool_harness.IPdfWorkerEvents;
interface IPdfWorkerTestClient {
    int mainBarrier();
    boolean chatInitialized();
    void wrongControls(IPdfWorker control, String id, long generation, IPdfWorkerEvents events);
    void start(IPdfWorker control, String id, long generation, in ParcelFileDescriptor inputFd, in ParcelFileDescriptor outputFd);
    void terminate();
}
