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

    interface Callback {
        void onSuccess(UpdateManifest manifest);
        void onError(String message);
    }

    private UpdateManager() {}

    static void check(Context context, int preferredSource, Callback callback) {
        EXECUTOR.execute(() -> {
            String lastError = context.getString(R.string.update_check_failed);
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
        preferences(context).edit()
            .putLong(PENDING_APK_DOWNLOAD_ID, downloadId)
            .putString(EXPECTED_SHA256, manifest.sha256)
            .putLong(EXPECTED_SIZE, manifest.size)
            .putLong(EXPECTED_VERSION_CODE, manifest.versionCode)
            .putLong(VERIFIED_DOWNLOAD_ID, NO_DOWNLOAD)
            .putLong(INSTALLER_LAUNCHED_DOWNLOAD_ID, NO_DOWNLOAD)
            .putBoolean(WAITING_FOR_INSTALL_PERMISSION, false)
            .apply();
        return downloadId;
    }

    static SharedPreferences preferences(Context context) {
        return context.getSharedPreferences(DOWNLOAD_PREFERENCES, Context.MODE_PRIVATE);
    }

    static void clearPending(Context context) {
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
