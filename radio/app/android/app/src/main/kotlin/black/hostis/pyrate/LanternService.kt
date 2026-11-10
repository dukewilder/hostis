package black.hostis.pyrate

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat

/** Whether the phone is on a charger, and whether its network costs nothing. */
object Conditions {
    fun read(c: Context): Pair<Boolean, Boolean> {
        val bm = c.getSystemService(Context.BATTERY_SERVICE) as android.os.BatteryManager
        val cm = c.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val caps = cm.getNetworkCapabilities(cm.activeNetwork)
        return bm.isCharging to (caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true)
    }
}

/**
 * The drawer Lantern: an old phone on a charger keeps the node running with the screen off,
 * carrying its share of the fleet and relaying for others. It holds the processor and the
 * Wi-Fi awake only while it runs, and the keeper switches it off from the app.
 */
class LanternService : Service() {
    private var cpu: PowerManager.WakeLock? = null
    private var wifi: WifiManager.WifiLock? = null

    private val tick = android.os.Handler(android.os.Looper.getMainLooper())
    private var power: BroadcastReceiver? = null
    private var net: ConnectivityManager.NetworkCallback? = null
    private val every = object : Runnable {
        override fun run() {
            gate()
            tick.postDelayed(this, 60_000)
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // A restart after the process died, or after a reboot, comes with no intent and no
        // screen: the size is read back, and the service starts the node itself.
        val gb = intent?.getIntExtra("gb", -1)?.takeIf { it > 0 } ?: getSharedPreferences("pyrate", Context.MODE_PRIVATE).getInt("lantern_gb", 1)
        val plain = getSharedPreferences("pyrate", Context.MODE_PRIVATE).getBoolean("plain_lock", false)
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("lantern", "Lantern", NotificationManager.IMPORTANCE_MIN))
        val open = PendingIntent.getActivity(this, 0, packageManager.getLaunchIntentForPackage(packageName), PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(this, "lantern")
            .setSmallIcon(R.drawable.ic_flag)
            .setContentTitle(if (plain) "Radio" else "Keeping a Lantern")
            .setContentText(if (plain) "" else "Carrying up to $gb GB for the fleet while it charges")
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
        Thread {
            try {
                Native.ensure(applicationContext)
                Native.cmd("cmd" to "ballast_size", "gb" to gb)
            } catch (e: Throwable) {}
            tick.post { watch() }
        }.start()
        return START_STICKY
    }

    /** The charger-and-Wi-Fi gate lives here too, so it holds with the app's screen long gone. */
    private fun watch() {
        if (power == null) {
            power = object : BroadcastReceiver() {
                override fun onReceive(c: Context, i: Intent) = gate()
            }
            val f = IntentFilter().apply {
                addAction(Intent.ACTION_POWER_CONNECTED)
                addAction(Intent.ACTION_POWER_DISCONNECTED)
            }
            ContextCompat.registerReceiver(this, power, f, ContextCompat.RECEIVER_NOT_EXPORTED)
        }
        if (net == null) {
            val cb = object : ConnectivityManager.NetworkCallback() {
                override fun onCapabilitiesChanged(n: Network, c: NetworkCapabilities) { tick.post { gate() } }
                override fun onLost(n: Network) { tick.post { gate() } }
            }
            try {
                getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(cb)
                net = cb
            } catch (e: Exception) {}
        }
        tick.removeCallbacks(every)
        tick.post(every)
    }

    private fun gate() {
        val (charging, unmetered) = Conditions.read(this)
        Thread {
            try { Native.cmd("cmd" to "ballast_ok", "value" to (charging && unmetered), "unmetered" to unmetered) } catch (e: Throwable) {}
        }.start()
    }

    override fun onDestroy() {
        tick.removeCallbacks(every)
        power?.let { try { unregisterReceiver(it) } catch (e: Exception) {} }
        net?.let { try { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) } catch (e: Exception) {} }
        power = null
        net = null
        // Off the charger gate: carrying stops until the app's own check says otherwise.
        Thread { try { Native.cmd("cmd" to "ballast_ok", "value" to false, "unmetered" to Conditions.read(this).second) } catch (e: Throwable) {} }.start()
        cpu?.let { if (it.isHeld) it.release() }
        wifi?.let { if (it.isHeld) it.release() }
        cpu = null
        wifi = null
        super.onDestroy()
    }

    companion object {
        fun set(context: Context, on: Boolean, gb: Int) {
            context.getSharedPreferences("pyrate", Context.MODE_PRIVATE).edit().putBoolean("lantern_on", on).putInt("lantern_gb", gb).apply()
            val i = Intent(context, LanternService::class.java).putExtra("gb", gb)
            if (on) {
                if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(i) else context.startService(i)
            } else {
                context.stopService(i)
            }
        }
    }
}
