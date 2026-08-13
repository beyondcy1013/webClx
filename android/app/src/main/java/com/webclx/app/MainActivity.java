package com.webclx.app;

import android.Manifest;
import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.AlertDialog;
import android.app.DownloadManager;
import android.content.ActivityNotFoundException;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.pm.Signature;
import android.content.res.TypedArray;
import android.database.Cursor;
import android.graphics.Color;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.CookieManager;
import android.webkit.JavascriptInterface;
import android.webkit.URLUtil;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;
import android.widget.Toast;

import androidx.core.content.FileProvider;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.CompletionService;
import java.util.concurrent.ExecutorCompletionService;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

public final class MainActivity extends Activity {
    static final String APK_MIME_TYPE = "application/vnd.android.package-archive";
    private static final int FILE_CHOOSER_REQUEST = 7001;
    private static final int STORAGE_PERMISSION_REQUEST = 7002;

    private final ExecutorService resolverExecutor = Executors.newSingleThreadExecutor();
    private final ExecutorService probeExecutor =
        Executors.newFixedThreadPool(SourceRegistry.URLS.length);
    private final boolean[] rejectedSources = new boolean[SourceRegistry.URLS.length];
    private final BroadcastReceiver downloadCompleteReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (!DownloadManager.ACTION_DOWNLOAD_COMPLETE.equals(intent.getAction())) {
                return;
            }
            long downloadId = intent.getLongExtra(
                DownloadManager.EXTRA_DOWNLOAD_ID,
                UpdateManager.NO_DOWNLOAD
            );
            if (downloadId != pendingApkDownloadId()) {
                return;
            }
            if (activityResumed) {
                installPendingApk(true);
            }
        }
    };
    private WebView webView;
    private View connectionPanel;
    private ProgressBar connectionProgress;
    private TextView connectionMessage;
    private Button retryButton;
    private ValueCallback<Uri[]> fileChooserCallback;
    private PendingDownload pendingDownload;
    private int selectedSource = -1;
    private String selectedSourceOrigin;
    private boolean sourceResolutionInProgress;
    private boolean sourceReevaluationInProgress;
    private boolean activityResumed;
    private boolean downloadReceiverRegistered;
    private boolean automaticUpdateCheckStarted;
    private boolean updateVerificationInProgress;
    private AlertDialog updateProgressDialog;
    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private String createdThemeMode;
    private ConnectivityManager connectivityManager;
    private ConnectivityManager.NetworkCallback networkCallback;
    private volatile boolean destroyed;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        createdThemeMode = AppPreferences.themeMode(this);
        AppPreferences.applyTheme(this);
        super.onCreate(savedInstanceState);

        int contentBackgroundColor = resolveWindowBackgroundColor();
        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(contentBackgroundColor);

        webView = new WebView(this);
        webView.setBackgroundColor(contentBackgroundColor);
        root.addView(webView, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        ));
        connectionPanel = createConnectionPanel();
        root.addView(connectionPanel, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        ));
        setContentView(root);

        registerDownloadCompleteReceiver();
        configureWebView();
        registerNetworkCallback();
        beginSourceResolution(true);
    }

    private int resolveWindowBackgroundColor() {
        TypedArray attributes = obtainStyledAttributes(
            new int[]{android.R.attr.windowBackground}
        );
        try {
            return attributes.getColor(0, Color.rgb(247, 248, 250));
        } finally {
            attributes.recycle();
        }
    }

    private void registerDownloadCompleteReceiver() {
        IntentFilter filter = new IntentFilter(DownloadManager.ACTION_DOWNLOAD_COMPLETE);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            registerReceiver(downloadCompleteReceiver, filter, Context.RECEIVER_EXPORTED);
        } else {
            registerReceiver(downloadCompleteReceiver, filter);
        }
        downloadReceiverRegistered = true;
    }

    private SharedPreferences downloadPreferences() {
        return UpdateManager.preferences(this);
    }

    private long pendingApkDownloadId() {
        return downloadPreferences().getLong(
            UpdateManager.PENDING_APK_DOWNLOAD_ID,
            UpdateManager.NO_DOWNLOAD
        );
    }

    private void rememberPendingApkDownload(long downloadId) {
        downloadPreferences().edit()
            .putLong(UpdateManager.PENDING_APK_DOWNLOAD_ID, downloadId)
            .putBoolean(UpdateManager.WAITING_FOR_INSTALL_PERMISSION, false)
            .remove(UpdateManager.EXPECTED_SHA256)
            .remove(UpdateManager.EXPECTED_SIZE)
            .remove(UpdateManager.EXPECTED_VERSION_CODE)
            .remove(UpdateManager.VERIFIED_DOWNLOAD_ID)
            .remove(UpdateManager.INSTALLER_LAUNCHED_DOWNLOAD_ID)
            .apply();
    }

    private void clearPendingApkDownload() {
        UpdateManager.clearPending(this);
    }

    private boolean installPendingApk(boolean requestPermission) {
        long downloadId = pendingApkDownloadId();
        if (downloadId == UpdateManager.NO_DOWNLOAD) {
            return false;
        }
        long expectedVersion = downloadPreferences().getLong(
            UpdateManager.EXPECTED_VERSION_CODE,
            -1
        );
        if (expectedVersion > 0 && expectedVersion <= UpdateManager.currentVersionCode(this)) {
            UpdateManager.clearPending(this);
            return false;
        }

        DownloadManager manager = downloadManager();
        int status = downloadStatus(manager, downloadId);
        if (status == DownloadManager.STATUS_FAILED) {
            clearPendingApkDownload();
            Toast.makeText(this, R.string.apk_download_failed, Toast.LENGTH_LONG).show();
            return true;
        }
        if (status != DownloadManager.STATUS_SUCCESSFUL) {
            return false;
        }

        SharedPreferences preferences = downloadPreferences();
        long installerLaunchedDownloadId = preferences.getLong(
            UpdateManager.INSTALLER_LAUNCHED_DOWNLOAD_ID,
            UpdateManager.NO_DOWNLOAD
        );
        if (installerLaunchedDownloadId == downloadId) {
            return true;
        }
        String expectedSha256 = preferences.getString(UpdateManager.EXPECTED_SHA256, "");
        long verifiedDownloadId = preferences.getLong(
            UpdateManager.VERIFIED_DOWNLOAD_ID,
            UpdateManager.NO_DOWNLOAD
        );
        if (!expectedSha256.isEmpty() && verifiedDownloadId != downloadId) {
            verifyPendingUpdate(downloadId, requestPermission);
            return true;
        }

        String downloadedMimeType = manager.getMimeTypeForDownloadedFile(downloadId);
        String verifiedPath = preferences.getString(UpdateManager.VERIFIED_APK_PATH, "");
        Uri apkUri = !verifiedPath.isEmpty()
            ? FileProvider.getUriForFile(
                this,
                getPackageName() + ".updateprovider",
                new File(verifiedPath)
            )
            : manager.getUriForDownloadedFile(downloadId);
        if (!APK_MIME_TYPE.equalsIgnoreCase(downloadedMimeType) || apkUri == null) {
            clearPendingApkDownload();
            Toast.makeText(this, R.string.invalid_apk_download, Toast.LENGTH_LONG).show();
            return true;
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
            && !getPackageManager().canRequestPackageInstalls()) {
            if (requestPermission) {
                requestInstallPermission();
            }
            return true;
        }

        Intent installIntent = new Intent(Intent.ACTION_VIEW);
        installIntent.setDataAndType(apkUri, APK_MIME_TYPE);
        installIntent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try {
            preferences.edit()
                .putLong(UpdateManager.INSTALLER_LAUNCHED_DOWNLOAD_ID, downloadId)
                .apply();
            startActivity(installIntent);
            if (updateProgressDialog != null) {
                updateProgressDialog.dismiss();
                updateProgressDialog = null;
            }
        } catch (ActivityNotFoundException error) {
            preferences.edit()
                .remove(UpdateManager.INSTALLER_LAUNCHED_DOWNLOAD_ID)
                .apply();
            Toast.makeText(this, R.string.apk_installer_unavailable, Toast.LENGTH_LONG).show();
        }
        return true;
    }

    private void verifyPendingUpdate(long downloadId, boolean requestPermission) {
        if (updateVerificationInProgress) {
            return;
        }
        updateVerificationInProgress = true;
        Toast.makeText(this, R.string.update_verifying, Toast.LENGTH_SHORT).show();
        resolverExecutor.execute(() -> {
            int error = validatePendingUpdate(downloadId);
            runOnUiThread(() -> {
                updateVerificationInProgress = false;
                if (error != 0) {
                    UpdateManager.clearPending(this);
                    if (updateProgressDialog != null) {
                        updateProgressDialog.dismiss();
                        updateProgressDialog = null;
                    }
                    Toast.makeText(this, error, Toast.LENGTH_LONG).show();
                    return;
                }
                downloadPreferences().edit()
                    .putLong(UpdateManager.VERIFIED_DOWNLOAD_ID, downloadId)
                    .apply();
                installPendingApk(requestPermission);
            });
        });
    }

    private int validatePendingUpdate(long downloadId) {
        SharedPreferences preferences = downloadPreferences();
        String expectedSha256 = preferences.getString(UpdateManager.EXPECTED_SHA256, "");
        long expectedSize = preferences.getLong(UpdateManager.EXPECTED_SIZE, -1);
        long expectedVersionCode = preferences.getLong(UpdateManager.EXPECTED_VERSION_CODE, -1);
        Uri source = downloadManager().getUriForDownloadedFile(downloadId);
        if (source == null || expectedSize <= 0 || expectedVersionCode <= 0) {
            return R.string.update_integrity_failed;
        }
        File updateDir = new File(getFilesDir(), "updates");
        File staged = new File(updateDir, "verified-webclx-update.apk");
        try {
            if (!updateDir.isDirectory() && !updateDir.mkdirs()) {
                return R.string.update_integrity_failed;
            }
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            long copied = 0;
            try (InputStream input = getContentResolver().openInputStream(source);
                 FileOutputStream output = new FileOutputStream(staged, false)) {
                if (input == null) {
                    return R.string.update_integrity_failed;
                }
                byte[] buffer = new byte[32 * 1024];
                int read;
                while ((read = input.read(buffer)) >= 0) {
                    copied += read;
                    if (copied > expectedSize) {
                        return R.string.update_integrity_failed;
                    }
                    digest.update(buffer, 0, read);
                    output.write(buffer, 0, read);
                }
                output.getFD().sync();
            }
            if (copied != expectedSize || !hex(digest.digest()).equals(expectedSha256)) {
                return R.string.update_integrity_failed;
            }
            int flags = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P
                ? PackageManager.GET_SIGNING_CERTIFICATES
                : PackageManager.GET_SIGNATURES;
            PackageInfo archive = getPackageManager().getPackageArchiveInfo(
                staged.getAbsolutePath(),
                flags
            );
            PackageInfo installed = getPackageManager().getPackageInfo(getPackageName(), flags);
            if (archive == null
                || !getPackageName().equals(archive.packageName)
                || packageVersionCode(archive) != expectedVersionCode
                || expectedVersionCode <= packageVersionCode(installed)
                || !signatureDigests(archive).equals(signatureDigests(installed))) {
                return R.string.update_identity_failed;
            }
            downloadPreferences().edit()
                .putString(UpdateManager.VERIFIED_APK_PATH, staged.getAbsolutePath())
                .apply();
            return 0;
        } catch (Exception error) {
            return R.string.update_integrity_failed;
        } finally {
            if (!downloadPreferences().contains(UpdateManager.VERIFIED_APK_PATH)
                && staged.exists()) {
                staged.delete();
            }
        }
    }

    private long packageVersionCode(PackageInfo info) {
        return Build.VERSION.SDK_INT >= Build.VERSION_CODES.P
            ? info.getLongVersionCode()
            : info.versionCode;
    }

    private Set<String> signatureDigests(PackageInfo info) throws Exception {
        Signature[] signatures;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            if (info.signingInfo == null) {
                return java.util.Collections.emptySet();
            }
            signatures = info.signingInfo.getApkContentsSigners();
        } else {
            signatures = info.signatures;
        }
        Set<String> digests = new HashSet<>();
        if (signatures != null) {
            for (Signature signature : signatures) {
                digests.add(hex(MessageDigest.getInstance("SHA-256").digest(signature.toByteArray())));
            }
        }
        return digests;
    }

    private String hex(byte[] bytes) {
        StringBuilder builder = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) {
            builder.append(String.format(java.util.Locale.ROOT, "%02x", value & 0xff));
        }
        return builder.toString();
    }

    private int downloadStatus(DownloadManager manager, long downloadId) {
        DownloadManager.Query query = new DownloadManager.Query().setFilterById(downloadId);
        try (Cursor cursor = manager.query(query)) {
            if (cursor == null || !cursor.moveToFirst()) {
                clearPendingApkDownload();
                return -1;
            }
            int statusColumn = cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS);
            return cursor.getInt(statusColumn);
        }
    }

    private void requestInstallPermission() {
        Intent settingsIntent = new Intent(
            Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
            Uri.parse("package:" + getPackageName())
        );
        downloadPreferences().edit()
            .putBoolean(UpdateManager.WAITING_FOR_INSTALL_PERMISSION, true)
            .apply();
        try {
            startActivity(settingsIntent);
        } catch (ActivityNotFoundException error) {
            downloadPreferences().edit()
                .putBoolean(UpdateManager.WAITING_FOR_INSTALL_PERMISSION, false)
                .apply();
            Toast.makeText(
                this,
                R.string.install_permission_settings_unavailable,
                Toast.LENGTH_LONG
            ).show();
        }
    }

    private void resumePendingInstallPermission() {
        SharedPreferences preferences = downloadPreferences();
        if (!preferences.getBoolean(UpdateManager.WAITING_FOR_INSTALL_PERMISSION, false)) {
            return;
        }
        preferences.edit()
            .putBoolean(UpdateManager.WAITING_FOR_INSTALL_PERMISSION, false)
            .apply();
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O
            || getPackageManager().canRequestPackageInstalls()) {
            installPendingApk(false);
        } else {
            Toast.makeText(this, R.string.install_permission_denied, Toast.LENGTH_LONG).show();
        }
    }

    private DownloadManager downloadManager() {
        return (DownloadManager) getSystemService(Context.DOWNLOAD_SERVICE);
    }

    private View createConnectionPanel() {
        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setGravity(Gravity.CENTER);
        panel.setPadding(dp(32), dp(32), dp(32), dp(32));
        panel.setBackgroundColor(Color.rgb(247, 248, 250));

        ImageView icon = new ImageView(this);
        icon.setImageResource(R.drawable.ic_launcher);
        LinearLayout.LayoutParams iconParams = new LinearLayout.LayoutParams(dp(64), dp(64));
        iconParams.bottomMargin = dp(18);
        panel.addView(icon, iconParams);

        TextView title = new TextView(this);
        title.setText(R.string.app_name);
        title.setTextColor(Color.rgb(30, 35, 42));
        title.setTextSize(24);
        title.setGravity(Gravity.CENTER);
        LinearLayout.LayoutParams titleParams = new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT
        );
        titleParams.bottomMargin = dp(16);
        panel.addView(title, titleParams);

        connectionProgress = new ProgressBar(this);
        LinearLayout.LayoutParams progressParams = new LinearLayout.LayoutParams(dp(32), dp(32));
        progressParams.bottomMargin = dp(14);
        panel.addView(connectionProgress, progressParams);

        connectionMessage = new TextView(this);
        connectionMessage.setText(R.string.connecting);
        connectionMessage.setTextColor(Color.rgb(86, 94, 104));
        connectionMessage.setTextSize(15);
        connectionMessage.setGravity(Gravity.CENTER);
        panel.addView(connectionMessage);

        retryButton = new Button(this);
        retryButton.setText(R.string.retry);
        retryButton.setAllCaps(false);
        retryButton.setVisibility(View.GONE);
        retryButton.setOnClickListener(view -> beginSourceResolution(true));
        LinearLayout.LayoutParams retryParams = new LinearLayout.LayoutParams(dp(120), dp(48));
        retryParams.topMargin = dp(18);
        panel.addView(retryButton, retryParams);
        return panel;
    }

    @SuppressLint("SetJavaScriptEnabled")
    private void configureWebView() {
        WebSettings settings = webView.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setDatabaseEnabled(true);
        settings.setAllowFileAccess(false);
        settings.setAllowContentAccess(true);
        settings.setMediaPlaybackRequiresUserGesture(false);
        settings.setCacheMode(WebSettings.LOAD_DEFAULT);
        settings.setUserAgentString(
            settings.getUserAgentString() + " webClxAndroid/" + UpdateManager.currentVersionName(this)
        );
        settings.setTextZoom(100);

        CookieManager cookies = CookieManager.getInstance();
        cookies.setAcceptCookie(true);
        cookies.setAcceptThirdPartyCookies(webView, false);

        webView.addJavascriptInterface(new AndroidWebViewBridge(), "WebClxAndroid");

        webView.setWebChromeClient(new WebChromeClient() {
            @Override
            public boolean onShowFileChooser(
                WebView view,
                ValueCallback<Uri[]> callback,
                FileChooserParams params
            ) {
                if (fileChooserCallback != null) {
                    fileChooserCallback.onReceiveValue(null);
                }
                fileChooserCallback = callback;
                try {
                    startActivityForResult(params.createIntent(), FILE_CHOOSER_REQUEST);
                    return true;
                } catch (RuntimeException error) {
                    fileChooserCallback = null;
                    Toast.makeText(MainActivity.this, "无法打开文件选择器", Toast.LENGTH_LONG).show();
                    return false;
                }
            }
        });
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                return handleNavigation(request.getUrl());
            }

            @Override
            public void onPageFinished(WebView view, String url) {
                if (!sourceResolutionInProgress && isSelectedSourceUrl(url)) {
                    applyWebPreferences();
                    connectionPanel.setVisibility(View.GONE);
                    webView.setVisibility(View.VISIBLE);
                    maybeCheckForUpdate();
                }
            }

            @Override
            public void onReceivedError(
                WebView view,
                WebResourceRequest request,
                WebResourceError error
            ) {
                if (request.isForMainFrame()
                    && isSelectedSourceUrl(request.getUrl().toString())) {
                    retryAlternateSource();
                }
            }

            @Override
            public void onReceivedHttpError(
                WebView view,
                WebResourceRequest request,
                WebResourceResponse response
            ) {
                if (request.isForMainFrame()
                    && response.getStatusCode() >= 500
                    && isSelectedSourceUrl(request.getUrl().toString())) {
                    retryAlternateSource();
                }
            }
        });
        webView.setDownloadListener(this::requestDownload);
        settings.setTextZoom(AppPreferences.textZoom(this));
    }

    private void applyWebPreferences() {
        String requested = AppPreferences.themeMode(this);
        String effective = requested;
        if (AppPreferences.THEME_SYSTEM.equals(requested)) {
            int nightMode = getResources().getConfiguration().uiMode
                & android.content.res.Configuration.UI_MODE_NIGHT_MASK;
            effective = nightMode == android.content.res.Configuration.UI_MODE_NIGHT_YES
                ? AppPreferences.THEME_DARK
                : AppPreferences.THEME_LIGHT;
        }
        String storage = AppPreferences.THEME_SYSTEM.equals(requested)
            ? "localStorage.removeItem('webclx:theme-mode');"
            : "localStorage.setItem('webclx:theme-mode','" + requested + "');";
        webView.evaluateJavascript(
            "(()=>{" + storage
                + "document.documentElement.dataset.theme='" + effective + "';"
                + "document.documentElement.style.colorScheme='" + effective + "';})()",
            null
        );
    }

    private void maybeCheckForUpdate() {
        if (automaticUpdateCheckStarted || !AppPreferences.autoUpdate(this)) {
            return;
        }
        automaticUpdateCheckStarted = true;
        UpdateManager.check(this, selectedSource, new UpdateManager.Callback() {
            @Override
            public void onSuccess(UpdateManager.UpdateManifest manifest) {
                if (!UpdateManager.isNewer(MainActivity.this, manifest)) {
                    return;
                }
                runOnUiThread(() -> showUpdatePrompt(manifest));
            }

            @Override
            public void onError(String message) {
                // Automatic checks stay quiet; manual checks expose their error in Settings.
            }
        });
    }

    private void showUpdatePrompt(UpdateManager.UpdateManifest manifest) {
        String notes = manifest.notes.trim().isEmpty()
            ? getString(R.string.update_available, manifest.version)
            : manifest.notes.trim();
        new AlertDialog.Builder(this)
            .setTitle(getString(R.string.update_prompt_title, manifest.version))
            .setMessage(getString(
                R.string.update_prompt_message,
                notes,
                formatBytes(manifest.size)
            ))
            .setNegativeButton(R.string.update_later, null)
            .setPositiveButton(R.string.update_now, (dialog, which) -> {
                long downloadId = UpdateManager.enqueue(this, manifest);
                showUpdateProgress(downloadId);
            })
            .show();
    }

    private void showUpdateProgress(long downloadId) {
        ProgressBar progress = new ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal);
        progress.setMax(100);
        progress.setIndeterminate(true);
        progress.setPadding(dp(20), dp(16), dp(20), dp(8));
        updateProgressDialog = new AlertDialog.Builder(this)
            .setTitle(R.string.update_downloading)
            .setView(progress)
            .setNegativeButton(R.string.update_later, null)
            .create();
        updateProgressDialog.show();
        Runnable poll = new Runnable() {
            @Override
            public void run() {
                if (updateProgressDialog == null || !updateProgressDialog.isShowing()) {
                    return;
                }
                DownloadManager.Query query = new DownloadManager.Query().setFilterById(downloadId);
                try (Cursor cursor = downloadManager().query(query)) {
                    if (cursor == null || !cursor.moveToFirst()) {
                        updateProgressDialog.dismiss();
                        return;
                    }
                    int status = cursor.getInt(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS));
                    long downloaded = cursor.getLong(
                        cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR)
                    );
                    long total = cursor.getLong(
                        cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_TOTAL_SIZE_BYTES)
                    );
                    if (total > 0) {
                        progress.setIndeterminate(false);
                        progress.setProgress((int) Math.min(100, downloaded * 100 / total));
                    }
                    if (status == DownloadManager.STATUS_SUCCESSFUL) {
                        updateProgressDialog.setTitle(R.string.update_verifying);
                        progress.setIndeterminate(true);
                        return;
                    }
                    if (status == DownloadManager.STATUS_FAILED) {
                        updateProgressDialog.dismiss();
                        Toast.makeText(MainActivity.this, R.string.apk_download_failed, Toast.LENGTH_LONG).show();
                        return;
                    }
                }
                mainHandler.postDelayed(this, 500);
            }
        };
        mainHandler.post(poll);
    }

    private String formatBytes(long bytes) {
        if (bytes >= 1024L * 1024L) {
            return String.format(java.util.Locale.ROOT, "%.1f MB", bytes / 1024d / 1024d);
        }
        return String.format(java.util.Locale.ROOT, "%.1f KB", bytes / 1024d);
    }

    private void registerNetworkCallback() {
        connectivityManager = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
        if (connectivityManager == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.N) {
            return;
        }
        networkCallback = new ConnectivityManager.NetworkCallback() {
            @Override
            public void onAvailable(Network network) {
                requestSourceReevaluation();
            }

            @Override
            public void onLost(Network network) {
                // Re-probe after the replacement network is reported by Android.
                requestSourceReevaluation();
            }

            @Override
            public void onCapabilitiesChanged(Network network, NetworkCapabilities capabilities) {
                if (capabilities != null
                        && capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)) {
                    requestSourceReevaluation();
                }
            }
        };
        try {
            connectivityManager.registerDefaultNetworkCallback(networkCallback);
        } catch (RuntimeException ignored) {
            networkCallback = null;
        }
    }

    private void unregisterNetworkCallback() {
        if (connectivityManager == null || networkCallback == null) {
            return;
        }
        try {
            connectivityManager.unregisterNetworkCallback(networkCallback);
        } catch (RuntimeException ignored) {
        }
        networkCallback = null;
    }

    private static final long REEVALUATION_DEBOUNCE_MS = 2000;
    private long lastReevaluationAt;

    /**
     * Called by the network callback when connectivity changes.
     * If we already have a working connection, silently probe the current
     * server first — only re-select if it is actually unreachable.
     * This avoids restarting the page when the network merely switches
     * but the current server is still perfectly reachable.
     */
    private void requestSourceReevaluation() {
        mainHandler.post(this::reevaluateCurrentSource);
    }

    private void reevaluateCurrentSource() {
        if (destroyed || webView == null) {
            return;
        }
        if (sourceResolutionInProgress || sourceReevaluationInProgress) {
            return;
        }
        long now = System.currentTimeMillis();
        if (now - lastReevaluationAt < REEVALUATION_DEBOUNCE_MS) {
            return;
        }
        lastReevaluationAt = now;

        int currentSource = selectedSource;
        String currentOrigin = selectedSourceOrigin;
        if (currentOrigin != null && !currentOrigin.isEmpty()) {
            sourceReevaluationInProgress = true;
            resolverExecutor.execute(() -> {
                SourceRegistry.ProbeResult result = SourceRegistry.isValidIndex(currentSource)
                    ? SourceRegistry.probe(currentSource)
                    : SourceRegistry.probeUrl(currentOrigin);
                runOnUiThread(() -> {
                    sourceReevaluationInProgress = false;
                    if (destroyed) {
                        return;
                    }
                    if (!currentOrigin.equals(selectedSourceOrigin)) {
                        return;
                    }
                    if (result.healthy) {
                        // Current server still works — stay put, don't reload.
                        return;
                    }
                    // Keep the current page visible while alternatives are selected.
                    Arrays.fill(rejectedSources, false);
                    resolveAndLoad(currentSource, false);
                });
            });
            return;
        }
        beginSourceResolution(true);
    }

    private void beginSourceResolution(boolean foreground) {
        if (sourceResolutionInProgress || sourceReevaluationInProgress) {
            return;
        }
        Arrays.fill(rejectedSources, false);
        int preferred = AppPreferences.preferredSource(this);
        if (preferred >= 0) {
            sourceResolutionInProgress = true;
            if (foreground) {
                showConnectingState();
            }
            resolverExecutor.execute(() -> {
                SourceRegistry.ProbeResult result = SourceRegistry.probe(preferred);
                runOnUiThread(() -> {
                    sourceResolutionInProgress = false;
                    if (result.healthy) {
                        loadSource(preferred);
                    } else {
                        resolveAndLoad(preferred, foreground);
                    }
                });
            });
            return;
        }
        resolveAndLoad(-1, foreground);
    }

    private void resolveAndLoad(int excludedSource, boolean foreground) {
        if (sourceResolutionInProgress) {
            return;
        }
        if (excludedSource >= 0) {
            rejectedSources[excludedSource] = true;
        }
        sourceResolutionInProgress = true;
        if (foreground) {
            showConnectingState();
        }

        resolverExecutor.execute(() -> {
            CompletionService<Integer> completion =
                new ExecutorCompletionService<>(probeExecutor);
            List<Future<Integer>> probes = new ArrayList<>();
            for (int index = 0; index < SourceRegistry.URLS.length; index++) {
                if (rejectedSources[index]) {
                    continue;
                }
                int source = index;
                probes.add(completion.submit(() -> {
                    SourceRegistry.ProbeResult result = SourceRegistry.probe(source);
                    return result.healthy ? source : -(source + 1);
                }));
            }

            for (int pending = probes.size(); pending > 0; pending--) {
                try {
                    int result = completion.take().get();
                    if (result >= 0) {
                        cancelProbes(probes);
                        runOnUiThread(() -> loadSource(result));
                        return;
                    }
                    rejectedSources[-result - 1] = true;
                } catch (InterruptedException error) {
                    cancelProbes(probes);
                    Thread.currentThread().interrupt();
                    return;
                } catch (Exception ignored) {
                    // A failed probe cannot win; keep waiting for the remaining candidates.
                }
            }
            runOnUiThread(() -> {
                if (foreground) {
                    showConnectionFailure();
                } else {
                    sourceResolutionInProgress = false;
                }
            });
        });
    }

    private void showConnectingState() {
        selectedSource = -1;
        webView.stopLoading();
        webView.setVisibility(View.GONE);
        connectionPanel.setVisibility(View.VISIBLE);
        connectionProgress.setVisibility(View.VISIBLE);
        connectionMessage.setText(R.string.connecting);
        retryButton.setVisibility(View.GONE);
    }

    private void cancelProbes(List<Future<Integer>> probes) {
        for (Future<Integer> probe : probes) {
            probe.cancel(true);
        }
    }

    private void loadSource(int source) {
        sourceResolutionInProgress = false;
        selectedSource = source;
        selectedSourceOrigin = source >= 0 && source < SourceRegistry.URLS.length
            ? SourceRegistry.URLS[source]
            : null;
        connectionMessage.setText(R.string.connecting);
        AppPreferences.get(this).edit()
            .putInt(AppPreferences.KEY_ACTIVE_SOURCE, source)
            .putLong(AppPreferences.KEY_ACTIVE_SOURCE_AT, System.currentTimeMillis())
            .apply();
        webView.loadUrl(SourceRegistry.URLS[source] + AppPreferences.startPath(this));
    }

    private void showConnectionFailure() {
        sourceResolutionInProgress = false;
        selectedSource = -1;
        selectedSourceOrigin = null;
        connectionProgress.setVisibility(View.GONE);
        connectionMessage.setText(R.string.connection_failed);
        retryButton.setVisibility(View.VISIBLE);
    }

    private void retryAlternateSource() {
        int failedSource = selectedSource;
        if (failedSource >= 0) {
            resolveAndLoad(failedSource, false);
        }
    }

    private boolean handleNavigation(Uri uri) {
        String scheme = uri.getScheme();
        if ("webclx".equalsIgnoreCase(scheme)) {
            if ("downloads".equalsIgnoreCase(uri.getHost())) {
                openDownloadManager();
            }
            return true;
        }
        if ("about".equalsIgnoreCase(scheme) || isTrustedOrigin(uri)) {
            return false;
        }
        if ("http".equalsIgnoreCase(scheme) || "https".equalsIgnoreCase(scheme)) {
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, uri));
            } catch (RuntimeException error) {
                Toast.makeText(this, "无法打开外部链接", Toast.LENGTH_LONG).show();
            }
        }
        return true;
    }

    private final class AndroidWebViewBridge {
        /**
         * Native HTTP probe for the terminal server-probe JS module.
         * Called from {@code TERMINAL_SERVER_PROBE.probeHostViaNative()}
         * to test cross-origin server reachability without CORS restrictions
         * and with accurate latency measurement.
         *
         * @param url     full URL to probe (e.g. http://192.168.3.2:11111/favicon.svg)
         * @param timeoutMs connect/read timeout in milliseconds
         * @return JSON object string: {"ok":true/false,"latency":<ms>} or null on error
         */
        @JavascriptInterface
        public String probeHost(String url, int timeoutMs) {
            java.net.HttpURLConnection connection = null;
            long startedAt = System.nanoTime();
            try {
                java.net.URL target = new java.net.URL(url);
                connection = (java.net.HttpURLConnection) target.openConnection();
                connection.setRequestMethod("GET");
                connection.setConnectTimeout(Math.max(500, timeoutMs));
                connection.setReadTimeout(Math.max(500, timeoutMs));
                connection.setInstanceFollowRedirects(true);
                connection.setUseCaches(false);
                int status = connection.getResponseCode();
                long latencyMs = Math.max(1, (System.nanoTime() - startedAt) / 1_000_000);
                // Any HTTP response (200, 303, 401, etc.) proves the server is alive.
                boolean reachable = status > 0;
                connection.disconnect();
                return "{\"ok\":" + reachable + ",\"latency\":" + latencyMs + "}";
            } catch (Exception error) {
                long latencyMs = Math.max(1, (System.nanoTime() - startedAt) / 1_000_000);
                return "{\"ok\":false,\"latency\":" + latencyMs + "}";
            } finally {
                if (connection != null) {
                    connection.disconnect();
                }
            }
        }

        @JavascriptInterface
        public void openInWebView(String rawUrl) {
            Uri target = Uri.parse(rawUrl);
            String scheme = target.getScheme();
            if (!("http".equalsIgnoreCase(scheme) || "https".equalsIgnoreCase(scheme))
                || target.getHost() == null
                || target.getHost().isEmpty()
                || target.getUserInfo() != null) {
                mainHandler.post(() ->
                    Toast.makeText(MainActivity.this, "服务器地址无效", Toast.LENGTH_LONG).show()
                );
                return;
            }
            mainHandler.post(() -> {
                selectedSource = sourceIndexForOrigin(target);
                selectedSourceOrigin = sourceOrigin(target);
                webView.loadUrl(target.toString());
            });
        }
    }

    private int sourceIndexForOrigin(Uri target) {
        for (int index = 0; index < SourceRegistry.URLS.length; index++) {
            Uri candidate = Uri.parse(SourceRegistry.URLS[index]);
            if (equalsIgnoreCase(target.getScheme(), candidate.getScheme())
                && equalsIgnoreCase(target.getHost(), candidate.getHost())
                && effectivePort(target) == effectivePort(candidate)) {
                return index;
            }
        }
        return -1;
    }

    private String sourceOrigin(Uri target) {
        return target.getScheme() + "://" + target.getHost() + ":" + effectivePort(target);
    }

    private void openDownloadManager() {
        Intent intent = new Intent(DownloadManager.ACTION_VIEW_DOWNLOADS);
        try {
            startActivity(intent);
        } catch (ActivityNotFoundException error) {
            Toast.makeText(this, R.string.download_manager_unavailable, Toast.LENGTH_LONG).show();
        }
    }

    private boolean isTrustedOrigin(Uri uri) {
        for (String candidate : SourceRegistry.URLS) {
            Uri trusted = Uri.parse(candidate);
            if (equalsIgnoreCase(uri.getScheme(), trusted.getScheme())
                && equalsIgnoreCase(uri.getHost(), trusted.getHost())
                && effectivePort(uri) == effectivePort(trusted)) {
                return true;
            }
        }
        return false;
    }

    private boolean isSelectedSourceUrl(String url) {
        if (selectedSourceOrigin == null || selectedSourceOrigin.isEmpty()) {
            return false;
        }
        Uri actual = Uri.parse(url);
        Uri selected = Uri.parse(selectedSourceOrigin);
        return equalsIgnoreCase(actual.getScheme(), selected.getScheme())
            && equalsIgnoreCase(actual.getHost(), selected.getHost())
            && effectivePort(actual) == effectivePort(selected);
    }

    private static boolean equalsIgnoreCase(String left, String right) {
        return left != null && right != null && left.equalsIgnoreCase(right);
    }

    private static int effectivePort(Uri uri) {
        if (uri.getPort() >= 0) {
            return uri.getPort();
        }
        return "https".equalsIgnoreCase(uri.getScheme()) ? 443 : 80;
    }

    private void requestDownload(
        String url,
        String userAgent,
        String contentDisposition,
        String mimeType,
        long contentLength
    ) {
        Uri uri = Uri.parse(url);
        if (!isTrustedOrigin(uri)) {
            handleNavigation(uri);
            return;
        }
        PendingDownload download = new PendingDownload(url, userAgent, contentDisposition, mimeType);
        if (Build.VERSION.SDK_INT <= Build.VERSION_CODES.P
            && checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE)
                != PackageManager.PERMISSION_GRANTED) {
            pendingDownload = download;
            requestPermissions(
                new String[]{Manifest.permission.WRITE_EXTERNAL_STORAGE},
                STORAGE_PERMISSION_REQUEST
            );
            return;
        }
        enqueueDownload(download);
    }

    private void enqueueDownload(PendingDownload download) {
        try {
            String filename = URLUtil.guessFileName(
                download.url,
                download.contentDisposition,
                download.mimeType
            ).replaceAll("[\\\\/:*?\"<>|\\p{Cntrl}]", "-");
            if (filename.trim().isEmpty()) {
                filename = "webclx-download";
            }
            DownloadManager.Request request = new DownloadManager.Request(Uri.parse(download.url));
            request.setTitle(filename);
            request.setNotificationVisibility(
                DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED
            );
            request.setVisibleInDownloadsUi(true);
            request.setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, filename);
            if (download.mimeType != null && !download.mimeType.isEmpty()) {
                request.setMimeType(download.mimeType);
            }
            if (download.userAgent != null && !download.userAgent.isEmpty()) {
                request.addRequestHeader("User-Agent", download.userAgent);
            }
            String cookie = CookieManager.getInstance().getCookie(download.url);
            if (cookie != null && !cookie.isEmpty()) {
                request.addRequestHeader("Cookie", cookie);
            }
            DownloadManager manager = downloadManager();
            long downloadId = manager.enqueue(request);
            if (isApkDownload(filename, download.mimeType)) {
                rememberPendingApkDownload(downloadId);
            }
            Toast.makeText(this, "已加入下载队列", Toast.LENGTH_SHORT).show();
        } catch (RuntimeException error) {
            Toast.makeText(this, "下载失败", Toast.LENGTH_LONG).show();
        }
    }

    private static boolean isApkDownload(String filename, String mimeType) {
        int extensionStart = filename.length() - 4;
        return extensionStart >= 0
            && filename.regionMatches(true, extensionStart, ".apk", 0, 4)
            && APK_MIME_TYPE.equalsIgnoreCase(mimeType);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != FILE_CHOOSER_REQUEST || fileChooserCallback == null) {
            return;
        }
        Uri[] result = WebChromeClient.FileChooserParams.parseResult(resultCode, data);
        fileChooserCallback.onReceiveValue(result);
        fileChooserCallback = null;
    }

    @Override
    public void onRequestPermissionsResult(
        int requestCode,
        String[] permissions,
        int[] grantResults
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode != STORAGE_PERMISSION_REQUEST || pendingDownload == null) {
            return;
        }
        PendingDownload download = pendingDownload;
        pendingDownload = null;
        if (grantResults.length > 0 && grantResults[0] == PackageManager.PERMISSION_GRANTED) {
            enqueueDownload(download);
        } else {
            Toast.makeText(this, "未获得下载目录权限", Toast.LENGTH_LONG).show();
        }
    }

    @Override
    public void onBackPressed() {
        if (webView.canGoBack()) {
            webView.goBack();
        } else {
            super.onBackPressed();
        }
    }

    @Override
    protected void onPause() {
        activityResumed = false;
        webView.onPause();
        super.onPause();
    }

    @Override
    protected void onResume() {
        super.onResume();
        activityResumed = true;
        webView.onResume();
        boolean returningFromInstallPermission = downloadPreferences().getBoolean(
            UpdateManager.WAITING_FOR_INSTALL_PERMISSION,
            false
        );
        resumePendingInstallPermission();
        if (!returningFromInstallPermission) {
            installPendingApk(true);
        }
        if (!createdThemeMode.equals(AppPreferences.themeMode(this))) {
            recreate();
            return;
        }
        webView.getSettings().setTextZoom(AppPreferences.textZoom(this));
    }

    @Override
    protected void onDestroy() {
        unregisterNetworkCallback();
        destroyed = true;
        resolverExecutor.shutdownNow();
        probeExecutor.shutdownNow();
        if (downloadReceiverRegistered) {
            unregisterReceiver(downloadCompleteReceiver);
            downloadReceiverRegistered = false;
        }
        if (fileChooserCallback != null) {
            fileChooserCallback.onReceiveValue(null);
            fileChooserCallback = null;
        }
        mainHandler.removeCallbacksAndMessages(null);
        if (updateProgressDialog != null) {
            updateProgressDialog.dismiss();
            updateProgressDialog = null;
        }
        webView.stopLoading();
        webView.destroy();
        super.onDestroy();
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    private static final class PendingDownload {
        final String url;
        final String userAgent;
        final String contentDisposition;
        final String mimeType;

        PendingDownload(String url, String userAgent, String contentDisposition, String mimeType) {
            this.url = url;
            this.userAgent = userAgent;
            this.contentDisposition = contentDisposition;
            this.mimeType = mimeType;
        }
    }
}
