package com.webclx.app;

import android.app.DownloadManager;
import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;

import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.File;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.regex.Pattern;

final class UpdateManager {
    static final String DOWNLOAD_PREFERENCES = "download_install";
    static final String PENDING_APK_DOWNLOAD_ID = "pending_apk_download_id";
    static final String WAITING_FOR_INSTALL_PERMISSION = "waiting_for_install_permission";
    static final String EXPECTED_SHA256 = "expected_sha256";
    static final String EXPECTED_SIZE = "expected_size";
    static final String EXPECTED_VERSION_CODE = "expected_version_code";
    static final String VERIFIED_DOWNLOAD_ID = "verified_download_id";
    static final String VERIFIED_APK_PATH = "verified_apk_path";
    static final String INSTALLER_LAUNCHED_DOWNLOAD_ID = "installer_launched_download_id";
    static final long NO_DOWNLOAD = -1L;
    private static final long MAX_APK_SIZE = 512L * 1024L * 1024L;
    private static final Pattern SHA256 = Pattern.compile("[0-9a-f]{64}");
    private static final Pattern VERSION = Pattern.compile("[0-9]+\\.[0-9]+\\.[0-9]+");
    private static final ExecutorService EXECUTOR = Executors.newSingleThreadExecutor();
    private static final Object PREFERENCES_LOCK = new Object();

    interface Callback {
        void onSuccess(UpdateManifest manifest);
        void onError(String message);
    }

    private UpdateManager() {}

    /**
     * Serializes the short SharedPreferences mutations that make up one update
     * transaction so concurrent callers can never interleave half-written
     * pending/verified download state.
     */
    static void mutatePreferences(Context context, java.util.function.Consumer<SharedPreferences.Editor> mutation) {
        synchronized (PREFERENCES_LOCK) {
            SharedPreferences.Editor editor = preferences(context).edit();
            mutation.accept(editor);
            editor.apply();
        }
    }

    static long pendingDownloadId(Context context) {
        return preferences(context).getLong(PENDING_APK_DOWNLOAD_ID, NO_DOWNLOAD);
    }

    /** True only while {@code downloadId} still names the live pending update. */
    static boolean isCurrentPending(Context context, long downloadId) {
        return downloadId != NO_DOWNLOAD && pendingDownloadId(context) == downloadId;
    }

    /**
     * Immutable view of the expected-update fields for one download id,
     * captured atomically so a verification worker never reads mutable
     * preferences that may already belong to a newer download.
     */
    static final class PendingUpdateSnapshot {
        final long downloadId;
        final String sha256;
        final long size;
        final long versionCode;

        PendingUpdateSnapshot(long downloadId, String sha256, long size, long versionCode) {
            this.downloadId = downloadId;
            this.sha256 = sha256;
            this.size = size;
            this.versionCode = versionCode;
        }
    }

    /**
     * Returns a snapshot for {@code downloadId} only while preferences still
     * name it as the live pending download, otherwise null.
     */
    static PendingUpdateSnapshot pendingSnapshot(Context context, long downloadId) {
        synchronized (PREFERENCES_LOCK) {
            SharedPreferences prefs = preferences(context);
            if (downloadId == NO_DOWNLOAD
                || prefs.getLong(PENDING_APK_DOWNLOAD_ID, NO_DOWNLOAD) != downloadId) {
                return null;
            }
            return new PendingUpdateSnapshot(
                downloadId,
                prefs.getString(EXPECTED_SHA256, ""),
                prefs.getLong(EXPECTED_SIZE, -1),
                prefs.getLong(EXPECTED_VERSION_CODE, -1)
            );
        }
    }

    /**
     * Commits a verified staged APK only if the pending download id has not
     * moved on to a newer download since verification started.
     */
    static boolean commitVerifiedApk(Context context, long downloadId, String stagedPath) {
        synchronized (PREFERENCES_LOCK) {
            if (!isCurrentPending(context, downloadId)) {
                return false;
            }
            preferences(context).edit()
                .putLong(VERIFIED_DOWNLOAD_ID, downloadId)
                .putString(VERIFIED_APK_PATH, stagedPath)
                .apply();
            return true;
        }
    }

