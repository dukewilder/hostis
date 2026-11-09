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

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
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
                    stopService(Intent(this, PlaybackService::class.java))
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
    }
}
