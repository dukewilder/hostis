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
 * Whether this app's own traffic goes through a VPN: its active network carries TRANSPORT_VPN.
 * The active network is the app's own, so a VPN that leaves this app out counts as none.
 */
object Vpn {
    fun on(c: Context): Boolean {
        val cm = c.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        return cm.getNetworkCapabilities(cm.activeNetwork)?.hasTransport(NetworkCapabilities.TRANSPORT_VPN) == true
    }

    /** Whether the radio goes through its own Tor tunnel. */
    fun tor(): Boolean = try { Native.cmd("cmd" to "tor").optString("url").isNotEmpty() } catch (e: Throwable) { false }
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
    /** Stopped for want of a VPN (null until first looked at). The keeper still wants it on. */
    @Volatile private var paused: Boolean? = null
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
        val gb = intent?.getIntExtra("gb", -1)?.takeIf { it > 0 } ?: Hold.int(this, "lantern_gb", 1)
        val plain = Hold.flag(this, "plain_lock")
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
        if (paused != true) hold()
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

    private fun hold() {
        if (cpu == null) {
            cpu = (getSystemService(Context.POWER_SERVICE) as PowerManager).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "pyrate:lantern").apply { setReferenceCounted(false); acquire() }
        }
        if (wifi == null) {
            @Suppress("DEPRECATION")
            wifi = (applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager).createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "pyrate:lantern").apply { setReferenceCounted(false); acquire() }
        }
    }

    private fun letGo() {
        cpu?.let { if (it.isHeld) it.release() }
        wifi?.let { if (it.isHeld) it.release() }
        cpu = null
        wifi = null
    }

    /**
     * A spare phone is on all day for strangers to reach, so it runs only while the app's own
     * traffic goes through a VPN or the radio goes through Tor. When neither is there it stops at
     * once: no extra share, no door, the phone let sleep. It starts again by itself when the VPN
     * is back, and says so each time.
     */
    private fun gate() {
        val (charging, unmetered) = Conditions.read(this)
        val vpn = Vpn.on(this)
        Thread {
            val ok = vpn || Vpn.tor()
            synchronized(this) {
                val was = paused
                if (!ok) {
                    // Once, as it stops: what the app does while its screen is open is an
                    // ordinary listener's, and is left to the app.
                    if (was != true) {
                        paused = true
                        try {
                            Native.cmd("cmd" to "ballast_ok", "value" to false, "unmetered" to unmetered)
                            Native.cmd("cmd" to "ballast_size", "gb" to 0)
                            Native.cmd("cmd" to "door", "on" to false, "links" to false)
                        } catch (e: Throwable) {}
                        tick.post { letGo() }
                        notice(false, was == null)
                    }
                    return@Thread
                }
                paused = false
                if (was == true) {
                    try { Native.cmd("cmd" to "ballast_size", "gb" to Hold.int(this, "lantern_gb", 1)) } catch (e: Throwable) {}
                    tick.post { hold() }
                    notice(true, false)
                }
                try { Native.cmd("cmd" to "ballast_ok", "value" to (charging && unmetered), "unmetered" to unmetered) } catch (e: Throwable) {}
            }
        }.start()
    }

    /** Stopped for want of a VPN, or running again with it back. */
    private fun notice(back: Boolean, first: Boolean) {
        val plain = Hold.flag(this, "plain_lock")
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("lantern_vpn", "Spare phone and VPN", NotificationManager.IMPORTANCE_DEFAULT))
        val open = PendingIntent.getActivity(this, 0, packageManager.getLaunchIntentForPackage(packageName), PendingIntent.FLAG_IMMUTABLE)
        val text = when {
            back -> "The VPN is back. The spare phone is carrying again."
            first -> "Waiting for a VPN. The spare phone starts when one is on."
            else -> "The VPN went off, so the spare phone stopped. It starts again when the VPN is back."
        }
        val n = NotificationCompat.Builder(this, "lantern_vpn")
            .setSmallIcon(R.drawable.ic_flag)
            .setContentTitle(if (plain) "Radio" else if (back) "Spare phone running" else "Spare phone stopped")
            .setContentText(if (plain) "" else text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(if (plain) "" else text))
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        try { nm.notify(4, n) } catch (e: SecurityException) {}
    }

    override fun onCreate() {
        super.onCreate()
        Native.serviceUp()
    }

    override fun onDestroy() {
        tick.removeCallbacks(every)
        power?.let { try { unregisterReceiver(it) } catch (e: Exception) {} }
        net?.let { try { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) } catch (e: Exception) {} }
        power = null
        net = null
        // Off the charger gate: carrying stops until the app's own check says otherwise.
        Thread { try { Native.cmd("cmd" to "ballast_ok", "value" to false, "unmetered" to Conditions.read(this).second) } catch (e: Throwable) {} }.start()
        letGo()
        getSystemService(NotificationManager::class.java).cancel(4)
        Native.serviceDown()
        super.onDestroy()
    }

    companion object {
        fun set(context: Context, on: Boolean, gb: Int) {
            Hold.setFlag(context, "lantern_on", on); Hold.setInt(context, "lantern_gb", gb)
            val i = Intent(context, LanternService::class.java).putExtra("gb", gb)
            if (on) {
                if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(i) else context.startService(i)
            } else {
                context.stopService(i)
            }
        }
    }
}
