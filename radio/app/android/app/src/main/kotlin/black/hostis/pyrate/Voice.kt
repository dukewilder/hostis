package black.hostis.pyrate

import android.content.Context
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import java.io.File

/** Spoken idents and announcements in the phone's own voice, written to a file on the phone. */
object Voice {
    fun speakToFile(context: Context, text: String, path: String, rate: Float, pitch: Float, done: (Boolean) -> Unit) {
        val main = Handler(Looper.getMainLooper())
        var finished = false
        fun finish(ok: Boolean) = main.post { if (!finished) { finished = true; done(ok) } }
        var tts: TextToSpeech? = null
        tts = TextToSpeech(context.applicationContext) { status ->
            val t = tts
            if (status != TextToSpeech.SUCCESS || t == null) { finish(false); return@TextToSpeech }
            t.setSpeechRate(rate)
            t.setPitch(pitch)
            t.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                override fun onStart(id: String?) {}
                override fun onDone(id: String?) { t.shutdown(); finish(true) }
                @Deprecated("Deprecated in Java")
                override fun onError(id: String?) { t.shutdown(); finish(false) }
                override fun onError(id: String?, code: Int) { t.shutdown(); finish(false) }
            })
            if (t.synthesizeToFile(text, Bundle(), File(path), "pyrate-voice") != TextToSpeech.SUCCESS) { t.shutdown(); finish(false) }
        }
        main.postDelayed({ finish(false) }, 60_000)
    }
}
