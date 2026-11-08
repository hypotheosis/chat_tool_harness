package com.example.chat_tool_harness

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.io.FileNotFoundException
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Ciphertext only; a lost key is never silently replaced while ciphertext exists. */
class CredentialStore(context: Context, private val alias: String = "agent.credentials.v1",
                      directory: File = File(context.noBackupFilesDir, "credentials")) {
    private val root = directory.apply { mkdirs() }
    private val keys = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    private fun file(reference: String): AtomicFile {
        require(UUID.fromString(reference).toString() == reference)
        return AtomicFile(File(root, reference))
    }
    private fun key(create: Boolean): SecretKey {
        (keys.getKey(alias, null) as? SecretKey)?.let { return it }
        check(create && root.listFiles().orEmpty().isEmpty())
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true).build())
            generateKey()
        }
    }
    @Synchronized fun put(reference: String, value: ByteArray) {
        val target = file(reference)
        require(value.isNotEmpty() && value.size <= 8192)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key(true))
        cipher.updateAAD(reference.toByteArray(Charsets.UTF_8))
        val encrypted = cipher.doFinal(value)
        val output = target.startWrite()
        try {
            output.write(cipher.iv)
            output.write(encrypted)
            target.finishWrite(output)
        } catch (error: Exception) {
            target.failWrite(output)
            throw error
        }
    }
    /**
     * Null only when nothing is stored under [reference]: no file and no AtomicFile backup.
     * An unreadable file (permissions), bad size, decrypt or keystore errors throw.
     */
    @Synchronized fun resolve(reference: String): ByteArray? {
        val target = file(reference)
        val encrypted = try {
            target.readFully()
        } catch (error: FileNotFoundException) {
            if (target.baseFile.exists() || File(target.baseFile.path + ".bak").exists()) throw error
            return null
        }
        check(encrypted.size in 29..8220)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(false), GCMParameterSpec(128, encrypted.copyOfRange(0, 12)))
        cipher.updateAAD(reference.toByteArray(Charsets.UTF_8))
        return cipher.doFinal(encrypted, 12, encrypted.size - 12)
    }
    /**
     * Describes the stored value without returning it; null if absent. Works on the bytes so
     * no unzeroable copy of the secret is made; only the scheme bytes become a String.
     * - scheme: the first word when the value is "<scheme> <more>", the same rule as Rust's
     *   `net::authorization_value`; otherwise null and Rust adds "Bearer ".
     * - length: UTF-8 byte length of the stored value.
     * - sendable: every byte is visible ASCII or a single space between visible bytes (no
     *   tabs, control bytes, non-ASCII, leading/trailing or consecutive spaces). Stricter
     *   than Rust on purpose: it flags pasted newlines, tabs and spaces.
     */
    @Synchronized fun describe(reference: String): Map<String, Any?>? {
        val value = resolve(reference) ?: return null
        try {
            val space = value.indexOf(SPACE)
            val scheme = if (space > 0 && space < value.size - 1 && isScheme(value, space)) {
                String(value, 0, space, Charsets.US_ASCII)
            } else {
                null
            }
            return mapOf("scheme" to scheme, "length" to value.size, "sendable" to isSendable(value))
        } finally {
            value.fill(0)
        }
    }
    private fun isScheme(value: ByteArray, end: Int): Boolean {
        val first = value[0].toInt()
        if (first !in 'A'.code..'Z'.code && first !in 'a'.code..'z'.code) return false
        for (i in 0 until end) {
            val b = value[i].toInt()
            val ok = b in 'A'.code..'Z'.code || b in 'a'.code..'z'.code || b in '0'.code..'9'.code ||
                (b in 0x21..0x7E && b.toChar() in TCHAR_SYMBOLS)
            if (!ok) return false
        }
        return true
    }
    private fun isSendable(value: ByteArray): Boolean {
        if (value.isEmpty() || value.first() == SPACE || value.last() == SPACE) return false
        for (i in value.indices) {
            val b = value[i]
            if (b == SPACE) {
                if (value[i - 1] == SPACE) return false
            } else if (b.toInt() !in 0x21..0x7E) {
                return false
            }
        }
        return true
    }
    @Synchronized fun delete(reference: String) {
        val target = file(reference)
        target.delete()
        check(!target.baseFile.exists())
    }
    private companion object {
        const val SPACE: Byte = 0x20
        const val TCHAR_SYMBOLS = "!#$%&'*+.^_`|~-"
    }
}
