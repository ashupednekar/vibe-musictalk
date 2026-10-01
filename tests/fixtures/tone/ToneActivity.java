package dev.musictalk.testtone;
import android.app.Activity;
import android.os.Bundle;
import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioTrack;
import android.widget.TextView;
public class ToneActivity extends Activity {
    private volatile boolean running;
    private AudioTrack track;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        TextView label = new TextView(this);
        label.setText("MusicTalk test fixture\nPlaying a capturable 440 Hz tone");
        label.setTextSize(24); setContentView(label);
        track = new AudioTrack.Builder()
            .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_ALL).build())
            .setAudioFormat(new AudioFormat.Builder().setSampleRate(48000)
                .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
            .setBufferSizeInBytes(16384).setTransferMode(AudioTrack.MODE_STREAM).build();
        running = true; track.play();
        new Thread(new Runnable() { public void run() {
            short[] data = new short[2048]; long frame = 0;
            while (running) {
                for(int i=0;i<data.length;i+=2) {
                    short sample = (short)(Math.sin(2*Math.PI*440*frame++/48000)*12000);
                    data[i]=sample; data[i+1]=sample;
                }
                track.write(data,0,data.length,AudioTrack.WRITE_BLOCKING);
            }
        } }, "TestTone").start();
    }
    @Override public void onDestroy() {
        running=false; if(track!=null) { track.stop(); track.release(); } super.onDestroy();
    }
}
