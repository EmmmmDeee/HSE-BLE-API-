package com.hse.bleradar;

import java.util.List;

/**
 * The live scan state {@link ApiHttpServer} publishes.
 *
 * <p>{@link BleScanEngine} implements it on the device; the host harness
 * behind {@code cargo xtask verify-api-live} implements it with fixture
 * devices, which is what lets the real server run on a plain JVM (nothing
 * in {@code android.*} is reachable there).
 */
interface SnapshotSource {

    /** Every live device in ranked order (a defensive copy). */
    List<Blip> snapshot();

    /** Whether a scan is active. */
    boolean isScanning();

    /** Milliseconds since scanning started, or 0 when idle. */
    long getUptimeMillis();
}
