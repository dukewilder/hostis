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
        // A bell's tap carries the app's own key and tunes at once; anything else asks first.
        i.getStringExtra("station")?.let {
            if (i.action == "tune" && it.matches(Regex("[0-9a-f]{64}"))) {
                pendingLink = if (i.getStringExtra("key") == Bells.bellKey(this)) "tune=$it" else it
                return
            }
        }
        val u = i.data ?: return
        val id = when (u.scheme) {
            "pyrate" -> u.lastPathSegment
            else -> u.fragment ?: u.getQueryParameter("s")
        }
        if (id != null && id.matches(Regex("[0-9a-f]{64}"))) pendingLink = id
        // A flock's or a pocket's link: joined only once the owner says yes on the dial.
        u.fragment?.takeIf { it.startsWith("flock=") || it.startsWith("pocket=") }?.let { pendingLink = it }
    }

    private val micWait = mutableListOf<(String) -> Unit>()

    /** Asks for the microphone and answers once the owner has: never before. */
    private fun withMic(then: (String) -> Unit) {
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) return then("ok")
        micWait.add(then)
        if (micWait.size == 1) requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 2)
    }

    override fun onRequestPermissionsResult(code: Int, perms: Array<out String>, grants: IntArray) {
        super.onRequestPermissionsResult(code, perms, grants)
        if (code != 2) return
        val state = when {
            grants.firstOrNull() == PackageManager.PERMISSION_GRANTED -> "ok"
            grants.isEmpty() -> "denied"
            // After a refusal the phone still offers the question; after "don't ask again",
            // or a second refusal on newer phones, it does not, and only settings can allow it.
            shouldShowRequestPermissionRationale(Manifest.permission.RECORD_AUDIO) -> "denied"
            else -> "blocked"
        }
        val w = micWait.toList()
        micWait.clear()
        w.forEach { it(state) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        takeLink(intent)
    }

    private fun quiet() = getSharedPreferences("pyrate", Context.MODE_PRIVATE).getBoolean("plain_lock", false)

    /** With the lock screen saying only "Radio", the recents screen shows nothing either. */
    private fun discreet() {
        val flag = android.view.WindowManager.LayoutParams.FLAG_SECURE
        if (quiet()) window.addFlags(flag) else window.clearFlags(flag)
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        discreet()
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
                // "ok", "denied" (it can be asked again) or "blocked" (only the phone's settings can allow it).
                "mic" -> withMic { result.success(it) }
                "app_settings" -> {
                    try {
                        startActivity(Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.fromParts("package", packageName, null)))
                    } catch (e: Exception) {}
                    result.success(null)
                }
                "hold_key" -> { Hold.clearIfScuttled(applicationContext); result.success(Hold.key(applicationContext)) }
                // The key goes first, then the radio stops dead: the process ends with the node.
                "scuttle" -> {
                    try { stopService(Intent(this, PlaybackService::class.java)) } catch (e: Exception) {}
                    try { LanternService.set(this, false, 1) } catch (e: Exception) {}
                    Bells.cancelAll(this)
                    Hold.scuttle(applicationContext)
                    result.success(null)
                    window.decorView.postDelayed({ finishAndRemoveTask(); android.os.Process.killProcess(android.os.Process.myPid()) }, 300)
                }
                "build_code" -> {
                    val info = packageManager.getPackageInfo(packageName, 0)
                    result.success(if (Build.VERSION.SDK_INT >= 28) info.longVersionCode.toInt() else @Suppress("DEPRECATION") info.versionCode)
                }
                "copy_secret" -> {
                    // Marked sensitive, so keyboards and clipboard previews keep it out of their
                    // history and cloud; gone after a minute and a half if nothing replaced it.
                    val text = call.argument<String>("text") ?: ""
                    val cm = getSystemService(Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                    val clip = android.content.ClipData.newPlainText("", text)
                    clip.description.extras = android.os.PersistableBundle().apply { putBoolean("android.content.extra.IS_SENSITIVE", true) }
                    cm.setPrimaryClip(clip)
                    android.os.Handler(mainLooper).postDelayed({
                        try { if (cm.primaryClip?.getItemAt(0)?.text?.toString() == text) cm.clearPrimaryClip() } catch (_: Exception) { }
                    }, 90_000L)
                    result.success(null)
                }
                "signer" -> {
                    val s = packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNING_CERTIFICATES).signingInfo?.apkContentsSigners?.firstOrNull()
                    result.success(s?.let { java.security.MessageDigest.getInstance("SHA-256").digest(it.toByteArray()).joinToString(":") { b -> "%02X".format(b) } })
                }
                "secure" -> {
                    // While the 24 words or a PIN are on screen: no screenshots, no recents
                    // thumbnail, no screen recording.
                    val flag = android.view.WindowManager.LayoutParams.FLAG_SECURE
                    if (call.argument<Boolean>("on") == true) window.addFlags(flag) else if (!quiet()) window.clearFlags(flag)
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
                    val (charging, unmetered) = Conditions.read(this)
                    result.success(mapOf("charging" to charging, "unmetered" to unmetered))
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
                    result.success(Bells.canRing(this))
                }
                "bell_cancel" -> {
                    Bells.cancelBell(this, call.argument<Int>("id") ?: 0, call.argument<String>("station"), (call.argument<Number>("at") ?: 0).toLong())
                    result.success(null)
                }
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
                    discreet()
                    result.success(null)
                }
                "plain_lock_get" -> result.success(getSharedPreferences("pyrate", Context.MODE_PRIVATE).getBoolean("plain_lock", false))
                "remember" -> {
                    Hold.putS(this, "last", call.argument<String>("station"))
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
    }
}
