package com.hse.bleradar;

/**
 * The minimal JSON writer behind {@link ApiHttpServer}'s three documents:
 * objects, arrays, names and the scalar values the API uses, with no
 * whitespace and RFC 8259 string escaping.
 *
 * <p>It exists instead of {@code android.util.JsonWriter} so the server has
 * no {@code android.*} dependency and runs unchanged on a host JVM, where
 * {@code cargo xtask verify-api-live} compares its output byte for byte with
 * the fixture the browser proofs use. The state machine mirrors the subset of
 * {@code JsonWriter} the server used: a comma precedes every value or name
 * that follows another value or a closed container.
 */
final class Json {

    private final StringBuilder out = new StringBuilder();
    private boolean needsComma;

    Json beginObject() {
        separator();
        out.append('{');
        needsComma = false;
        return this;
    }

    Json endObject() {
        out.append('}');
        needsComma = true;
        return this;
    }

    Json beginArray() {
        separator();
        out.append('[');
        needsComma = false;
        return this;
    }

    Json endArray() {
        out.append(']');
        needsComma = true;
        return this;
    }

    Json name(String name) {
        separator();
        string(name);
        out.append(':');
        needsComma = false;
        return this;
    }

    /** A string value; {@code null} writes JSON {@code null}. */
    Json value(String value) {
        separator();
        if (value == null) {
            out.append("null");
        } else {
            string(value);
        }
        needsComma = true;
        return this;
    }

    Json value(boolean value) {
        separator();
        out.append(value ? "true" : "false");
        needsComma = true;
        return this;
    }

    Json value(long value) {
        separator();
        out.append(value);
        needsComma = true;
        return this;
    }

    /** A number, or JSON {@code null} for NaN and infinities, which JSON cannot represent. */
    Json value(double value) {
        separator();
        if (Double.isFinite(value)) {
            out.append(Double.toString(value));
        } else {
            out.append("null");
        }
        needsComma = true;
        return this;
    }

    @Override
    public String toString() {
        return out.toString();
    }

    private void separator() {
        if (needsComma) {
            out.append(',');
        }
    }

    private void string(String value) {
        out.append('"');
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            switch (c) {
                case '"':
                    out.append("\\\"");
                    break;
                case '\\':
                    out.append("\\\\");
                    break;
                case '\n':
                    out.append("\\n");
                    break;
                case '\r':
                    out.append("\\r");
                    break;
                case '\t':
                    out.append("\\t");
                    break;
                case '\b':
                    out.append("\\b");
                    break;
                case '\f':
                    out.append("\\f");
                    break;
                default:
                    if (c < 0x20 || c == ' ' || c == ' ') {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
                    break;
            }
        }
        out.append('"');
    }
}
