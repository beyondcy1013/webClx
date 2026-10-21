package android.content;

/** Minimal in-memory SharedPreferences stub for JVM update-state tests. */
public interface SharedPreferences {
    long getLong(String key, long defValue);
    String getString(String key, String defValue);
    boolean getBoolean(String key, boolean defValue);
    Editor edit();

    interface Editor {
        Editor putLong(String key, long value);
        Editor putString(String key, String value);
        Editor putBoolean(String key, boolean value);
        Editor remove(String key);
        void apply();
    }
}
