package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * The two checks every route of {@link ApiHttpServer} applies before it answers:
 * whose {@code Host} a request names, and whether its {@code Origin} is the same.
 * They are what keep a web page in the user's browser away from the loopback
 * API (DNS rebinding, a cross-site POST); the live server is exercised over a
 * real socket by {@code cargo xtask verify-api-live}.
 */
public class ApiHttpServerGuardTest {

    @Test
    public void the_loopback_names_are_allowed_with_or_without_a_port() {
        String[] allowed = {
            "127.0.0.1", "127.0.0.1:8080", "localhost", "localhost:8080", "LOCALHOST:1",
            "[::1]", "[::1]:8080", " 127.0.0.1:40875 ",
        };
        for (String host : allowed) {
            assertTrue("allowed: " + host, ApiHttpServer.hostAllowed(host));
        }
    }

    @Test
    public void any_other_name_is_refused_including_look_alikes() {
        String[] refused = {
            null, "", "evil.example", "evil.example:8080", "127.0.0.1.evil.example",
            "localhost.evil.example", "evil.example:127.0.0.1", "127.0.0.2", "0.0.0.0",
            "127.0.0.1:", "127.0.0.1:abc", "127.0.0.1:123456", "[::1", "[::2]", "[::1]x",
            "user@127.0.0.1", "127.0.0.1 evil.example",
        };
        for (String host : refused) {
            assertFalse("refused: " + host, ApiHttpServer.hostAllowed(host));
        }
    }

    @Test
    public void an_origin_is_the_same_only_when_it_is_the_host_served() {
        assertTrue("the dashboard's own POST",
                ApiHttpServer.sameOrigin("http://127.0.0.1:8080", "127.0.0.1:8080"));
        assertTrue("case is not significant",
                ApiHttpServer.sameOrigin("HTTP://LocalHost:8080", "localhost:8080"));
        assertFalse("another site", ApiHttpServer.sameOrigin("http://evil.example", "127.0.0.1:8080"));
        assertFalse("another port", ApiHttpServer.sameOrigin("http://127.0.0.1:9090", "127.0.0.1:8080"));
        assertFalse("another scheme", ApiHttpServer.sameOrigin("https://127.0.0.1:8080", "127.0.0.1:8080"));
        assertFalse("a sandboxed page", ApiHttpServer.sameOrigin("null", "127.0.0.1:8080"));
        assertFalse("no origin is not an origin", ApiHttpServer.sameOrigin(null, "127.0.0.1:8080"));
        assertFalse("no host", ApiHttpServer.sameOrigin("http://127.0.0.1:8080", null));
    }

    @Test
    public void the_header_caps_are_bounded_and_positive() {
        assertTrue("a line cap", ApiHttpServer.MAX_HEADER_LINE_CHARS > 0
                && ApiHttpServer.MAX_HEADER_LINE_CHARS <= 64 * 1024);
        assertTrue("a header-count cap", ApiHttpServer.MAX_HEADERS > 0 && ApiHttpServer.MAX_HEADERS <= 256);
    }
}