    /**
     * Starts an asynchronous manifest check.  Callers must pass an application
     * context lifetime (the method takes one internally) and may cancel the
     * returned Future when the requesting screen is destroyed.
     */
    static Future<?> check(Context context, int preferredSource, Callback callback) {
        final Context appContext = context.getApplicationContext();
        return EXECUTOR.submit(() -> {
            String lastError = appContext.getString(R.string.update_check_failed);
            for (int offset = 0; offset < SourceRegistry.URLS.length; offset++) {
                int index = preferredSource >= 0
                    ? (preferredSource + offset) % SourceRegistry.URLS.length
                    : offset;
                try {
                    UpdateManifest manifest = fetchManifest(index);
                    callback.onSuccess(manifest);
                    return;
                } catch (Exception error) {
                    String message = error.getMessage();
                    if (message != null && !message.trim().isEmpty()) {
                        lastError = message;
                    }
                }
            }
            callback.onError(lastError);
        });
    }

    static long currentVersionCode(Context context) {
        try {
            PackageInfo info = context.getPackageManager().getPackageInfo(context.getPackageName(), 0);
            return Build.VERSION.SDK_INT >= Build.VERSION_CODES.P
                ? info.getLongVersionCode()
                : info.versionCode;
        } catch (Exception error) {
            return 0;
        }
    }

    static String currentVersionName(Context context) {
        try {
            PackageInfo info = context.getPackageManager().getPackageInfo(context.getPackageName(), 0);
            return info.versionName == null || info.versionName.trim().isEmpty()
                ? context.getString(R.string.update_version_unknown)
                : info.versionName;
        } catch (Exception error) {
            return context.getString(R.string.update_version_unknown);
        }
    }

    static boolean isNewer(Context context, UpdateManifest manifest) {
        return manifest.versionCode > currentVersionCode(context);
    }

