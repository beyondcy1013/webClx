package com.webclx.app;

import android.Manifest;
import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.AlertDialog;
import android.app.DownloadManager;
import android.content.ActivityNotFoundException;
import android.content.BroadcastReceiver;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.SharedPreferences;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.pm.Signature;
import android.content.res.TypedArray;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Rect;
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
import android.view.WindowInsets;
import android.webkit.CookieManager;
import android.webkit.JavascriptInterface;
import android.webkit.RenderProcessGoneDetail;
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

import androidx.core.content.ContextCompat;
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
import java.util.concurrent.ScheduledExecutorService;

public final class MainActivity extends Activity {
    static final String APK_MIME_TYPE = "application/vnd.android.package-archive";
    private static final int HARNESS_PORT = 3080;
    private static final int FILE_CHOOSER_REQUEST = 7001;
    private static final int STORAGE_PERMISSION_REQUEST = 7002;

    private static final String STATE_SELECTED_SOURCE = "selected_source";
    private static final String STATE_SELECTED_ORIGIN = "selected_source_origin";
    private static final String STATE_LOADED_URL = "last_loaded_url";
    private static final int MAX_RENDER_PROCESS_RETRIES = 3;

    private final ExecutorService resolverExecutor = Executors.newSingleThreadExecutor();
    private final ExecutorService probeExecutor =
        Executors.newFixedThreadPool(SourceRegistry.URLS.length);
    private final ExecutorService updateVerifyExecutor = Executors.newSingleThreadExecutor();
    private final ScheduledExecutorService updatePollExecutor =
        Executors.newSingleThreadScheduledExecutor();
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
    private FrameLayout contentRoot;
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
    private boolean sourceReevaluationPending;
    private boolean activityResumed;
    private boolean webViewPausedSinceResume;
    private volatile boolean systemKeyboardVisible;
    private boolean downloadReceiverRegistered;
    private boolean automaticUpdateCheckStarted;
    private boolean updateVerificationInProgress;
    private AlertDialog updateProgressDialog;
    private ProgressBar updateProgressBar;
    private long updateProgressDownloadId = UpdateManager.NO_DOWNLOAD;
    private int updateProgressGeneration;
    private Future<?> updateCheckFuture;
    private UpdateManager.UpdateManifest pendingUpdateManifest;
    private int sourceSelectionGeneration;
    private String lastLoadedUrl;
    private int renderProcessFailures;
    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private String createdThemeMode;
    private ConnectivityManager connectivityManager;
    private ConnectivityManager.NetworkCallback networkCallback;
    private volatile boolean destroyed;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        createdThemeMode = AppPreferences.themeMode(this);
        AppPreferences.applyTheme(this);
        applyScreenOrientation(AppPreferences.screenOrientation(this));
        super.onCreate(savedInstanceState);

        int contentBackgroundColor = resolveWindowBackgroundColor();
        contentRoot = new FrameLayout(this);
        contentRoot.setBackgroundColor(contentBackgroundColor);

