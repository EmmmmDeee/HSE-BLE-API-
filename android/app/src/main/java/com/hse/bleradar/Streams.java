package com.hse.bleradar;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;

/**
 * The one {@link InputStream}-draining helper in the app.
 *
 * <p>{@code InputStream.readAllBytes()} is present in {@code android.jar} (so
 * {@code javac} accepts it) but exists on devices only from API 33, while the
 * manifest's {@code minSdkVersion} is 26: on Android 8–12 the call site throws
 * {@code NoSuchMethodError}, an {@code Error} that no {@code catch (Exception)}
 * sees, and the process dies. Every asset read therefore goes through this
 * loop, and {@code cargo xtask verify-android-live} runs Android lint's
 * {@code NewApi} check over the sources so no library call above the
 * declared minimum can come back.
 */
final class Streams {

    private static final int CHUNK_BYTES = 8 * 1024;

    private Streams() {
    }

    /** Reads {@code in} to its end and returns every byte; the caller closes the stream. */
    static byte[] readAllBytes(InputStream in) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] chunk = new byte[CHUNK_BYTES];
        for (int read = in.read(chunk); read >= 0; read = in.read(chunk)) {
            out.write(chunk, 0, read);
        }
        return out.toByteArray();
    }
}
