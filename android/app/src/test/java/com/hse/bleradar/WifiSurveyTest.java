package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

import java.util.ArrayList;
import java.util.List;

/**
 * Unit tests for {@link WifiSurvey}: one generation of the survey, published and
 * read as a unit so an API response never mixes two generations.
 */
public class WifiSurveyTest {

    private static WifiAp ap(String bssid) {
        return WifiAp.observe(bssid, "n", "[ESS]", -60, 2437, 1L);
    }

    @Test
    public void an_empty_survey_carries_its_state_and_nothing_else() {
        WifiSurvey survey = WifiSurvey.empty(WifiSurveySource.STATE_WIFI_OFF);
        assertEquals("state", WifiSurveySource.STATE_WIFI_OFF, survey.state);
        assertTrue("no access points", survey.accessPoints.isEmpty());
        assertEquals("nothing dropped", 0, survey.dropped);
    }

    @Test
    public void a_survey_keeps_its_list_state_and_count_together() {
        List<WifiAp> rows = new ArrayList<>();
        rows.add(ap("3c:5a:b4:11:22:01"));
        WifiSurvey survey = new WifiSurvey(rows, WifiSurveySource.STATE_ACTIVE, 2);
        assertEquals("one row", 1, survey.accessPoints.size());
        assertEquals("state", WifiSurveySource.STATE_ACTIVE, survey.state);
        assertEquals("dropped", 2, survey.dropped);
    }

    @Test
    public void a_published_survey_is_not_changed_by_its_source_list() {
        List<WifiAp> rows = new ArrayList<>();
        rows.add(ap("3c:5a:b4:11:22:01"));
        WifiSurvey survey = new WifiSurvey(rows, WifiSurveySource.STATE_ACTIVE, 0);
        rows.add(ap("3c:5a:b4:11:22:02"));
        assertEquals("the generation did not grow", 1, survey.accessPoints.size());
    }

    @Test
    public void a_survey_list_cannot_be_modified_by_a_reader() {
        WifiSurvey survey = WifiSurvey.empty(WifiSurveySource.STATE_IDLE);
        boolean refused = false;
        try {
            survey.accessPoints.add(ap("3c:5a:b4:11:22:01"));
        } catch (UnsupportedOperationException expected) {
            refused = true;
        }
        assertTrue("the list is unmodifiable", refused);
    }
}
