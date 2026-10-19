package android.content;

import java.util.HashMap;
import java.util.Map;

/**
 * HashMap-backed implementation that mirrors real SharedPreferences editor
 * semantics: edit() buffers mutations and apply() commits them atomically.
 */
public final class MemorySharedPreferences implements SharedPreferences {
    private final Map<String, Object> values = new HashMap<>();

    @Override
    public synchronized long getLong(String key, long defValue) {
        Object value = values.get(key);
        return value instanceof Long ? (Long) value : defValue;
    }

    @Override
    public synchronized String getString(String key, String defValue) {
        Object value = values.get(key);
        return value instanceof String ? (String) value : defValue;
    }

    @Override
    public synchronized boolean getBoolean(String key, boolean defValue) {
        Object value = values.get(key);
        return value instanceof Boolean ? (Boolean) value : defValue;
    }

    @Override
    public Editor edit() {
        return new Editor() {
            private final Map<String, Object> puts = new HashMap<>();
            private final java.util.Set<String> removes = new java.util.HashSet<>();

            @Override
            public Editor putLong(String key, long value) {
                puts.put(key, value);
                removes.remove(key);
                return this;
            }

            @Override
            public Editor putString(String key, String value) {
                puts.put(key, value);
                removes.remove(key);
                return this;
            }

            @Override
            public Editor putBoolean(String key, boolean value) {
                puts.put(key, value);
                removes.remove(key);
                return this;
            }

            @Override
            public Editor remove(String key) {
                puts.remove(key);
                removes.add(key);
                return this;
            }

            @Override
            public void apply() {
                synchronized (MemorySharedPreferences.this) {
                    for (String key : removes) {
                        values.remove(key);
                    }
                    values.putAll(puts);
                }
            }
        };
    }
}
