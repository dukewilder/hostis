package black.hostis.pyrate

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    companion object {
        init {
            System.loadLibrary("pyrate_ffi")
        }

        @JvmStatic
        external fun initNative(context: Context)

        @JvmStatic
        external fun setStartMuted(muted: Boolean)
    }

    private var pendingLink: String? = null

    /** A station link (pyrate://station/<id>, hostis.black/radio/#<id>, or hostis.black/pyrateradio/listen/#<id>) or a bell's tap. */
    private fun takeLink(i: Intent?) {
        if (i == null) return
        i.getStringExtra("station")?.let { if (i.action == "tune") { pendingLink = it; return } }
        val u = i.data ?: return
        val id = when (u.scheme) {
            "pyrate" -> u.lastPathSegment
            else -> u.fragment ?: u.getQueryParameter("s")
        }
        if (id != null && id.matches(Regex("[0-9a-f]{64}"))) pendingLink = id
        // A flock's link: this phone joins the flock and plays in step with its host.
        u.fragment?.takeIf { it.startsWith("flock=") || it.startsWith("pocket=") }?.let { pendingLink = it }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        takeLink(intent)
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        takeLink(intent)
        initNative(applicationContext)
        setStartMuted(intent?.getBooleanExtra("mute", false) == true)
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "pyrate/service").setMethodCallHandler { call, result ->
            when (call.method) {
                "start" -> {
                    val i = Intent(this, PlaybackService::class.java)
                        .putExtra("title", call.argument<String>("title") ?: "Pyrate Radio")
                    try {
                        if (Build.VERSION.SDK_INT >= 26) startForegroundService(i) else startService(i)
                        result.success(null)
                    } catch (e: Exception) {
                        android.util.Log.w("pyrate", "service not started: $e")
                        result.success(null)
                    }
                }
                "mic" -> {
                    if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                        result.success(true)
                    } else {
                        requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 2)
                        result.success(false)
                    }
                }
                "secure" -> {
                    // While the 24 words or a PIN are on screen: no screenshots, no recents
                    // thumbnail, no screen recording.
                    val flag = android.view.WindowManager.LayoutParams.FLAG_SECURE
                    if (call.argument<Boolean>("on") == true) window.addFlags(flag) else window.clearFlags(flag)
                    result.success(null)
                }
                "live" -> {
                    val i = Intent(this, PlaybackService::class.java)
                        .putExtra("title", "ON AIR")
                        .putExtra("live", call.argument<Boolean>("on") == true)
                    try {
                        if (Build.VERSION.SDK_INT >= 26) startForegroundService(i) else startService(i)
                    } catch (e: Exception) {
                        android.util.Log.w("pyrate", "service not started: $e")
                    }
                    result.success(null)
                }
                "conditions" -> {
                    val bm = getSystemService(Context.BATTERY_SERVICE) as android.os.BatteryManager
                    val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager
                    val caps = cm.getNetworkCapabilities(cm.activeNetwork)
                    val unmetered = caps?.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true
                    result.success(mapOf("charging" to bm.isCharging, "unmetered" to unmetered))
                }
                "install" -> {
                    val f = java.io.File(call.argument<String>("path") ?: "")
                    val uri = androidx.core.content.FileProvider.getUriForFile(this, "$packageName.updates", f)
                    startActivity(Intent(Intent.ACTION_VIEW)
                        .setDataAndType(uri, "application/vnd.android.package-archive")
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK))
                    result.success(null)
                }
                "stop" -> {
                    try { startService(Intent(this, PlaybackService::class.java).setAction("stop")) } catch (e: Exception) {
                        stopService(Intent(this, PlaybackService::class.java))
                    }
                    result.success(null)
                }
                "take_link" -> { result.success(pendingLink); pendingLink = null }
                "bell_set" -> {
                    Bells.setBell(this, call.argument<Int>("id") ?: 0, (call.argument<Number>("at") ?: 0).toLong(),
                        call.argument<String>("title") ?: "", call.argument<String>("station") ?: "", call.argument<String>("name") ?: "")
                    result.success(null)
                }
                "bell_cancel" -> { Bells.cancelBell(this, call.argument<Int>("id") ?: 0); result.success(null) }
                "bells" -> result.success(Bells.bells(this))
                "wake_set" -> {
                    Bells.setWake(this, call.argument<Boolean>("on") == true, call.argument<Int>("hour") ?: 7, call.argument<Int>("minute") ?: 0,
                        call.argument<String>("station"), call.argument<String>("name"))
                    result.success(null)
                }
                "wake_get" -> result.success(Bells.wake(this).toString())
                "exact_ok" -> {
                    val am = getSystemService(Context.ALARM_SERVICE) as android.app.AlarmManager
                    result.success(Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms())
                }
                "exact_settings" -> {
                    if (Build.VERSION.SDK_INT >= 31) {
                        try { startActivity(Intent(android.provider.Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, android.net.Uri.parse("package:$packageName"))) } catch (e: Exception) {}
                    }
                    result.success(null)
                }
                "record_start" -> {
                    if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                        requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 2)
                        result.success(null)
                    } else {
                        try { result.success(Ship.recordStart(this, call.argument<Int>("bitrate") ?: 128_000)) } catch (e: Exception) { result.success(null) }
                    }
                }
                "record_stop" -> result.success(Ship.recordStop())
                "speak" -> Voice.speakToFile(this, call.argument<String>("text") ?: "", call.argument<String>("path") ?: "",
                    (call.argument<Double>("rate") ?: 1.0).toFloat(), (call.argument<Double>("pitch") ?: 1.0).toFloat()) { ok -> result.success(ok) }
                "play_file" -> { try { Ship.playOnce(call.argument<String>("path") ?: ""); result.success(true) } catch (e: Exception) { result.success(false) } }
                "lantern" -> { LanternService.set(this, call.argument<Boolean>("on") == true, call.argument<Int>("gb") ?: 1); result.success(null) }
                "share_text" -> { Ship.shareText(this, call.argument<String>("text") ?: ""); result.success(null) }
                "share_file" -> { Ship.shareFile(this, call.argument<String>("path") ?: "", call.argument<String>("mime") ?: "audio/ogg"); result.success(null) }
                "share_app" -> { try { Ship.shareApp(this) } catch (e: Exception) {}; result.success(null) }
                "pass_start" -> result.success(try { Ship.passStart(this) } catch (e: Exception) { null })
                "pass_stop" -> { Ship.passStop(); result.success(null) }
                "open_uri" -> {
                    try {
                        startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse(call.argument<String>("uri") ?: "")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        result.success(true)
                    } catch (e: Exception) { result.success(false) }
                }
                "plain_set" -> { Ship.setPlain(this, call.argument<Boolean>("value") == true); result.success(null) }
                "plain_get" -> result.success(Ship.isPlain(this))
                "plain_lock_set" -> {
                    getSharedPreferences("pyrate", Context.MODE_PRIVATE).edit().putBoolean("plain_lock", call.argument<Boolean>("value") == true).apply()
                    result.success(null)
                }
                "plain_lock_get" -> result.success(getSharedPreferences("pyrate", Context.MODE_PRIVATE).getBoolean("plain_lock", false))
                "remember" -> {
                    getSharedPreferences("pyrate", Context.MODE_PRIVATE).edit().putString("last", call.argument<String>("station")).apply()
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
    }
}
