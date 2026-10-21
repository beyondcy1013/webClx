package android.content;

import android.app.DownloadManager;
import android.content.pm.PackageManager;
import java.io.File;
import java.util.HashMap;
import java.util.Map;

/** Minimal Context stub: per-name preference stores, files dir, services. */
public class Context {
    public static final int MODE_PRIVATE = 0;
    public static final String DOWNLOAD_SERVICE = "download";

    private final Map<String, SharedPreferences> stores = new HashMap<>();
    private final PackageManager packageManager = new PackageManager();
    private final DownloadManager downloadManager = new DownloadManager();
    private File filesDir = new File(System.getProperty("java.io.tmpdir"), "webclx-test-files");

    public Context getApplicationContext() {
        return this;
    }

    public SharedPreferences getSharedPreferences(String name, int mode) {
        return stores.computeIfAbsent(name, key -> new MemorySharedPreferences());
    }

    public String getString(int resId) {
        return "stub-string-" + resId;
    }

    public File getFilesDir() {
        return filesDir;
    }

    public void setFilesDir(File dir) {
        filesDir = dir;
    }

    public PackageManager getPackageManager() {
        return packageManager;
    }

    public String getPackageName() {
        return "com.webclx.app";
    }

    public Object getSystemService(String name) {
        if (DOWNLOAD_SERVICE.equals(name)) {
            return downloadManager;
        }
        return null;
    }
}
