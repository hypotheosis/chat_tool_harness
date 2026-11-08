package com.example.chat_tool_harness

import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.security.KeyStore
import java.util.UUID
import org.xmlpull.v1.XmlPullParser

class CredentialStoreTest {
    private fun fixture(test: (CredentialStore, File, String) -> Unit) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alias = "task12.test.${UUID.randomUUID()}"
        val directory = File(context.noBackupFilesDir, alias).apply { mkdirs() }
        try { test(CredentialStore(context, alias, directory), directory, alias) }
        finally {
            KeyStore.getInstance("AndroidKeyStore").apply { load(null); deleteEntry(alias) }
            directory.deleteRecursively()
        }
    }
    @Test fun credential_replace_delete_invalidate() = fixture { store, root, _ ->
        val reference = UUID.randomUUID().toString()
        val other = UUID.randomUUID().toString()
        val secret = "test-only-token".toByteArray()
        store.put(reference, secret)
        val first = File(root, reference).readBytes()
        assertFalse(String(first).contains("test-only-token"))
        store.put(reference, secret)
        val second = File(root, reference).readBytes()
        assertFalse(first.copyOfRange(0, 12).contentEquals(second.copyOfRange(0, 12)))
        assertArrayEquals(secret, store.resolve(reference))
        store.put(other, "other".toByteArray())
        assertArrayEquals("other".toByteArray(), store.resolve(other))
        File(root, other).writeBytes(second)
        try { store.resolve(other); fail("reference must be authenticated") } catch (_: Exception) { }
        store.delete(reference)
        try { store.resolve(reference); fail("deleted value") } catch (_: Exception) { }
    }
    @Test fun keystore_failure_requires_reentry() = fixture { store, root, alias ->
        val reference = UUID.randomUUID().toString()
        store.put(reference, "only-test".toByteArray())
        KeyStore.getInstance("AndroidKeyStore").apply { load(null); deleteEntry(alias) }
        try { store.resolve(reference); fail("missing key must fail") } catch (_: Exception) { }
        try { store.put(reference, "replacement".toByteArray()); fail("no silent key rotation") } catch (_: Exception) { }
        assertEquals(1, root.listFiles()!!.size)
        store.delete(reference)
        store.put(reference, "reentered".toByteArray())
        assertArrayEquals("reentered".toByteArray(), store.resolve(reference))
    }
    @Test fun backup_excludes_all_private_user_data() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        for ((resource, expected) in listOf(R.xml.backup_rules to 1, R.xml.data_extraction_rules to 2)) {
            val counts = mutableMapOf<String, Int>()
            context.resources.getXml(resource).use { parser ->
                while (parser.next() != XmlPullParser.END_DOCUMENT) {
                    if (parser.eventType == XmlPullParser.START_TAG && parser.name == "exclude") {
                        assertEquals(".", parser.getAttributeValue(null, "path"))
                        val domain = parser.getAttributeValue(null, "domain")
                        counts[domain] = (counts[domain] ?: 0) + 1
                    }
                }
            }
            for (domain in listOf("root", "file", "database", "sharedpref", "external", "device_root", "device_file", "device_database", "device_sharedpref")) assertEquals(expected, counts[domain])
        }
        assertEquals(0, context.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_ALLOW_BACKUP)
    }
}
