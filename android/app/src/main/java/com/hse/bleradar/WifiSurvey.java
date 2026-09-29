package com.hse.bleradar;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * One generation of the Wi-Fi survey: the access points, the state that explains
 * them, and the number of scan results dropped, published and read as a unit.
 * Immutable, so a response built from it can never pair one generation's list
 * with another's state or count (the engine publishes on the main thread while
 * the API serves on a handler thread). Free of {@code android.*}.
 */
final class WifiSurvey {

    /** The access points, ranked by {@link WifiAp#STRONGEST_FIRST}; never modifiable. */
    final List<WifiAp> accessPoints;
    /** One of {@link WifiSurveySource}'s {@code STATE_*} constants. */
    final String state;
    /** Scan results in the read that produced this survey that named no access point. */
    final int dropped;

    WifiSurvey(List<WifiAp> accessPoints, String state, int dropped) {
        this.accessPoints = Collections.unmodifiableList(new ArrayList<>(accessPoints));
        this.state = state;
        this.dropped = dropped;
    }

    /** A survey with no access point and nothing dropped, in {@code state}. */
    static WifiSurvey empty(String state) {
        return new WifiSurvey(Collections.emptyList(), state, 0);
    }
}
