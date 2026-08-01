package org.mlm.adblock

import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Thin JNI wrapper around the brave/adblock-rust Engine.
 *
 * Thread-safe: the underlying engine is guarded by a Mutex on the Rust side
 * (the "single-thread" feature of the adblock crate is intentionally left off,
 * since WebView request interception happens on multiple threads).
 */
class AdblockEngine private constructor(private var nativePtr: Long) : AutoCloseable {

    private val closed = AtomicBoolean(false)

    companion object {
        init {
            System.loadLibrary("adblock_ffi")
        }

        fun create(): AdblockEngine = AdblockEngine(nativeCreate())

        @JvmStatic
        private external fun nativeCreate(): Long

        @JvmStatic
        private external fun nativeDestroy(ptr: Long)

        @JvmStatic
        private external fun nativeLoadFilterList(ptr: Long, rules: String): Boolean

        @JvmStatic
        private external fun nativeCheckNetworkUrls(
            ptr: Long,
            url: String,
            sourceUrl: String,
            requestType: String
        ): Boolean

        @JvmStatic
        private external fun nativeSerialize(ptr: Long): ByteArray?

        @JvmStatic
        private external fun nativeDeserialize(ptr: Long, data: ByteArray): Boolean
    }

    /**
     * Loads filter list text (one or more lists joined with `\n`).
     * Replaces any previously loaded filters.
     */
    fun loadFilterList(rulesText: String): Boolean {
        check(!closed.get()) { "AdblockEngine is closed" }
        return nativeLoadFilterList(nativePtr, rulesText)
    }

    /**
     * @param requestType adblock-rust type: script|image|stylesheet|xmlhttprequest|
     *   media|font|websocket|other|document|subdocument|...
     */
    fun shouldBlock(url: String, sourceUrl: String, requestType: String): Boolean {
        if (closed.get() || nativePtr == 0L) return false
        return try {
            nativeCheckNetworkUrls(nativePtr, url, sourceUrl, requestType)
        } catch (_: Throwable) {
            false
        }
    }

    fun serializeTo(file: File): Boolean = try {
        val bytes = nativeSerialize(nativePtr) ?: return false
        file.writeBytes(bytes)
        true
    } catch (_: Exception) {
        false
    }

    fun deserializeFrom(file: File): Boolean = try {
        if (!file.exists()) return false
        nativeDeserialize(nativePtr, file.readBytes())
    } catch (_: Exception) {
        false
    }

    override fun close() {
        if (closed.compareAndSet(false, true) && nativePtr != 0L) {
            nativeDestroy(nativePtr)
            nativePtr = 0L
        }
    }
}
