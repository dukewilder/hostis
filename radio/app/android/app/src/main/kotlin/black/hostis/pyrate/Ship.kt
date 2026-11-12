package black.hostis.pyrate

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.MediaRecorder
import android.os.Build
import androidx.core.content.FileProvider
import java.io.File
import java.net.Inet4Address
import java.net.NetworkInterface
import java.net.ServerSocket
import kotlin.concurrent.thread

/** Recording, sharing, passing the app on, and the plain disguise. */
object Ship {
    private var recorder: MediaRecorder? = null
    private var recPath: String? = null
    private var recStart = 0L

    fun recordStart(c: Context, bitrate: Int = 128_000): String {
        recordStop()
        val f = File(c.cacheDir, "rec-${System.currentTimeMillis()}.m4a")
        val r = if (Build.VERSION.SDK_INT >= 31) MediaRecorder(c) else @Suppress("DEPRECATION") MediaRecorder()
        r.setAudioSource(MediaRecorder.AudioSource.MIC)
        r.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
        r.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
        r.setAudioSamplingRate(48_000)
        r.setAudioEncodingBitRate(bitrate)
        r.setAudioChannels(1)
        r.setOutputFile(f.path)
        r.prepare()
        r.start()
        recorder = r
        recPath = f.path
        recStart = System.currentTimeMillis()
        return f.path
    }

    private var player: android.media.MediaPlayer? = null

    /** Plays a file once, then removes it: how a keeper hears a hail. */
    fun playOnce(path: String) {
        player?.release()
        player = android.media.MediaPlayer().apply {
            setDataSource(path)
            setOnCompletionListener { it.release(); player = null; java.io.File(path).delete() }
            prepare()
            start()
        }
    }

    fun recordStop(): Map<String, Any?> {
        val r = recorder ?: return mapOf("path" to null)
        try { r.stop() } catch (e: Exception) {}
        r.release()
        recorder = null
        return mapOf("path" to recPath, "ms" to (System.currentTimeMillis() - recStart))
    }

    private fun shareUri(c: Context, f: File) = FileProvider.getUriForFile(c, "${c.packageName}.updates", f)

