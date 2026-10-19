package org.json;

/** Compilation-only stub; the harness never reaches manifest parsing. */
public class JSONObject {
    public JSONObject(String body) {}

    public String getString(String key) {
        throw new UnsupportedOperationException("stub");
    }

    public long getLong(String key) {
        throw new UnsupportedOperationException("stub");
    }

    public String optString(String key, String fallback) {
        return fallback;
    }
}
