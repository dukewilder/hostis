package black.hostis.pyrate

import android.content.Context
import org.json.JSONObject
import java.io.File

/** The node, reached from Android's own side. The app's screen reaches it through Dart. */
object Native {
    init {
        System.loadLibrary("pyrate_ffi")
    }

    @JvmStatic external fun setReleaseKeys(keys: String)
    @JvmStatic external fun start(dir: String, boot: String): Int
    @JvmStatic external fun call(req: String): String
    @JvmStatic external fun art(hash: String): ByteArray?

    @Volatile private var started = false

    /** Starts the node if the app's screen has not already. Safe to call from anywhere. */
    fun ensure(ctx: Context) {
        if (started) return
        synchronized(this) {
            if (started) return
            MainActivity.initNative(ctx.applicationContext)
            fun asset(name: String) = try {
                ctx.assets.open("flutter_assets/assets/$name").bufferedReader().readText()
            } catch (e: Exception) { "" }
            setReleaseKeys(asset("release_keys.txt"))
            start(File(ctx.filesDir, "node").path, asset("bootstrap.txt").trim())
            started = true
        }
    }

    fun cmd(vararg pairs: Pair<String, Any?>): JSONObject {
        val req = JSONObject()
        for ((k, v) in pairs) req.put(k, v)
        return try { JSONObject(call(req.toString())) } catch (e: Exception) { JSONObject() }
    }

    fun list(cmd: String): org.json.JSONArray = try {
        org.json.JSONArray(call(JSONObject().put("cmd", cmd).toString()))
    } catch (e: Exception) { org.json.JSONArray() }

    fun bitmap(hash: String?): android.graphics.Bitmap? {
        if (hash.isNullOrEmpty()) return null
        val b = art(hash) ?: return null
        val bb = java.nio.ByteBuffer.wrap(b)
        val w = bb.int
        val h = bb.int
        if (w <= 0 || h <= 0 || b.size < 8 + w * h * 4) return null
        val px = IntArray(w * h)
        for (i in 0 until w * h) {
            val o = 8 + i * 4
            val r = b[o].toInt() and 0xff
            val g = b[o + 1].toInt() and 0xff
            val bl = b[o + 2].toInt() and 0xff
            px[i] = (0xff shl 24) or (r shl 16) or (g shl 8) or bl
        }
        return android.graphics.Bitmap.createBitmap(px, w, h, android.graphics.Bitmap.Config.ARGB_8888)
    }
}