        webView = createWebView(contentBackgroundColor);
        contentRoot.addView(webView, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        ));
        connectionPanel = createConnectionPanel();
        contentRoot.addView(connectionPanel, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        ));
        setContentView(contentRoot);

        registerDownloadCompleteReceiver();
        configureWebView();
        registerNetworkCallback();
        if (!restoreInstanceState(savedInstanceState)) {
            beginSourceResolution(true);
        }
    }

    // minSdk is 24, so isDestroyed() (API 17) is always available.
    private boolean isActivityUsable() {
        return !destroyed && !isFinishing() && !isDestroyed();
    }

    private WebView createWebView(int backgroundColor) {
        WebView view = new WebView(this);
        view.setBackgroundColor(backgroundColor);
        bindKeyboardVisibility(view);
        return view;
    }

    private void bindKeyboardVisibility(WebView view) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // API 30+ reports the IME window directly; no geometry guessing.
            view.setOnApplyWindowInsetsListener((target, insets) -> {
                systemKeyboardVisible = insets.isVisible(WindowInsets.Type.ime());
                return target.onApplyWindowInsets(insets);
            });
            return;
        }
        view.getViewTreeObserver().addOnGlobalLayoutListener(() -> {
            Rect visibleFrame = new Rect();
            view.getWindowVisibleDisplayFrame(visibleFrame);
            int rootHeight = view.getRootView().getHeight();
            int obscuredHeight = Math.max(0, rootHeight - visibleFrame.bottom);
            systemKeyboardVisible = rootHeight > 0 && obscuredHeight > rootHeight * 0.15;
        });
    }

    private boolean restoreInstanceState(Bundle savedInstanceState) {
        if (savedInstanceState == null || !savedInstanceState.containsKey(STATE_LOADED_URL)) {
            return false;
        }
        String savedUrl = savedInstanceState.getString(STATE_LOADED_URL);
        String savedOrigin = savedInstanceState.getString(STATE_SELECTED_ORIGIN);
        int savedSource = savedInstanceState.getInt(STATE_SELECTED_SOURCE, -1);
        if (!isRestorableSelectedPage(savedUrl, savedOrigin, savedSource)) {
            return false;
        }
        // Only assign selection state after the bundle passed full validation.
        selectedSource = savedSource;
        selectedSourceOrigin = savedOrigin;
        lastLoadedUrl = savedUrl;
        connectionMessage.setText(R.string.connecting);
        webView.loadUrl(savedUrl);
        return true;
    }

    /**
     * A saved page is restorable only when it still belongs to the saved
     * selected origin — including custom sources picked via openInWebView —
     * or to the debug harness port on the selected host.  The persisted
     * source index must agree with the origin it names, and anything else
     * (other trusted sites, malformed URIs, user-info URLs) is rejected so
     * restoration never silently expands navigation scope.
     */
    private boolean isRestorableSelectedPage(String url, String origin, int sourceIndex) {
        if (url == null || url.isEmpty() || origin == null || origin.isEmpty()) {
            return false;
        }
        Uri selected = Uri.parse(origin);
        String selectedScheme = selected.getScheme();
        if (!("http".equalsIgnoreCase(selectedScheme) || "https".equalsIgnoreCase(selectedScheme))
            || selected.getHost() == null || selected.getHost().isEmpty()
            || selected.getUserInfo() != null) {
            return false;
        }
        if (sourceIndexForOrigin(selected) != sourceIndex) {
            return false;
        }
        Uri page = Uri.parse(url);
        String pageScheme = page.getScheme();
        if (!("http".equalsIgnoreCase(pageScheme) || "https".equalsIgnoreCase(pageScheme))
            || page.getHost() == null || page.getHost().isEmpty()
            || page.getUserInfo() != null) {
            return false;
        }
        if (equalsIgnoreCase(page.getScheme(), selected.getScheme())
            && equalsIgnoreCase(page.getHost(), selected.getHost())
            && effectivePort(page) == effectivePort(selected)) {
            return true;
        }
        return equalsIgnoreCase(page.getHost(), selected.getHost())
            && effectivePort(page) == HARNESS_PORT;
    }

    @Override
    protected void onSaveInstanceState(Bundle outState) {
        super.onSaveInstanceState(outState);
        outState.putInt(STATE_SELECTED_SOURCE, selectedSource);
        outState.putString(STATE_SELECTED_ORIGIN, selectedSourceOrigin);
        outState.putString(STATE_LOADED_URL, lastLoadedUrl);
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
        ContextCompat.registerReceiver(
            this,
            downloadCompleteReceiver,
            filter,
            ContextCompat.RECEIVER_EXPORTED
        );
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
        UpdateManager.mutatePreferences(this, editor -> editor
            .putLong(UpdateManager.PENDING_APK_DOWNLOAD_ID, downloadId)
            .putBoolean(UpdateManager.WAITING_FOR_INSTALL_PERMISSION, false)
            .remove(UpdateManager.EXPECTED_SHA256)
            .remove(UpdateManager.EXPECTED_SIZE)
            .remove(UpdateManager.EXPECTED_VERSION_CODE)
            .remove(UpdateManager.VERIFIED_DOWNLOAD_ID)
            .remove(UpdateManager.VERIFIED_APK_PATH)
            .remove(UpdateManager.INSTALLER_LAUNCHED_DOWNLOAD_ID));
    }

    private void clearPendingApkDownload(long downloadId) {
        UpdateManager.clearPending(this, downloadId);
    }

    private boolean installPendingApk(boolean requestPermission) {
        if (!isActivityUsable()) {
            return true;
        }
        if (!activityResumed) {
            // Nothing in this method may touch UI (installer, permission
            // settings, dialogs) while paused; a verified commit made in the
            // background is picked up by the next onResume.
            return true;
        }
        long downloadId = pendingApkDownloadId();
        if (downloadId == UpdateManager.NO_DOWNLOAD) {
            return false;
        }
        long expectedVersion = downloadPreferences().getLong(
            UpdateManager.EXPECTED_VERSION_CODE,
            -1
        );
        if (expectedVersion > 0 && expectedVersion <= UpdateManager.currentVersionCode(this)) {
            UpdateManager.clearPending(this, downloadId);
            return false;
        }

        DownloadManager manager = downloadManager();
        int status = downloadStatus(manager, downloadId);
        if (status == DownloadManager.STATUS_FAILED) {
            clearPendingApkDownload(downloadId);
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
        String verifiedPath = verifiedDownloadId == downloadId
            ? preferences.getString(UpdateManager.VERIFIED_APK_PATH, "")
            : "";
        Uri apkUri = !verifiedPath.isEmpty()
            ? FileProvider.getUriForFile(
                this,
                getPackageName() + ".updateprovider",
                new File(verifiedPath)
            )
            : manager.getUriForDownloadedFile(downloadId);
        if (!APK_MIME_TYPE.equalsIgnoreCase(downloadedMimeType) || apkUri == null) {
            clearPendingApkDownload(downloadId);
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
        // Capture the expected fields atomically before the worker starts; the
        // validator never touches shared preferences afterwards.
        UpdateManager.PendingUpdateSnapshot snapshot =
            UpdateManager.pendingSnapshot(this, downloadId);
        if (snapshot == null) {
            // Pending state moved on already; just re-evaluate whatever is current.
            installPendingApk(requestPermission);
            return;
        }
        updateVerificationInProgress = true;
        Toast.makeText(this, R.string.update_verifying, Toast.LENGTH_SHORT).show();
        updateVerifyExecutor.execute(() -> {
            ValidationResult result = validatePendingUpdate(snapshot);
            mainHandler.post(() -> {
                updateVerificationInProgress = false;
                if (!isActivityUsable()) {
                    deleteStagedUpdate(result.stagedPath);
                    return;
                }
                // A newer download may have replaced this one while
                // verification ran; never commit or clear state that belongs
                // to another pending id. The newer pending may have been
                // blocked behind the in-flight flag, so start its normal
                // flow now (installPendingApk is id-safe and only launches
                // the installer while resumed).
                if (!UpdateManager.isCurrentPending(this, snapshot.downloadId)) {
                    deleteStagedUpdate(result.stagedPath);
                    installPendingApk(requestPermission);
                    return;
                }
                if (result.error != 0) {
                    UpdateManager.clearPending(this, snapshot.downloadId);
                    if (updateProgressDialog != null) {
                        updateProgressDialog.dismiss();
                        updateProgressDialog = null;
                    }
                    Toast.makeText(this, result.error, Toast.LENGTH_LONG).show();
                    return;
                }
                if (!UpdateManager.commitVerifiedApk(this, snapshot.downloadId, result.stagedPath)) {
                    deleteStagedUpdate(result.stagedPath);
                    return;
                }
                // Commit may run while paused; installPendingApk only launches
                // the installer when the activity is resumed and re-runs on
                // the next onResume.
                installPendingApk(requestPermission);
            });
        });
    }

    private void deleteStagedUpdate(String path) {
        if (path == null || path.isEmpty()) {
            return;
        }
        try {
            File updatesDir = new File(getFilesDir(), "updates").getCanonicalFile();
            File file = new File(path).getCanonicalFile();
            if (file.getName().startsWith("verified-webclx-update-")
                && updatesDir.equals(file.getParentFile())) {
                file.delete();
            }
        } catch (Exception ignored) {
            // Only ever delete app-owned staged update files.
        }
    }

    private ValidationResult validatePendingUpdate(UpdateManager.PendingUpdateSnapshot snapshot) {
        String expectedSha256 = snapshot.sha256;
        long expectedSize = snapshot.size;
        long expectedVersionCode = snapshot.versionCode;
        File updateDir = new File(getFilesDir(), "updates");
        File staged = null;
        boolean keepStaged = false;
        try {
            // DownloadManager queries can throw; keep everything inside try so
            // a stale/destroyed worker always returns a bounded result.
            Uri source = downloadManager().getUriForDownloadedFile(snapshot.downloadId);
            if (source == null || expectedSize <= 0 || expectedVersionCode <= 0) {
                return new ValidationResult(null, R.string.update_integrity_failed);
            }
            if (!updateDir.isDirectory() && !updateDir.mkdirs()) {
                return new ValidationResult(null, R.string.update_integrity_failed);
            }
            // Unique per-attempt staging: an overlapping task for the same
            // download id can never collide with or delete this file.
            staged = File.createTempFile(
                "verified-webclx-update-" + snapshot.downloadId + "-",
                ".apk",
                updateDir
            );
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            long copied = 0;
            try (InputStream input = getContentResolver().openInputStream(source);
                 FileOutputStream output = new FileOutputStream(staged, false)) {
                if (input == null) {
                    return new ValidationResult(staged.getAbsolutePath(), R.string.update_integrity_failed);
                }
                byte[] buffer = new byte[32 * 1024];
                int read;
                while ((read = input.read(buffer)) >= 0) {
                    if (Thread.currentThread().isInterrupted() || destroyed) {
                        return new ValidationResult(staged.getAbsolutePath(), R.string.update_integrity_failed);
                    }
                    copied += read;
                    if (copied > expectedSize) {
                        return new ValidationResult(staged.getAbsolutePath(), R.string.update_integrity_failed);
                    }
                    digest.update(buffer, 0, read);
                    output.write(buffer, 0, read);
                }
                output.getFD().sync();
            }
            if (copied != expectedSize || !hex(digest.digest()).equals(expectedSha256)) {
                return new ValidationResult(staged.getAbsolutePath(), R.string.update_integrity_failed);
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
                return new ValidationResult(staged.getAbsolutePath(), R.string.update_identity_failed);
            }
            keepStaged = true;
            return new ValidationResult(staged.getAbsolutePath(), 0);
        } catch (Exception error) {
            return new ValidationResult(
                staged == null ? null : staged.getAbsolutePath(),
                R.string.update_integrity_failed
            );
        } finally {
            if (!keepStaged && staged != null && staged.exists()) {
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
                clearPendingApkDownload(downloadId);
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
        // An explicit retry also resets the renderer crash counter so the user
        // can truly retry after repeated crashes.
        retryButton.setOnClickListener(view -> {
            renderProcessFailures = 0;
            beginSourceResolution(true);
        });
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
                if (view != webView || !isActivityUsable()) {
                    callback.onReceiveValue(null);
                    return true;
                }
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
            public void onPageStarted(WebView view, String url, android.graphics.Bitmap favicon) {
                // Remember the last validated selected-origin URL so a render
                // process crash or Activity recreation can reload exactly it.
                // Only the live view's selected-source or debug-harness page
                // counts; anything else stays untracked.
                if (view != webView || !isActivityUsable()) {
                    return;
                }
                Uri uri = Uri.parse(url);
                if (isSelectedSourceUrl(url) || isHarnessOrigin(uri)) {
                    lastLoadedUrl = url;
                }
            }

            @Override
            public void onPageFinished(WebView view, String url) {
                if (view != webView || !isActivityUsable()) {
                    return;
                }
                // A harness page is a fully connected state too: after a
                // renderer recovery onto it the panel must still clear.
                Uri uri = Uri.parse(url);
                if (!sourceResolutionInProgress
                    && (isSelectedSourceUrl(url) || isHarnessOrigin(uri))) {
                    renderProcessFailures = 0;
                    applyWebPreferences();
                    connectionPanel.setVisibility(View.GONE);
                    webView.setVisibility(View.VISIBLE);
                    maybeCheckForUpdate();
                }
            }

            @Override
            public boolean onRenderProcessGone(WebView view, RenderProcessGoneDetail detail) {
                return handleRenderProcessGone(view);
            }

            @Override
            public void onReceivedError(
                WebView view,
                WebResourceRequest request,
                WebResourceError error
            ) {
                if (view == webView && isActivityUsable()
                    && request.isForMainFrame()
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
                if (view == webView && isActivityUsable()
                    && request.isForMainFrame()
                    && response.getStatusCode() >= 500
                    && isSelectedSourceUrl(request.getUrl().toString())) {
                    retryAlternateSource();
                }
            }
        });
        webView.setDownloadListener(this::requestDownload);
        settings.setTextZoom(AppPreferences.textZoom(this));
    }

    /**
     * The renderer process crashed (OOM/kill). Returning true acknowledges the
     * crash — the failed WebView must never be used again, so we dispose it and
     * build a fresh view, then reload the last validated selected-origin URL
     * or fall back to normal source resolution.
     */
    private boolean handleRenderProcessGone(WebView failedView) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return false;
        }
        if (failedView != webView) {
            // A stale view we already replaced: only detach/destroy it. It must
            // not complete the current view's file chooser or mutate live state.
            if (contentRoot != null) {
                contentRoot.removeView(failedView);
            }
            failedView.destroy();
            return true;
        }
        if (fileChooserCallback != null) {
            fileChooserCallback.onReceiveValue(null);
            fileChooserCallback = null;
        }
        if (contentRoot != null) {
            contentRoot.removeView(failedView);
        }
        failedView.destroy();
        webView = null;
        if (!isActivityUsable()) {
            return true;
        }
        // Any in-flight source probe/re-evaluation result refers to the dead
        // view and must not drive the replacement.
        sourceSelectionGeneration++;
        sourceResolutionInProgress = false;
        sourceReevaluationInProgress = false;
        sourceReevaluationPending = false;
        renderProcessFailures++;

        if (renderProcessFailures > MAX_RENDER_PROCESS_RETRIES) {
            webView = createWebView(resolveWindowBackgroundColor());
            if (contentRoot != null) {
                contentRoot.addView(webView, 0, new FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.MATCH_PARENT
                ));
            }
            configureWebView();
            connectionPanel.setVisibility(View.VISIBLE);
            webView.setVisibility(View.GONE);
            showConnectionFailure();
            connectionMessage.setText(R.string.connection_failed);
            return true;
        }

        webView = createWebView(resolveWindowBackgroundColor());
        if (contentRoot != null) {
            // Keep the connection panel above the terminal view.
            contentRoot.addView(webView, 0, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            ));
        }
        configureWebView();

        String recoverUrl = lastLoadedUrl;
        // lastLoadedUrl is only ever a selected-source (including custom
        // openInWebView origins) or debug-harness page, so reuse the same
        // tracking predicate — plain isTrustedOrigin would reject custom picks.
        if (recoverUrl != null
            && (isSelectedSourceUrl(recoverUrl) || isHarnessOrigin(Uri.parse(recoverUrl)))) {
            connectionMessage.setText(R.string.connecting);
            connectionPanel.setVisibility(View.VISIBLE);
            webView.loadUrl(recoverUrl);
        } else {
            beginSourceResolution(true);
        }
        return true;
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

    private void applyScreenOrientation(String mode) {
        int orientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT;
        if (AppPreferences.ORIENTATION_LANDSCAPE.equals(mode)) {
            orientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE;
        } else if (AppPreferences.ORIENTATION_SYSTEM.equals(mode)) {
            orientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED;
        }
        setRequestedOrientation(orientation);
    }

    private void maybeCheckForUpdate() {
        if (automaticUpdateCheckStarted || !AppPreferences.autoUpdate(this)) {
            return;
        }
        automaticUpdateCheckStarted = true;
        updateCheckFuture = UpdateManager.check(this, selectedSource, new UpdateManager.Callback() {
            @Override
            public void onSuccess(UpdateManager.UpdateManifest manifest) {
                if (!UpdateManager.isNewer(MainActivity.this, manifest)) {
                    return;
                }
                runOnUiThread(() -> {
                    if (destroyed || isFinishing()) {
                        return;
                    }
                    // Never pop a dialog over a paused/backgrounded activity;
                    // keep the manifest and prompt once on the next resume.
                    if (!activityResumed) {
                        pendingUpdateManifest = manifest;
                        return;
                    }
                    showUpdatePrompt(manifest);
                });
            }

            @Override
            public void onError(String message) {
                // Automatic checks stay quiet; manual checks expose their error in Settings.
            }
        });
    }

    private void showUpdatePrompt(UpdateManager.UpdateManifest manifest) {
        if (destroyed || isFinishing()) {
            return;
        }
        if (!activityResumed) {
            pendingUpdateManifest = manifest;
            return;
        }
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
                try {
                    long downloadId = UpdateManager.enqueue(this, manifest);
                    showUpdateProgress(downloadId);
                } catch (RuntimeException error) {
                    Toast.makeText(this, R.string.apk_download_failed, Toast.LENGTH_LONG).show();
                }
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
        updateProgressDialog.setOnDismissListener(dialog -> {
            // Only the dialog that owns the field may invalidate the poll
            // generation; a stale dialog closing late must not kill a newer
            // download's polling.
            if (dialog == updateProgressDialog) {
                updateProgressDialog = null;
                stopUpdateProgressPolling();
            }
        });
        updateProgressDialog.show();
        updateProgressBar = progress;
        updateProgressDownloadId = downloadId;
        startUpdateProgressPolling();
    }

    // DownloadManager.query hits a provider; each poll runs one background
    // query and posts its result. Only the main-thread result handler re-arms
    // the next query, so a stopped/paused/new-generation dialog can never be
    // touched by a queued result or accumulate unbounded posts.
    private void startUpdateProgressPolling() {
        int generation = ++updateProgressGeneration;
        final long downloadId = updateProgressDownloadId;
        if (!isActivityUsable() || !activityResumed || updateProgressDialog == null
            || !updateProgressDialog.isShowing()
            || downloadId == UpdateManager.NO_DOWNLOAD) {
            return;
        }
        updatePollExecutor.execute(() -> runUpdateProgressQuery(generation, downloadId));
    }

    private void stopUpdateProgressPolling() {
        updateProgressGeneration++;
    }

    private void runUpdateProgressQuery(int generation, long downloadId) {
        boolean missing = false;
        boolean queryFailed = false;
        int status = 0;
        long downloaded = 0;
        long total = 0;
        try {
            DownloadManager.Query query = new DownloadManager.Query().setFilterById(downloadId);
            try (Cursor cursor = downloadManager().query(query)) {
                if (cursor == null || !cursor.moveToFirst()) {
                    missing = true;
                } else {
                    status = cursor.getInt(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS));
                    downloaded = cursor.getLong(
                        cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR)
                    );
                    total = cursor.getLong(
                        cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_TOTAL_SIZE_BYTES)
                    );
                }
            }
        } catch (RuntimeException error) {
            queryFailed = true;
        }
        final int resultStatus = status;
        final long resultDownloaded = downloaded;
        final long resultTotal = total;
        final boolean resultMissing = missing;
        final boolean resultFailed = queryFailed;
        mainHandler.post(() ->
            handleUpdateProgressResult(
                generation, downloadId, resultStatus, resultDownloaded, resultTotal,
                resultMissing, resultFailed
            )
        );
    }

    private void handleUpdateProgressResult(
        int generation,
        long downloadId,
        int status,
        long downloaded,
        long total,
        boolean missing,
        boolean queryFailed
    ) {
        if (generation != updateProgressGeneration || !isActivityUsable()) {
            return;
        }
        if (updateProgressDialog == null || !updateProgressDialog.isShowing()
            || updateProgressBar == null
            || downloadId != updateProgressDownloadId
            || downloadId != pendingApkDownloadId()
            || !activityResumed) {
            return;
        }
        if (queryFailed) {
            stopUpdateProgressPolling();
            updateProgressDialog.dismiss();
            Toast.makeText(this, R.string.apk_download_failed, Toast.LENGTH_LONG).show();
            return;
        }
        if (missing) {
            stopUpdateProgressPolling();
            updateProgressDialog.dismiss();
            updateProgressDialog = null;
            return;
        }
        if (total > 0) {
            updateProgressBar.setIndeterminate(false);
            updateProgressBar.setProgress((int) Math.min(100, downloaded * 100 / total));
        }
        if (status == DownloadManager.STATUS_SUCCESSFUL) {
            stopUpdateProgressPolling();
            updateProgressDialog.setTitle(R.string.update_verifying);
            updateProgressBar.setIndeterminate(true);
            // installPendingApk deduplicates via INSTALLER_LAUNCHED_DOWNLOAD_ID.
            installPendingApk(true);
            return;
        }
        if (status == DownloadManager.STATUS_FAILED) {
            stopUpdateProgressPolling();
            updateProgressDialog.dismiss();
            clearPendingApkDownload(downloadId);
            Toast.makeText(this, R.string.apk_download_failed, Toast.LENGTH_LONG).show();
            return;
        }
        // Arm the next query only while this generation still owns the dialog.
        mainHandler.postDelayed(() -> {
            if (generation == updateProgressGeneration && isActivityUsable()
                && activityResumed
                && updateProgressDialog != null && updateProgressDialog.isShowing()
                && downloadId == updateProgressDownloadId) {
                updatePollExecutor.execute(() -> runUpdateProgressQuery(generation, downloadId));
            }
        }, 500);
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
        mainHandler.post(() -> {
            if (sourceResolutionInProgress || sourceReevaluationInProgress) {
                sourceReevaluationPending = true;
                return;
            }
            reevaluateCurrentSource();
        });
    }

    private void replayPendingSourceReevaluation() {
        if (!sourceReevaluationPending || destroyed) {
            return;
        }
        sourceReevaluationPending = false;
        mainHandler.post(this::requestSourceReevaluation);
    }

    private void finishSourceResolution() {
        sourceResolutionInProgress = false;
        replayPendingSourceReevaluation();
    }

    private void finishSourceReevaluation() {
        sourceReevaluationInProgress = false;
        replayPendingSourceReevaluation();
    }

    private void reevaluateCurrentSource() {
        if (!isActivityUsable() || webView == null) {
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
        final int generation = sourceSelectionGeneration;
        if (currentOrigin != null && !currentOrigin.isEmpty()) {
            sourceReevaluationInProgress = true;
            resolverExecutor.execute(() -> {
                SourceRegistry.ProbeResult result = SourceRegistry.isValidIndex(currentSource)
                    ? SourceRegistry.probe(currentSource)
                    : SourceRegistry.probeUrl(currentOrigin);
                runOnUiThread(() -> {
                    // A newer selection (loadSource/failure) made while the
                    // probe ran owns the in-progress flags now; a stale result
                    // must not clear them.
                    if (!isActivityUsable() || generation != sourceSelectionGeneration) {
                        return;
                    }
                    finishSourceReevaluation();
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
        if (!isActivityUsable() || webView == null) {
            return;
        }
        Arrays.fill(rejectedSources, false);
        int preferred = AppPreferences.preferredSource(this);
        if (preferred >= 0) {
            sourceResolutionInProgress = true;
            if (foreground) {
                showConnectingState();
            }
            final int generation = sourceSelectionGeneration;
            resolverExecutor.execute(() -> {
                SourceRegistry.ProbeResult result = SourceRegistry.probe(preferred);
                runOnUiThread(() -> {
                    // Guard before clearing the flag: a stale result must not
                    // unlock a newer resolution that already started.
                    if (!isActivityUsable() || generation != sourceSelectionGeneration) {
                        return;
                    }
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
        if (!isActivityUsable() || webView == null) {
            return;
        }
        if (excludedSource >= 0) {
            rejectedSources[excludedSource] = true;
        }
        sourceResolutionInProgress = true;
        if (foreground) {
            showConnectingState();
        }
        final int generation = sourceSelectionGeneration;
        final boolean[] rejectedSnapshot =
            Arrays.copyOf(rejectedSources, SourceRegistry.URLS.length);

        resolverExecutor.execute(() -> {
            // Probe against an immutable snapshot of rejected sources; failure
            // marks are merged on the main thread only while this operation is
            // still the current selection attempt.
            final boolean[] failed = new boolean[SourceRegistry.URLS.length];
            CompletionService<Integer> completion =
                new ExecutorCompletionService<>(probeExecutor);
            List<Future<Integer>> probes = new ArrayList<>();
            for (int index = 0; index < SourceRegistry.URLS.length; index++) {
                if (rejectedSnapshot[index]) {
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
                        runOnUiThread(() -> {
                            if (!isActivityUsable() || generation != sourceSelectionGeneration) {
                                return;
                            }
                            mergeRejectedSources(failed);
                            loadSource(result);
                        });
                        return;
                    }
                    failed[-result - 1] = true;
                } catch (InterruptedException error) {
                    cancelProbes(probes);
                    Thread.currentThread().interrupt();
                    return;
                } catch (Exception ignored) {
                    // A failed probe cannot win; keep waiting for the remaining candidates.
                }
            }
            runOnUiThread(() -> {
                if (!isActivityUsable() || generation != sourceSelectionGeneration) {
                    return;
                }
                mergeRejectedSources(failed);
                if (foreground) {
                    showConnectionFailure();
                } else {
                    finishSourceResolution();
                }
            });
        });
    }

    private void mergeRejectedSources(boolean[] failed) {
        for (int index = 0; index < failed.length && index < rejectedSources.length; index++) {
            rejectedSources[index] |= failed[index];
        }
    }

    private void showConnectingState() {
        sourceSelectionGeneration++;
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
        sourceSelectionGeneration++;
        selectedSource = source;
        selectedSourceOrigin = source >= 0 && source < SourceRegistry.URLS.length
            ? SourceRegistry.URLS[source]
            : null;
        connectionMessage.setText(R.string.connecting);
        AppPreferences.get(this).edit()
            .putInt(AppPreferences.KEY_ACTIVE_SOURCE, source)
            .putLong(AppPreferences.KEY_ACTIVE_SOURCE_AT, System.currentTimeMillis())
            .apply();
        String targetUrl = SourceRegistry.URLS[source] + AppPreferences.startPath(this);
        lastLoadedUrl = targetUrl;
        webView.loadUrl(targetUrl);
        finishSourceResolution();
    }

    private void showConnectionFailure() {
        sourceSelectionGeneration++;
        selectedSource = -1;
        selectedSourceOrigin = null;
        connectionProgress.setVisibility(View.GONE);
        connectionMessage.setText(R.string.connection_failed);
        retryButton.setVisibility(View.VISIBLE);
        finishSourceResolution();
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
        @JavascriptInterface
        public boolean isSystemKeyboardVisible() {
            return systemKeyboardVisible;
        }

        @JavascriptInterface
        public String getScreenOrientation() {
            return AppPreferences.screenOrientation(MainActivity.this);
        }

        @JavascriptInterface
        public boolean setScreenOrientation(String mode) {
            if (!AppPreferences.ORIENTATION_PORTRAIT.equals(mode)
                && !AppPreferences.ORIENTATION_LANDSCAPE.equals(mode)
                && !AppPreferences.ORIENTATION_SYSTEM.equals(mode)) {
                return false;
            }
            AppPreferences.get(MainActivity.this)
                .edit()
                .putString(AppPreferences.KEY_SCREEN_ORIENTATION, mode)
                .apply();
            mainHandler.post(() -> applyScreenOrientation(mode));
            return true;
        }

        @JavascriptInterface
        public boolean copyText(String text) {
            if (text == null || text.length() > 1_048_576) {
                return false;
            }
            try {
                ClipboardManager clipboard = getSystemService(ClipboardManager.class);
                if (clipboard == null) {
                    return false;
                }
                clipboard.setPrimaryClip(ClipData.newPlainText("webClx", text));
                return true;
            } catch (RuntimeException error) {
                return false;
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
                if (!isActivityUsable() || webView == null) {
                    return;
                }
                // A manual server pick invalidates any in-flight probe results.
                sourceSelectionGeneration++;
                sourceResolutionInProgress = false;
                sourceReevaluationInProgress = false;
                sourceReevaluationPending = false;
                selectedSource = sourceIndexForOrigin(target);
                selectedSourceOrigin = sourceOrigin(target);
                lastLoadedUrl = target.toString();
                webView.loadUrl(lastLoadedUrl);
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
        if (isHarnessOrigin(uri)) {
            return true;
        }
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

    private boolean isHarnessOrigin(Uri uri) {
        if (selectedSourceOrigin == null || selectedSourceOrigin.isEmpty()) {
            return false;
        }
        Uri selected = Uri.parse(selectedSourceOrigin);
        return ("http".equalsIgnoreCase(uri.getScheme())
                || "https".equalsIgnoreCase(uri.getScheme()))
            && equalsIgnoreCase(uri.getHost(), selected.getHost())
            && effectivePort(uri) == HARNESS_PORT;
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
        if (webView != null && webView.canGoBack()) {
            webView.goBack();
        } else {
            super.onBackPressed();
        }
    }

    @Override
    protected void onPause() {
        activityResumed = false;
        webViewPausedSinceResume = true;
        stopUpdateProgressPolling();
        if (webView != null) {
            webView.onPause();
        }
        super.onPause();
    }

    @Override
    protected void onResume() {
        super.onResume();
        activityResumed = true;
        if (webView != null) {
            webView.onResume();
        }
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
        if (webView != null) {
            webView.getSettings().setTextZoom(AppPreferences.textZoom(this));
        }
        if (webViewPausedSinceResume && !returningFromInstallPermission) {
            webViewPausedSinceResume = false;
            // No reload on an ordinary resume: JS terminal state, drafts, and
            // scroll survive. A synthetic pageshow reaches the existing
            // refreshSessionsAfterPageResume handlers, and a native re-probe
            // revalidates the current origin in the background.
            if (webView != null) {
                webView.evaluateJavascript(
                    "window.dispatchEvent(new Event('pageshow'))",
                    null
                );
            }
            requestSourceReevaluation();
        }
        if (pendingUpdateManifest != null) {
            UpdateManager.UpdateManifest manifest = pendingUpdateManifest;
            pendingUpdateManifest = null;
            showUpdatePrompt(manifest);
        }
        if (updateProgressDialog != null && updateProgressDialog.isShowing()) {
            // Restart the generation-guarded query loop paused by onPause.
            startUpdateProgressPolling();
        }
    }

    @Override
    protected void onDestroy() {
        unregisterNetworkCallback();
        destroyed = true;
        if (updateCheckFuture != null) {
            updateCheckFuture.cancel(true);
            updateCheckFuture = null;
        }
        stopUpdateProgressPolling();
        updateProgressDownloadId = UpdateManager.NO_DOWNLOAD;
        resolverExecutor.shutdownNow();
        probeExecutor.shutdownNow();
        updateVerifyExecutor.shutdownNow();
        updatePollExecutor.shutdownNow();
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
        if (webView != null) {
            webView.stopLoading();
            webView.destroy();
            webView = null;
        }
        super.onDestroy();
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    private static final class ValidationResult {
        final String stagedPath;
        final int error;

        ValidationResult(String stagedPath, int error) {
            this.stagedPath = stagedPath;
            this.error = error;
        }
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
