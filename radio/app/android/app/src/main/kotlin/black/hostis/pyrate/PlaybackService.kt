package black.hostis.pyrate

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.Ringtone
import android.media.RingtoneManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.PowerManager
import android.support.v4.media.MediaBrowserCompat
import android.support.v4.media.MediaDescriptionCompat
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.media.MediaBrowserServiceCompat

/**
 * Keeps the station playing with the screen off, and answers the lock screen, headphones,
 * Bluetooth, the car and the morning alarm. The node itself runs in Rust.
 */
class PlaybackService : MediaBrowserServiceCompat() {
    private var wake: PowerManager.WakeLock? = null
    private var wifi: WifiManager.WifiLock? = null
    private lateinit var session: MediaSessionCompat
    private lateinit var audio: AudioManager
    private var focus: AudioFocusRequest? = null
    private val main = Handler(Looper.getMainLooper())
    /** Every call into the node runs here, one after another, never on the main thread: a
     *  node starting up or busy with a slow job cannot stall the lock screen or the car. */
    private val worker = HandlerThread("pyrate-node").apply { start() }
    private val work = Handler(worker.looper)
    private var lastKey = ""
    @Volatile private var live = false
    @Volatile private var foreground = false
    private var alarmTone: Ringtone? = null
    @Volatile private var waking = false

    private val noisy = object : BroadcastReceiver() {
        override fun onReceive(c: Context?, i: Intent?) {
            if (i?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY && !live) stopPlaying()
        }
    }

    private val poll = object : Runnable {
        override fun run() {
            refresh()
            work.postDelayed(this, 1500)
        }
    }

    override fun onCreate() {
        super.onCreate()
        work.post { Native.ensure(this) }
        audio = getSystemService(Context.AUDIO_SERVICE) as AudioManager
        session = MediaSessionCompat(this, "pyrate").apply {
            setCallback(object : MediaSessionCompat.Callback() {
                override fun onPlay() { work.post { resume() } }
                override fun onPause() = stopPlaying()
                override fun onStop() = stopPlaying()
                override fun onSkipToNext() { work.post { Native.cmd("cmd" to "tune_next", "dir" to 1); startPlaying() } }
                override fun onSkipToPrevious() { work.post { Native.cmd("cmd" to "tune_next", "dir" to -1); startPlaying() } }
                override fun onPlayFromMediaId(id: String?, extras: Bundle?) {
                    if (id != null && Regex("^[0-9a-f]{64}$").matches(id)) work.post { tune(id) }
                }
            })
            isActive = true
        }
        sessionToken = session.sessionToken
        registerReceiver(noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY))
        if (Build.VERSION.SDK_INT >= 26) {
            (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
                .createNotificationChannel(NotificationChannel("air", "On the air", NotificationManager.IMPORTANCE_LOW))
        }
        work.post(poll)
    }

    private fun prefs() = getSharedPreferences("pyrate", Context.MODE_PRIVATE)

    /** On the node's thread. */
    private fun tune(id: String) {
        Native.cmd("cmd" to "tune", "station" to id)
        Hold.putS(this, "last", id)
        startPlaying()
    }

    /** On the node's thread. */
    private fun resume() {
        val tuned = Native.cmd("cmd" to "status").optString("tuned", "")
        if (tuned.isEmpty()) {
            val last = Hold.getS(this, "last")
            if (last != null) tune(last) else { Native.cmd("cmd" to "tune_next", "dir" to 1); startPlaying() }
        } else startPlaying()
    }

    /** From any thread: the node is asked on its own, the speaker and the lock screen on the main one. */
    private fun startPlaying(title: String = "Pyrate Radio") {
        work.post {
            Native.cmd("cmd" to "volume", "value" to 1.0)
            main.post {
                requestFocus()
                goForeground(title, "")
            }
            refresh()
        }
    }

    private fun stopPlaying() {
        work.post { Native.cmd("cmd" to "stop") }
        main.post {
            alarmTone?.stop()
            alarmTone = null
            waking = false
            if (Build.VERSION.SDK_INT >= 26) focus?.let { audio.abandonAudioFocusRequest(it) }
            session.setPlaybackState(state(PlaybackStateCompat.STATE_STOPPED))
            if (!live) {
                stopForeground(STOP_FOREGROUND_REMOVE)
                foreground = false
                release()
            }
        }
    }

