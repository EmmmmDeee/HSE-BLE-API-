package com.hse.bleradar;

import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;

/**
 * Fetches the release manifest from its remote source: the platform half of
 * learning about a release. This class moves bytes and reports what happened;
 * every decision about them is the Rust core's — whether the text is a
 * manifest at all ({@link NativeRadar#releaseManifestCanonical}) and what a
 * status or failure means for the check
 * ({@link NativeRadar#remoteManifestDisposition}). It references nothing in
 * {@code android.*}, so {@code cargo xtask verify-api-live} runs it on the
 * host JVM against a scripted server.
 *
 * <p>Redirects within the URL's protocol are followed (a release asset on
 * GitHub answers with one); a redirect across protocols is refused by the
 * platform client and reported as the {@code 3xx} it left. A non-success
 * status is reported without reading its body.
 */
final class ReleaseManifestSource {

    /**
     * What one fetch produced: the HTTP status ({@code 0} without an answer),
     * a {@code NativeRadar.MANIFEST_FETCH_*} failure kind, the body text of a
     * success within the size cap, and a one-line detail for the log.
     */
    static final class Fetch {
        final int httpStatus;
        final int failureKind;
        final String text;
        final String detail;

        Fetch(int httpStatus, int failureKind, String text, String detail) {
            this.httpStatus = httpStatus;
            this.failureKind = failureKind;
            this.text = text;
            this.detail = detail;
        }
    }

    private ReleaseManifestSource() {
    }

    /**
     * GETs {@code url} with the given connect and read timeouts, reading at
     * most {@code maxBytes} of a success body. Never throws: a malformed URL,
     * a failed name resolution or connection, a TLS failure and a timeout are
     * all a {@link NativeRadar#MANIFEST_FETCH_TRANSPORT_FAILURE} with the
     * exception named in the detail.
     */
    static Fetch fetch(String url, int connectTimeoutMs, int readTimeoutMs, int maxBytes) {
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(url).openConnection();
            connection.setConnectTimeout(connectTimeoutMs);
            connection.setReadTimeout(readTimeoutMs);
            connection.setUseCaches(false);
            connection.setRequestProperty("Accept", "text/plain");
            int status = connection.getResponseCode();
            if (status < 0) {
                // No discernible status line: the peer closed or answered
                // something that is not HTTP — a transport failure, not an
                // answer to classify.
                return new Fetch(0, NativeRadar.MANIFEST_FETCH_TRANSPORT_FAILURE, null,
                        "no HTTP status line in the answer");
            }
            if (status < 200 || status > 299) {
                return new Fetch(status, NativeRadar.MANIFEST_FETCH_ANSWERED, null, "HTTP " + status);
            }
            try (InputStream in = connection.getInputStream()) {
                byte[] body = Streams.readUpTo(in, maxBytes);
                if (body == null) {
                    return new Fetch(status, NativeRadar.MANIFEST_FETCH_TOO_LARGE, null,
                            "HTTP " + status + ", body over " + maxBytes + " bytes");
                }
                return new Fetch(status, NativeRadar.MANIFEST_FETCH_ANSWERED,
                        new String(body, StandardCharsets.UTF_8),
                        "HTTP " + status + ", " + body.length + " bytes");
            }
        } catch (IOException | RuntimeException e) {
            // IOException: a malformed URL, DNS, the connection, TLS, a timeout.
            // RuntimeException: a URL of another protocol (the cast), a
            // security manager, or a client bug — reported, never thrown.
            return new Fetch(0, NativeRadar.MANIFEST_FETCH_TRANSPORT_FAILURE, null,
                    e.getClass().getSimpleName() + ": " + e.getMessage());
        } finally {
            if (connection != null) {
                connection.disconnect();
            }
        }
    }
}