    static long enqueue(Context context, UpdateManifest manifest) {
        DownloadManager.Request request = new DownloadManager.Request(Uri.parse(manifest.url));
        request.setTitle(manifest.file);
        request.setDescription(context.getString(R.string.update_downloading));
        request.setMimeType(MainActivity.APK_MIME_TYPE);
        request.setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED);
        request.setVisibleInDownloadsUi(true);
        request.setDestinationInExternalFilesDir(
            context,
            Environment.DIRECTORY_DOWNLOADS,
            "updates/" + manifest.file
        );
        DownloadManager manager = (DownloadManager) context.getSystemService(Context.DOWNLOAD_SERVICE);
        long downloadId = manager.enqueue(request);
        mutatePreferences(context, editor -> editor
            .putLong(PENDING_APK_DOWNLOAD_ID, downloadId)
            .putString(EXPECTED_SHA256, manifest.sha256)
            .putLong(EXPECTED_SIZE, manifest.size)
            .putLong(EXPECTED_VERSION_CODE, manifest.versionCode)
            .putLong(VERIFIED_DOWNLOAD_ID, NO_DOWNLOAD)
            .remove(VERIFIED_APK_PATH)
            .putLong(INSTALLER_LAUNCHED_DOWNLOAD_ID, NO_DOWNLOAD)
            .putBoolean(WAITING_FOR_INSTALL_PERMISSION, false));
        return downloadId;
    }

    static SharedPreferences preferences(Context context) {
        return context.getSharedPreferences(DOWNLOAD_PREFERENCES, Context.MODE_PRIVATE);
    }

    /**
     * Clears the pending update only when {@code downloadId} still matches the
     * stored pending id, so a stale completion can never wipe a newer
     * download's transaction state.
     */
    static boolean clearPending(Context context, long downloadId) {
        synchronized (PREFERENCES_LOCK) {
            // NO_DOWNLOAD is not a wildcard: an id-scoped clear must name the
            // live pending download or it touches nothing.
            if (downloadId == NO_DOWNLOAD || !isCurrentPending(context, downloadId)) {
                return false;
            }
            SharedPreferences preferences = preferences(context);
            String verifiedPath = preferences.getString(VERIFIED_APK_PATH, "");
            if (!verifiedPath.isEmpty()) {
                File updatesDir = new File(context.getFilesDir(), "updates");
                File verified = new File(verifiedPath);
                try {
                    if (verified.getCanonicalFile().getParentFile().equals(updatesDir.getCanonicalFile())) {
                        verified.delete();
                    }
                } catch (Exception ignored) {
                    // Never delete a path that cannot be proven to belong to the update directory.
                }
            }
            preferences.edit()
                .remove(PENDING_APK_DOWNLOAD_ID)
                .remove(EXPECTED_SHA256)
                .remove(EXPECTED_SIZE)
                .remove(EXPECTED_VERSION_CODE)
                .remove(VERIFIED_DOWNLOAD_ID)
                .remove(VERIFIED_APK_PATH)
                .remove(INSTALLER_LAUNCHED_DOWNLOAD_ID)
                .remove(WAITING_FOR_INSTALL_PERMISSION)
                .apply();
            return true;
        }
    }

    static void clearPending(Context context) {
        clearPending(context, pendingDownloadId(context));
    }

    private static UpdateManifest fetchManifest(int source) throws Exception {
        HttpURLConnection connection = null;
        try {
            String base = SourceRegistry.URLS[source];
            connection = (HttpURLConnection) new URL(base + SourceRegistry.UPDATE_MANIFEST_PATH)
                .openConnection();
            connection.setRequestMethod("GET");
            connection.setConnectTimeout(3500);
            connection.setReadTimeout(7000);
            connection.setUseCaches(false);
            connection.setRequestProperty("Accept", "application/json");
            int status = connection.getResponseCode();
            if (status < 200 || status >= 300) {
                throw new IllegalStateException("更新服务返回 " + status);
            }
            String body;
            try (InputStream input = connection.getInputStream();
                 BufferedReader reader = new BufferedReader(
                     new InputStreamReader(input, StandardCharsets.UTF_8))) {
                StringBuilder builder = new StringBuilder();
                String line;
                while ((line = reader.readLine()) != null && builder.length() < 64 * 1024) {
                    builder.append(line);
                }
                body = builder.toString();
            }
            JSONObject json = new JSONObject(body);
            String version = json.getString("version");
            long versionCode = json.getLong("version_code");
            String platform = json.getString("platform");
            String arch = json.getString("arch");
            String file = json.getString("file");
            String sha256 = json.getString("sha256");
            long size = json.getLong("size");
            String downloadUrl = json.getString("download_url");
            if (!VERSION.matcher(version).matches()
                || versionCode <= 0
                || !"android".equals(platform)
                || !"universal".equals(arch)
                || file.contains("/")
                || file.contains("\\")
                || !file.endsWith(".apk")
                || !SHA256.matcher(sha256).matches()
                || size <= 0
                || size > MAX_APK_SIZE
                || !downloadUrl.startsWith("/api/artifacts/download/")) {
                throw new IllegalStateException("更新清单格式无效");
            }
            String notes = json.optString("release_notes", "");
            return new UpdateManifest(
                source,
                version,
                versionCode,
                file,
                sha256,
                size,
                notes,
                base.substring(0, base.length() - 1) + downloadUrl
            );
        } finally {
            if (connection != null) {
                connection.disconnect();
            }
        }
    }

    static final class UpdateManifest {
        final int source;
        final String version;
        final long versionCode;
        final String file;
        final String sha256;
        final long size;
        final String notes;
        final String url;

        UpdateManifest(
            int source,
            String version,
            long versionCode,
            String file,
            String sha256,
            long size,
            String notes,
            String url
        ) {
            this.source = source;
            this.version = version;
            this.versionCode = versionCode;
            this.file = file;
            this.sha256 = sha256;
            this.size = size;
            this.notes = notes;
            this.url = url;
        }
    }
}