    /** One request for the speaker, kept for the life of the service: asking again with a new
     *  one each tune left the old ones registered, and they answered later focus changes. */
    private fun requestFocus() {
        if (Build.VERSION.SDK_INT < 26) return
        focus?.let { audio.requestAudioFocus(it); return }
        val req = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setOnAudioFocusChangeListener { change ->
                when (change) {
                    // A call or a voice prompt: radio goes quiet and keeps its place in the hour.
                    AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> work.post { Native.cmd("cmd" to "volume", "value" to 0.0) }
                    AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> work.post { Native.cmd("cmd" to "volume", "value" to 0.25) }
                    AudioManager.AUDIOFOCUS_GAIN -> work.post { Native.cmd("cmd" to "volume", "value" to 1.0) }
                    // Another player took the speaker for good.
                    AudioManager.AUDIOFOCUS_LOSS -> if (!live) stopPlaying()
                }
            }
            .build()
        focus = req
        audio.requestAudioFocus(req)
    }

    private fun state(s: Int) = PlaybackStateCompat.Builder()
        .setActions(PlaybackStateCompat.ACTION_PLAY or PlaybackStateCompat.ACTION_PAUSE or PlaybackStateCompat.ACTION_STOP or
            PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
            PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or PlaybackStateCompat.ACTION_PLAY_FROM_MEDIA_ID)
        .setState(s, PlaybackStateCompat.PLAYBACK_POSITION_UNKNOWN, 1f)
        .build()

    /** The plain disguise: the lock screen says only "Radio". */
    private val plain: Boolean get() = prefs().getBoolean("plain_lock", false)

