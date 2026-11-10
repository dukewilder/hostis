package black.hostis.pyrate

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

/**
 * The drawer Lantern: an old phone on a charger keeps the node running with the screen off,
 * carrying its share of the fleet and relaying for others. It holds the processor and the
 * Wi-Fi awake only while it runs, and the keeper switches it off from the app.
 */
class LanternService : Service() {
    private var cpu: PowerManager.WakeLock? = null
    private var wifi: WifiManager.WifiLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val gb = intent?.getIntExtra("gb", 1) ?: 1
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("lantern", "Lantern", NotificationManager.IMPORTANCE_MIN))
        val open = PendingIntent.getActivity(this, 0, packageManager.getLaunchIntentForPackage(packageName), PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(this, "lantern")
            .setSmallIcon(R.drawable.ic_flag)
            .setContentTitle("Keeping a Lantern")
            .setContentText("Carrying up to $gb GB for the fleet while it charges")
            .setContentIntent(open)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(3, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(3, n)
        }
        if (cpu == null) {
            cpu = (getSystemService(Context.POWER_SERVICE) as PowerManager).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "pyrate:lantern").apply { setReferenceCounted(false); acquire() }
        }
        if (wifi == null) {
            @Suppress("DEPRECATION")
            wifi = (applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager).createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "pyrate:lantern").apply { setReferenceCounted(false); acquire() }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        cpu?.let { if (it.isHeld) it.release() }
        wifi?.let { if (it.isHeld) it.release() }
        cpu = null
        wifi = null
        super.onDestroy()
    }

    companion object {
        fun set(context: Context, on: Boolean, gb: Int) {
            val i = Intent(context, LanternService::class.java).putExtra("gb", gb)
            if (on) {
                if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(i) else context.startService(i)
            } else {
                context.stopService(i)
            }
        }
    }
}
