package com.hse.bleradar;

import java.io.IOException;
import java.io.InputStream;

/**
 * Opens a packaged asset by name: {@code AssetManager::open} on the device,
 * a file on the host. The caller closes the stream.
 */
interface AssetSource {

    InputStream open(String name) throws IOException;
}
