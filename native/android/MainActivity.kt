package dev.dioxus.main

import android.app.*
import android.content.*
import android.content.pm.ServiceInfo
import android.media.*
import android.media.projection.*
import android.os.*
import android.util.Base64
import android.webkit.*
import org.json.JSONObject
import java.lang.ref.WeakReference
import java.nio.ByteBuffer
import java.nio.ByteOrder

typealias BuildConfig = dev.musictalk.BuildConfig

class MainActivity : WryActivity() {
    companion object { var current = WeakReference<MainActivity>(null) }
    private var webView: WebView? = null
    private var awaitingCapture = false

    override fun onWebViewCreate(webView: WebView) {
        super.onWebViewCreate(webView)
        current = WeakReference(this)
        this.webView = webView
        // Joining the call is the explicit opt-in; remote audio should then play immediately.
        webView.settings.mediaPlaybackRequiresUserGesture = false
        // Only the bundled Dioxus page gets this bridge. Do not load external pages in this WebView.
        webView.addJavascriptInterface(AudioBridge(), "MusicTalkAudio")
    }

    fun emit(script: String) { runOnUiThread { webView?.evaluateJavascript(script, null) } }
    fun error(message: String) { emit("window.musictalkNative?.error(${JSONObject.quote(message)})") }

    inner class AudioBridge {
        @JavascriptInterface fun start() { runOnUiThread {
            if (awaitingCapture || AudioShareService.running) { error("Audio sharing is already starting or running."); return@runOnUiThread }
            awaitingCapture = true
            val manager = getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
            @Suppress("DEPRECATION")
            val captureIntent = if (Build.VERSION.SDK_INT >= 34) {
                manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay())
            } else manager.createScreenCaptureIntent()
            startActivityForResult(captureIntent, 4201)
        } }
        @JavascriptInterface fun stop() { runOnUiThread { awaitingCapture = false; stopService(Intent(this@MainActivity, AudioShareService::class.java)) } }
        @JavascriptInterface fun copy(text: String) {
            val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            clipboard.setPrimaryClip(ClipData.newPlainText("MusicTalk invite", text))
        }
    }

    @Deprecated("Platform callback retained for the capture result")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != 4201) return
        if (!awaitingCapture) return
        awaitingCapture = false
        if (resultCode != RESULT_OK || data == null) { error("Audio sharing was cancelled."); return }
        val intent = Intent(this, AudioShareService::class.java).putExtra("resultCode", resultCode).putExtra("projectionData", data)
        startForegroundService(intent)
    }

    override fun onDestroy() {
        stopService(Intent(this, AudioShareService::class.java))
        current.clear()
        super.onDestroy()
    }
}

// Android requires a visible foreground service before using MediaProjection.
class AudioShareService : Service() {
    companion object { @Volatile var running = false }
    private var projection: MediaProjection? = null
    private var recorder: AudioRecord? = null
    private var reader: Thread? = null
    private val main = Handler(Looper.getMainLooper())
    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == "STOP") { stopSelf(); return START_NOT_STICKY }
        if (running || intent == null) return START_NOT_STICKY
        val channel = NotificationChannel("audio-share", "Shared audio", NotificationManager.IMPORTANCE_LOW)
        val notifications = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        notifications.createNotificationChannel(channel)
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val stop = PendingIntent.getService(this, 1, Intent(this, AudioShareService::class.java).setAction("STOP"), PendingIntent.FLAG_IMMUTABLE)
        if (intent.action == "STOP") { stopSelf(); return START_NOT_STICKY }
        val notification = Notification.Builder(this, "audio-share").setContentTitle("MusicTalk · sharing your sound")
            .setContentText("Only audio is shared. Tap Stop to end sharing.")
            .setSmallIcon(android.R.drawable.ic_lock_silent_mode_off).setContentIntent(open)
            .setOngoing(true).addAction(Notification.Action.Builder(null, "Stop", stop).build()).build()
        startForeground(42, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
        try {
            @Suppress("DEPRECATION")
            val data = intent.getParcelableExtra<Intent>("projectionData") ?: throw IllegalArgumentException("Missing capture permission.")
            val manager = getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
            projection = manager.getMediaProjection(intent.getIntExtra("resultCode", Activity.RESULT_CANCELED), data)
            projection!!.registerCallback(object : MediaProjection.Callback() { override fun onStop() { stopSelf() } }, main)
            val capture = AudioPlaybackCaptureConfiguration.Builder(projection!!)
                .addMatchingUsage(AudioAttributes.USAGE_MEDIA).addMatchingUsage(AudioAttributes.USAGE_GAME).excludeUid(Process.myUid()).build()
            val format = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .setSampleRate(48000).setChannelMask(AudioFormat.CHANNEL_IN_STEREO).build()
            val minimum = AudioRecord.getMinBufferSize(48000, AudioFormat.CHANNEL_IN_STEREO, AudioFormat.ENCODING_PCM_16BIT)
            recorder = AudioRecord.Builder().setAudioFormat(format).setBufferSizeInBytes(maxOf(minimum * 2, 16384))
                .setAudioPlaybackCaptureConfig(capture).build()
            check(recorder!!.state == AudioRecord.STATE_INITIALIZED) { "Device audio capture could not be initialized." }
            recorder!!.startRecording()
            running = true
            MainActivity.current.get()?.emit("window.musictalkNative?.started()")
            val audioRecord = recorder!!
            reader = Thread {
                val samples = ShortArray(2048)
                val bytes = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
                try {
                    while (running) {
                        val count = audioRecord.read(samples, 0, samples.size, AudioRecord.READ_BLOCKING)
                        if (count <= 0) { if (running) throw IllegalStateException("Device audio capture stopped."); break }
                        bytes.clear()
                        for (i in 0 until count) bytes.putShort(samples[i])
                        val encoded = Base64.encodeToString(bytes.array(), 0, count * 2, Base64.NO_WRAP)
                        MainActivity.current.get()?.emit("window.musictalkNative?.push('$encoded')")
                    }
                } catch (e: Exception) {
                    if (running) { MainActivity.current.get()?.error(e.message ?: "Device audio capture stopped."); main.post { stopSelf() } }
                }
            }.also { it.name = "MusicTalkAudio"; it.start() }
        } catch (e: Exception) { MainActivity.current.get()?.error(e.message ?: "Could not start device audio sharing."); stopSelf() }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        running = false
        try { recorder?.stop() } catch (_: Exception) {}
        reader?.join(500)
        recorder?.release(); recorder = null
        projection?.stop(); projection = null
        MainActivity.current.get()?.emit("window.musictalkNative?.stopped()")
        super.onDestroy()
    }
}
