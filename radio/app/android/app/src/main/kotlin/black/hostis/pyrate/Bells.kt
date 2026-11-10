package black.hostis.pyrate

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import org.json.JSONArray
import org.json.JSONObject
import java.util.Calendar

/**
 * Bells and the morning alarm. A station's clock is worked out on the phone, so the phone
 * knows when a show starts without any server telling it, and rings on its own.
 */
object Bells {
    private fun prefs(c: Context) = c.getSharedPreferences("pyrate", Context.MODE_PRIVATE)
    private fun alarms(c: Context) = c.getSystemService(Context.ALARM_SERVICE) as AlarmManager

    private fun pending(c: Context, code: Int, i: Intent) =
        PendingIntent.getBroadcast(c, code, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    /** The same show on the same station, whatever id an older build gave its bell. */
    private fun same(b: JSONObject, station: String, at: Long) = b.optString("station") == station && b.optLong("at") / 60_000 == at / 60_000

    fun setBell(c: Context, id: Int, at: Long, title: String, station: String, name: String) {
        val list = JSONArray(Hold.getS(c, "bells") ?: "[]")
        val keep = JSONArray()
        for (k in 0 until list.length()) {
            val b = list.getJSONObject(k)
            if (b.optInt("id") != id && same(b, station, at)) {
                alarms(c).cancel(pending(c, b.optInt("id"), Intent(c, AlarmReceiver::class.java).setAction("bell")))
                continue
            }
            if (b.optInt("id") != id && b.optLong("at") > System.currentTimeMillis()) keep.put(b)
        }
        keep.put(JSONObject().put("id", id).put("at", at).put("title", title).put("station", station).put("name", name))
        Hold.putS(c, "bells", keep.toString())
        register(c, id, at, title, station, name)
    }

    private fun register(c: Context, id: Int, at: Long, title: String, station: String, name: String) {
        val i = Intent(c, AlarmReceiver::class.java).setAction("bell")
            .putExtra("id", id).putExtra("title", title).putExtra("station", station).putExtra("name", name)
        // A window of two minutes needs no special permission and is close enough for a show.
        alarms(c).setWindow(AlarmManager.RTC_WAKEUP, at - 60_000, 120_000, pending(c, id, i))
    }

    fun cancelBell(c: Context, id: Int, station: String? = null, at: Long = 0) {
        alarms(c).cancel(pending(c, id, Intent(c, AlarmReceiver::class.java).setAction("bell")))
        val list = JSONArray(Hold.getS(c, "bells") ?: "[]")
        val keep = JSONArray()
        for (k in 0 until list.length()) {
            val b = list.getJSONObject(k)
            if (b.optInt("id") == id) continue
            if (station != null && same(b, station, at)) {
                alarms(c).cancel(pending(c, b.optInt("id"), Intent(c, AlarmReceiver::class.java).setAction("bell")))
                continue
            }
            keep.put(b)
        }
        Hold.putS(c, "bells", keep.toString())
    }

    /** Whether a bell can show at all: notifications on for the app and for its bells. */
    fun canRing(c: Context): Boolean {
        val nm = c.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (!nm.areNotificationsEnabled()) return false
        if (Build.VERSION.SDK_INT >= 26) {
            val ch = nm.getNotificationChannel("bells")
            if (ch != null && ch.importance == NotificationManager.IMPORTANCE_NONE) return false
        }
        return true
    }

    fun bells(c: Context): String = Hold.getS(c, "bells") ?: "[]"

    fun setWake(c: Context, on: Boolean, hour: Int, minute: Int, station: String?, name: String?) {
        prefs(c).edit().putBoolean("wake_on", on).putInt("wake_h", hour).putInt("wake_m", minute).apply()
        Hold.putS(c, "wake_station", station)
        Hold.putS(c, "wake_name", name)
        rescheduleWake(c)
    }

    fun wake(c: Context): JSONObject = JSONObject()
        .put("on", prefs(c).getBoolean("wake_on", false))
        .put("hour", prefs(c).getInt("wake_h", 7))
        .put("minute", prefs(c).getInt("wake_m", 0))
        .put("station", Hold.getS(c, "wake_station"))
        .put("name", Hold.getS(c, "wake_name"))

    /** Sets the next morning's alarm, every day the alarm is on. */
    fun rescheduleWake(c: Context) {
        val p = prefs(c)
        val i = Intent(c, AlarmReceiver::class.java).setAction("wake").putExtra("station", Hold.getS(c, "wake_station"))
        val pi = pending(c, 9_000_001, i)
        alarms(c).cancel(pi)
        if (!p.getBoolean("wake_on", false)) return
        val cal = Calendar.getInstance().apply {
            set(Calendar.HOUR_OF_DAY, p.getInt("wake_h", 7))
            set(Calendar.MINUTE, p.getInt("wake_m", 0))
            set(Calendar.SECOND, 0)
            set(Calendar.MILLISECOND, 0)
            if (timeInMillis <= System.currentTimeMillis() + 5_000) add(Calendar.DAY_OF_YEAR, 1)
        }
        val am = alarms(c)
        val exact = Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()
        if (exact) {
            val show = PendingIntent.getActivity(c, 0, Intent(c, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
            am.setAlarmClock(AlarmManager.AlarmClockInfo(cal.timeInMillis, show), pi)
        } else {
            am.setWindow(AlarmManager.RTC_WAKEUP, cal.timeInMillis, 60_000, pi)
        }
    }

    fun restoreAll(c: Context) {
        val list = JSONArray(Hold.getS(c, "bells") ?: "[]")
        for (k in 0 until list.length()) {
            val b = list.getJSONObject(k)
            if (b.optLong("at") > System.currentTimeMillis()) {
                register(c, b.optInt("id"), b.optLong("at"), b.optString("title"), b.optString("station"), b.optString("name"))
            }
        }
        rescheduleWake(c)
    }

    /** A secret only this app holds, so a tap on its own bell can be told from a link sent by another app. */
    fun bellKey(c: Context): String {
        val p = c.getSharedPreferences("pyrate", Context.MODE_PRIVATE)
        p.getString("bell_key", null)?.let { return it }
        val b = ByteArray(16).also { java.security.SecureRandom().nextBytes(it) }
        val k = b.joinToString("") { "%02x".format(it) }
        p.edit().putString("bell_key", k).apply()
        return k
    }

    fun ring(c: Context, title: String, station: String, name: String, id: Int) {
        val nm = c.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= 26) {
            nm.createNotificationChannel(NotificationChannel("bells", "Bells", NotificationManager.IMPORTANCE_HIGH))
        }
        val open = PendingIntent.getActivity(c, id,
            Intent(c, MainActivity::class.java).setAction("tune").putExtra("station", station).putExtra("key", bellKey(c)).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        // In the plain disguise a bell says only that something is on.
        val plain = c.getSharedPreferences("pyrate", Context.MODE_PRIVATE).getBoolean("plain_lock", false)
        nm.notify(id, NotificationCompat.Builder(c, "bells")
            .setSmallIcon(R.drawable.ic_flag)
            .setContentTitle(if (plain) "Radio" else title)
            .setContentText(if (plain) "Something you asked for is on." else "On $name now. Tap to tune in.")
            .setVisibility(if (plain) NotificationCompat.VISIBILITY_PRIVATE else NotificationCompat.VISIBILITY_PUBLIC)
            .setContentIntent(open)
            .setAutoCancel(true)
            .build())
    }
}

class AlarmReceiver : BroadcastReceiver() {
    override fun onReceive(c: Context, i: Intent) {
        when (i.action) {
            "bell" -> Bells.ring(c, i.getStringExtra("title") ?: "", i.getStringExtra("station") ?: "", i.getStringExtra("name") ?: "", i.getIntExtra("id", 0))
            "wake" -> {
                val s = Intent(c, PlaybackService::class.java).setAction("wake").putExtra("station", i.getStringExtra("station"))
                if (Build.VERSION.SDK_INT >= 26) c.startForegroundService(s) else c.startService(s)
            }
        }
    }
}

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(c: Context, i: Intent) {
        if (i.action != Intent.ACTION_BOOT_COMPLETED) return
        Bells.restoreAll(c)
        // A drawer Lantern comes back on after a power cut, as it was left.
        val p = c.getSharedPreferences("pyrate", Context.MODE_PRIVATE)
        if (p.getBoolean("lantern_on", false)) {
            try { LanternService.set(c, true, p.getInt("lantern_gb", 1)) } catch (e: Exception) {}
        }
    }
}