    /** What the lock screen, the car and the headphones show. */
    /** On the node's thread; what it changes on screen is handed to the main one. */
    private fun refresh() {
        val st = Native.cmd("cmd" to "status")
        val tuned = st.optString("tuned", "")
        if (tuned.isEmpty() && !live) return
        val now = st.optJSONObject("now")
        val stations = Native.list("stations")
        var name = "Pyrate Radio"
        var cover: String? = null
        for (i in 0 until stations.length()) {
            val s = stations.getJSONObject(i)
            if (s.optString("id") == tuned) { name = s.optString("name"); cover = s.optString("cover").ifEmpty { null } }
        }
        val title = now?.optString("title")?.ifEmpty { null } ?: name
        val by = now?.optString("by")?.ifEmpty { null } ?: name
        val pic = now?.optString("art")?.ifEmpty { null } ?: cover
        val plain = prefs().getBoolean("plain_lock", false)
        val key = "$tuned|$title|$by|$pic|$plain"
        val sounding = now?.optString("state") == "playing"
        main.post {
            session.setPlaybackState(state(PlaybackStateCompat.STATE_PLAYING))
            if (waking && sounding) {
                waking = false
                alarmTone?.stop()
                alarmTone = null
            }
        }
        if (key == lastKey) return
        lastKey = key
        val md = MediaMetadataCompat.Builder()
        if (plain) {
            md.putString(MediaMetadataCompat.METADATA_KEY_TITLE, "Radio")
        } else {
            md.putString(MediaMetadataCompat.METADATA_KEY_TITLE, title)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, by)
                .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, name)
            Native.bitmap(pic)?.let { md.putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, it) }
        }
        val built = md.build()
        main.post {
            session.setMetadata(built)
            if (foreground) goForeground(if (plain) "Radio" else title, if (plain) "" else name)
        }
    }

    private fun goForeground(title: String, text: String) {
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE)
        fun action(icon: Int, label: String, a: Long) = NotificationCompat.Action(icon, label,
            androidx.media.session.MediaButtonReceiver.buildMediaButtonPendingIntent(this, a))
        val n = NotificationCompat.Builder(this, "air")
            .setContentTitle(if (live && !plain) "ON AIR" else title)
            .setContentText(text)
            .setSmallIcon(R.drawable.ic_flag)
            .setContentIntent(open)
            .setOngoing(true)
            // In the plain disguise the lock screen shows only "Radio", whatever is on.
            .setVisibility(if (plain) NotificationCompat.VISIBILITY_PRIVATE else NotificationCompat.VISIBILITY_PUBLIC)
            .apply { if (plain) setPublicVersion(NotificationCompat.Builder(this@PlaybackService, "air").setContentTitle("Radio").setSmallIcon(R.drawable.ic_flag).build()) }
            .addAction(action(android.R.drawable.ic_media_previous, "Previous station", PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS))
            .addAction(action(android.R.drawable.ic_menu_close_clear_cancel, "Stop", PlaybackStateCompat.ACTION_STOP))
            .addAction(action(android.R.drawable.ic_media_next, "Next station", PlaybackStateCompat.ACTION_SKIP_TO_NEXT))
            .setStyle(androidx.media.app.NotificationCompat.MediaStyle().setMediaSession(session.sessionToken).setShowActionsInCompactView(0, 1, 2))
            .build()
        if (Build.VERSION.SDK_INT >= 30 && live) {
            startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
        } else if (Build.VERSION.SDK_INT >= 29) {
            startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } else {
            startForeground(1, n)
        }
        foreground = true
        hold()
    }

    private fun hold() {
        if (wake == null) {
            wake = (getSystemService(Context.POWER_SERVICE) as PowerManager)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "pyrate:air").apply { acquire() }
        }
        if (wifi == null) {
            @Suppress("DEPRECATION")
            wifi = (applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
                .createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "pyrate:air").apply { acquire() }
        }
    }

    private fun release() {
        wake?.let { if (it.isHeld) it.release() }
        wifi?.let { if (it.isHeld) it.release() }
        wake = null
        wifi = null
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        androidx.media.session.MediaButtonReceiver.handleIntent(session, intent)
        when (intent?.action) {
            "wake" -> wakeUp(intent.getStringExtra("station"))
            "stop" -> stopPlaying()
            else -> {
                val ending = intent?.hasExtra("live") == true && !intent.getBooleanExtra("live", false)
                if (intent?.hasExtra("live") == true) live = intent.getBooleanExtra("live", false)
                if (ending) {
                    // The broadcast is over: back to what is tuned, or out of the foreground.
                    work.post {
                        val tuned = Native.cmd("cmd" to "status").optString("tuned", "")
                        if (tuned.isEmpty()) stopPlaying() else refresh()
                    }
                } else {
                    // The app names the station; the plain disguise names nothing. The service
                    // goes to the foreground at once, before the node is asked anything.
                    val t = intent?.getStringExtra("title")?.takeIf { !plain } ?: "Pyrate Radio"
                    goForeground(t, "")
                    startPlaying(t)
                }
            }
        }
        return START_NOT_STICKY
    }

    /** The morning alarm: the station, rising from quiet. If no sound comes, the phone's
     *  own alarm rings instead, so nobody oversleeps on a dead network. */
    private fun wakeUp(station: String?) {
        live = false
        waking = true
        goForeground("Good morning", "Tuning in")
        if (station != null) work.post { Native.cmd("cmd" to "tune", "station" to station) }
        requestFocus()
        for (step in 0..10) {
            work.postDelayed({ if (waking || step > 0) Native.cmd("cmd" to "volume", "value" to (0.15 + step * 0.085)) }, step * 3000L)
        }
        main.postDelayed({
            if (waking) {
                val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM) ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
                alarmTone = RingtoneManager.getRingtone(this, uri)?.apply {
                    if (Build.VERSION.SDK_INT >= 28) isLooping = true
                    play()
                }
            }
        }, 45_000)
        Bells.rescheduleWake(this)
    }

    /** Only this app, the system and the car browse the Log: no other app on the phone may
     *  read the stations its owner listens to, or tune it. */
    override fun onGetRoot(clientPackageName: String, clientUid: Int, rootHints: Bundle?): BrowserRoot? {
        val trusted = clientUid == android.os.Process.myUid() ||
            clientUid == android.os.Process.SYSTEM_UID ||
            clientPackageName in setOf(
                "com.android.systemui",
                "com.google.android.projection.gearhead",
                "com.google.android.mediasimulator",
                "com.google.android.autosimulator",
                "com.android.bluetooth",
            )
        return if (trusted) BrowserRoot("root", null) else null
    }

    /** The Log, for the car and anything else that browses media. */
    override fun onLoadChildren(parentId: String, result: Result<MutableList<MediaBrowserCompat.MediaItem>>) {
        result.detach()
        work.post { loadChildren(result) }
    }

    private fun loadChildren(result: Result<MutableList<MediaBrowserCompat.MediaItem>>) {
        val items = mutableListOf<MediaBrowserCompat.MediaItem>()
        val stations = Native.list("stations")
        for (i in 0 until stations.length()) {
            val s = stations.getJSONObject(i)
            // A pocket station is for its members, never for a list another app can read.
            if (s.optBoolean("pocket")) continue
            val d = MediaDescriptionCompat.Builder()
                .setMediaId(s.optString("id"))
                .setTitle(s.optString("name"))
                .setSubtitle(s.optString("about"))
            Native.bitmap(s.optString("cover").ifEmpty { null })?.let { d.setIconBitmap(Bitmap.scale(it)) }
            items.add(MediaBrowserCompat.MediaItem(d.build(), MediaBrowserCompat.MediaItem.FLAG_PLAYABLE))
        }
        main.post { result.sendResult(items) }
    }

    override fun onDestroy() {
        work.removeCallbacksAndMessages(null)
        worker.quitSafely()
        main.removeCallbacksAndMessages(null)
        try { unregisterReceiver(noisy) } catch (e: Exception) {}
        session.release()
        release()
        super.onDestroy()
    }

    private object Bitmap {
        fun scale(b: android.graphics.Bitmap) = android.graphics.Bitmap.createScaledBitmap(b, 160, 160, true)
    }
}
