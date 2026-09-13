package com.hse.bleradar;

/**
 * The automatic-update bookkeeping {@link ApiHttpServer} publishes on
 * {@code /api/updates}; {@link UpdateManager} implements it on the device,
 * the host harness with fixed values.
 */
interface UpdateStatusSource {

    /** Epoch milliseconds of the last update check, or 0 if none. */
    long getLastCheckTimeMs();

    /** Epoch milliseconds of the next scheduled check. */
    long getNextCheckTimeMs();

    /** The current retry attempt count. */
    int getRetryCount();
}
