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

    fun setBell(c: Context, id: Int, at: Long, title: String, station: String, name: String) {
        val list = JSONArray(prefs(c).getString("bells", "[]"))
        val keep = JSONArray()
        for (k in 0 until list.length()) {
            val b = list.getJSONObject(k)
            if (b.optInt("id") != id && b.optLong("at") > System.currentTimeMillis()) keep.put(b)
        }
        keep.put(JSONObject().put("id", id).put("at", at).put("title", title).put("station", station).put("name", name))
        prefs(c).edit().putString("bells", keep.toString()).apply()
        register(c, id, at, title, station, name)
    }

    private fun register(c: Context, id: Int, at: Long, title: String, station: String, name: String) {
        val i = Intent(c, AlarmReceiver::class.java).setAction("bell")
            .putExtra("id", id).putExtra("title", title).putExtra("station", station).putExtra("name", name)
        // A window of two minutes needs no special permission and is close enough for a show.
        alarms(c).setWindow(AlarmManager.RTC_WAKEUP, at - 60_000, 120_000, pending(c, id, i))
    }

    fun cancelBell(c: Context, id: Int) {
        alarms(c).cancel(pending(c, id, Intent(c, AlarmReceiver::class.java).setAction("bell")))
        val list = JSONArray(prefs(c).getString("bells", "[]"))
        val keep = JSONArray()
        for (k in 0 until list.length()) if (list.getJSONObject(k).optInt("id") != id) keep.put(list.getJSONObject(k))
        prefs(c).edit().putString("bells", keep.toString()).apply()
    }

    fun bells(c: Context): String = prefs(c).getString("bells", "[]") ?: "[]"

    fun setWake(c: Context, on: Boolean, hour: Int, minute: Int, station: String?, name: String?) {
        prefs(c).edit().putBoolean("wake_on", on).putInt("wake_h", hour).putInt("wake_m", minute)
            .putString("wake_station", station).putString("wake_name", name).apply()
        rescheduleWake(c)
    }

    fun wake(c: Context): JSONObject = JSONObject()
        .put("on", prefs(c).getBoolean("wake_on", false))
        .put("hour", prefs(c).getInt("wake_h", 7))
        .put("minute", prefs(c).getInt("wake_m", 0))
        .put("station", prefs(c).getString("wake_station", null))
        .put("name", prefs(c).getString("wake_name", null))

    /** Sets the next morning's alarm, every day the alarm is on. */
    fun rescheduleWake(c: Context) {
        val p = prefs(c)
        val i = Intent(c, AlarmReceiver::class.java).setAction("wake").putExtra("station", p.getString("wake_station", null))
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
        val list = JSONArray(prefs(c).getString("bells", "[]"))
        for (k in 0 until list.length()) {
            val b = list.getJSONObject(k)
            if (b.optLong("at") > System.currentTimeMillis()) {
                register(c, b.optInt("id"), b.optLong("at"), b.optString("title"), b.optString("station"), b.optString("name"))
            }
        }
        rescheduleWake(c)
    }

    fun ring(c: Context, title: String, station: String, name: String, id: Int) {
        val nm = c.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= 26) {
            nm.createNotificationChannel(NotificationChannel("bells", "Bells", NotificationManager.IMPORTANCE_HIGH))
        }
        val open = PendingIntent.getActivity(c, id,
            Intent(c, MainActivity::class.java).setAction("tune").putExtra("station", station).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        nm.notify(id, NotificationCompat.Builder(c, "bells")
            .setSmallIcon(R.drawable.ic_flag)
            .setContentTitle(title)
            .setContentText("On $name now. Tap to tune in.")
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
        if (i.action == Intent.ACTION_BOOT_COMPLETED) Bells.restoreAll(c)
    }
}