    fun shareText(c: Context, text: String) {
        c.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), null)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    fun shareFile(c: Context, path: String, mime: String) {
        val src = File(path)
        val dir = File(c.cacheDir, "share").apply { mkdirs() }
        val f = if (src.parentFile == dir) src else File(dir, src.name).also { src.copyTo(it, true) }
        val i = Intent(Intent.ACTION_SEND).setType(mime).putExtra(Intent.EXTRA_STREAM, shareUri(c, f)).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        c.startActivity(Intent.createChooser(i, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    private fun apk(c: Context): File {
        val dir = File(c.cacheDir, "share").apply { mkdirs() }
        val f = File(dir, "Pyrate Radio.apk")
        File(c.applicationInfo.sourceDir).copyTo(f, true)
        return f
    }

    /** Pass it on through the share sheet: Quick Share and Bluetooth work with no internet. */
    fun shareApp(c: Context) = shareFile(c, apk(c).path, "application/vnd.android.package-archive")

    private var server: ServerSocket? = null

    /**
     * Pass it on by QR: this phone serves the app over the local network while the screen is
     * open, and for fifteen minutes at most. It listens only on its own local address, answers
     * only under a path nobody can guess (so a scan of the room's Wi-Fi finds nothing that says
     * Pyrate Radio), serves a few at a time, and drops anyone slow to ask.
     */
    fun passStart(c: Context): String? {
        passStop()
        val file = apk(c)
        val ip = NetworkInterface.getNetworkInterfaces().toList().filter { it.isUp && !it.isLoopback }
            .flatMap { it.inetAddresses.toList() }.firstOrNull { it is Inet4Address && it.isSiteLocalAddress } ?: return null
        val token = ByteArray(9).also { java.security.SecureRandom().nextBytes(it) }.joinToString("") { "%02x".format(it) }
        val s = ServerSocket(0, 8, ip)
        server = s
        val open = java.util.concurrent.atomic.AtomicInteger(0)
        val ends = System.currentTimeMillis() + 15 * 60_000
        // Closed at fifteen minutes even if nobody comes.
        thread(isDaemon = true) {
            try { Thread.sleep(15 * 60_000L) } catch (e: Exception) {}
            if (server === s) passStop()
        }
        thread(isDaemon = true) {
            while (!s.isClosed) {
                val sock = try { s.accept() } catch (e: Exception) { break }
                if (System.currentTimeMillis() > ends) {
                    try { sock.close() } catch (e: Exception) {}
                    passStop()
                    break
                }
                if (open.incrementAndGet() > 4) {
                    open.decrementAndGet()
                    try { sock.close() } catch (e: Exception) {}
                    continue
                }
                thread(isDaemon = true) {
                    try {
                        sock.use { k ->
                            k.soTimeout = 15_000
                            // The request line only, and no more than 2 KB of it: a line
                            // with no end cannot fill the phone's memory.
                            val inp = k.getInputStream()
                            val sb = StringBuilder()
                            while (sb.length < 2048) {
                                val b = inp.read()
                                if (b < 0 || b == '\n'.code) break
                                sb.append(b.toChar())
                            }
                            val line = sb.toString().trimEnd('\r')
                            val path = line.split(' ').getOrNull(1) ?: ""
                            val out = k.getOutputStream()
                            when (path) {
                                "/$token/PyrateRadio.apk" -> {
                                    out.write(("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.android.package-archive\r\nContent-Length: ${file.length()}\r\n" +
                                        "Content-Disposition: attachment; filename=\"Pyrate Radio.apk\"\r\nConnection: close\r\n\r\n").toByteArray())
                                    file.inputStream().use { it.copyTo(out) }
                                }
                                "/$token/" -> {
                                    val page = "<!doctype html><meta name=viewport content='width=device-width'><body style='background:#030303;color:#ede4ce;font-family:serif;text-align:center;padding:3em 1em'>" +
                                        "<h1 style='letter-spacing:.2em'>PYRATE RADIO</h1><p>Passed on by hand. No store, no account.</p>" +
                                        "<p><a style='color:#ede4ce;font-size:1.3em' href='/$token/PyrateRadio.apk'>Take the set</a></p>" +
                                        "<p style='opacity:.6'>Android asks once to allow installs from your browser.</p></body>"
                                    val b = page.toByteArray()
                                    out.write("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: ${b.size}\r\nConnection: close\r\n\r\n".toByteArray())
                                    out.write(b)
                                }
                                else -> out.write("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".toByteArray())
                            }
                            out.flush()
                        }
                    } catch (e: Exception) {
                    } finally {
                        open.decrementAndGet()
                    }
                }
            }
        }
        return "http://${ip.hostAddress}:${s.localPort}/$token/"
    }

    fun passStop() {
        try { server?.close() } catch (e: Exception) {}
        server = null
    }

    /** The plain disguise: a plain icon named Radio in place of the skull. */
    fun setPlain(c: Context, plain: Boolean) {
        val pm = c.packageManager
        val on = PackageManager.COMPONENT_ENABLED_STATE_ENABLED
        val off = PackageManager.COMPONENT_ENABLED_STATE_DISABLED
        pm.setComponentEnabledSetting(ComponentName(c, "black.hostis.pyrate.Plain"), if (plain) on else off, PackageManager.DONT_KILL_APP)
        pm.setComponentEnabledSetting(ComponentName(c, "black.hostis.pyrate.Skull"), if (plain) off else on, PackageManager.DONT_KILL_APP)
    }

    fun isPlain(c: Context) = c.packageManager.getComponentEnabledSetting(ComponentName(c, "black.hostis.pyrate.Plain")) == PackageManager.COMPONENT_ENABLED_STATE_ENABLED
}
