import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const activity = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/MainActivity.java", import.meta.url),
  "utf8",
);
const sourceRegistry = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/SourceRegistry.java", import.meta.url),
  "utf8",
);
const appPreferences = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/AppPreferences.java", import.meta.url),
  "utf8",
);
const settingsActivity = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/SettingsActivity.java", import.meta.url),
  "utf8",
);
const updateManager = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/UpdateManager.java", import.meta.url),
  "utf8",
);
const artifactRoutes = readFileSync(
  new URL("../src/routes/artifacts.rs", import.meta.url),
  "utf8",
);
const artifacts = readFileSync(
  new URL("../src/artifacts.rs", import.meta.url),
  "utf8",
);
const manifest = readFileSync(
  new URL("../android/app/src/main/AndroidManifest.xml", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(
  new URL("../static/terminal.html", import.meta.url),
  "utf8",
);
const terminalJs = readFileSync(
  new URL("../static/terminal.js", import.meta.url),
  "utf8",
);
const networkPolicy = readFileSync(
  new URL("../android/app/src/main/res/xml/network_security_config.xml", import.meta.url),
  "utf8",
);
const lightTheme = readFileSync(
  new URL("../android/app/src/main/res/values/themes.xml", import.meta.url),
  "utf8",
);
const nightTheme = readFileSync(
  new URL("../android/app/src/main/res/values-night/themes.xml", import.meta.url),
  "utf8",
);
const strings = readFileSync(
  new URL("../android/app/src/main/res/values/strings.xml", import.meta.url),
  "utf8",
);
const gradle = readFileSync(
  new URL("../android/app/build.gradle.kts", import.meta.url),
  "utf8",
);
const gradleProperties = readFileSync(
  new URL("../android/gradle.properties", import.meta.url),
  "utf8",
);
const buildScript = readFileSync(
  new URL("../scripts/build-android-apk.sh", import.meta.url),
  "utf8",
);
const serverProbeSource = readFileSync(
  new URL("../static/terminal-server-probe.js", import.meta.url),
  "utf8",
);

test("Android client races every configured source and selects the fastest healthy result", () => {
  for (const source of [
    "http://HOST:PORT/",
    "http://PUBLIC_DOMAIN:PORT/",
  ]) {
    assert.match(sourceRegistry, new RegExp(`"${source.replaceAll(".", "\\.")}"`));
  }
  assert.doesNotMatch(sourceRegistry, /"http:\/\/fpsq\.xyz:11111\/"/);
  assert.doesNotMatch(sourceRegistry, /webclx\.example\.invalid/);
  assert.match(sourceRegistry, /PROBE_PATH = "api\/auth\/session"/);
  assert.match(activity, /Executors\.newFixedThreadPool\(SourceRegistry\.URLS\.length\)/);
  assert.match(activity, /CompletionService<Integer>/);
  assert.match(activity, /new ExecutorCompletionService<>\(probeExecutor\)/);
  assert.match(activity, /completion\.take\(\)\.get\(\)/);
  assert.match(activity, /probe\.cancel\(true\)/);
  assert.match(activity, /AppPreferences\.preferredSource\(this\)/);
  assert.match(activity, /SourceRegistry\.probe\(preferred\)/);
  assert.match(activity, /KEY_ACTIVE_SOURCE/);
  assert.match(activity, /retryAlternateSource\(\)/);
  assert.match(activity, /rejectedSources\[excludedSource\] = true/);
  assert.match(activity, /onPageFinished[\s\S]*isSelectedSourceUrl\(url\)/);
});

test("Android network changes keep the exact current server while it remains healthy", () => {
  assert.match(activity, /private String selectedSourceOrigin;/);
  assert.match(
    activity,
    /String currentOrigin = selectedSourceOrigin;[\s\S]*SourceRegistry\.probeUrl\(currentOrigin\)/,
  );
  assert.match(
    activity,
    /if \(result\.healthy\) \{[\s\S]*return;[\s\S]*resolveAndLoad\(currentSource, false\);/,
  );
  assert.match(activity, /if \(!currentOrigin\.equals\(selectedSourceOrigin\)\) \{/);
  assert.match(
    activity,
    /selectedSource = sourceIndexForOrigin\(target\);[\s\S]*selectedSourceOrigin = sourceOrigin\(target\);/,
  );
  assert.match(sourceRegistry, /static ProbeResult probeUrl\(String baseUrl\)/);
});

test("Android runtime source selection stays in the background and ignores stale failures", () => {
  assert.match(activity, /private boolean sourceReevaluationInProgress;/);
  assert.match(
    activity,
    /if \(sourceResolutionInProgress \|\| sourceReevaluationInProgress\) \{[\s\S]*sourceReevaluationInProgress = true;/,
  );
  assert.match(
    activity,
    /onReceivedError[\s\S]*isSelectedSourceUrl\(request\.getUrl\(\)\.toString\(\)\)[\s\S]*retryAlternateSource\(\)/,
  );
  assert.match(
    activity,
    /onReceivedHttpError[\s\S]*isSelectedSourceUrl\(request\.getUrl\(\)\.toString\(\)\)[\s\S]*retryAlternateSource\(\)/,
  );
  assert.match(activity, /resolveAndLoad\(failedSource, false\)/);
  assert.match(
    activity,
    /private boolean isSelectedSourceUrl\(String url\)[\s\S]*Uri\.parse\(selectedSourceOrigin\)/,
  );
  assert.match(
    activity,
    /private void resolveAndLoad\(int excludedSource, boolean foreground\)[\s\S]*if \(foreground\) \{\s*showConnectingState\(\);\s*\}/,
  );
  assert.match(
    activity,
    /if \(foreground\) \{\s*showConnectionFailure\(\);\s*\} else \{\s*finishSourceResolution\(\);/,
  );
});

test("Android replays network changes that arrive during source probing", () => {
  assert.match(activity, /private boolean sourceReevaluationPending;/);
  assert.match(
    activity,
    /requestSourceReevaluation\(\)[\s\S]*sourceResolutionInProgress \|\| sourceReevaluationInProgress[\s\S]*sourceReevaluationPending = true;/,
  );
  assert.match(
    activity,
    /private void replayPendingSourceReevaluation\(\)[\s\S]*sourceReevaluationPending = false;[\s\S]*mainHandler\.post\(this::requestSourceReevaluation\)/,
  );
  assert.match(activity, /private void finishSourceResolution\(\)[\s\S]*replayPendingSourceReevaluation\(\)/);
  assert.match(activity, /loadSource\(int source\)[\s\S]*finishSourceResolution\(\)/);
  assert.match(activity, /showConnectionFailure\(\)[\s\S]*finishSourceResolution\(\)/);
});

test("Android WebView keeps navigation and cleartext access on known data sources", () => {
  assert.match(activity, /isTrustedOrigin\(uri\)/);
  assert.match(activity, /isHarnessOrigin\(uri\)/);
  assert.match(activity, /HARNESS_PORT = 3080/);
  assert.match(
    activity,
    /isHarnessOrigin\(Uri uri\)[\s\S]*selectedSourceOrigin[\s\S]*effectivePort\(uri\) == HARNESS_PORT/,
  );
  assert.doesNotMatch(activity, /onReceivedHttpAuthRequest|HARNESS_CREDENTIALS/);
  assert.match(activity, /CookieManager\.getInstance\(\)[\s\S]*setAcceptCookie\(true\)/);
  assert.match(activity, /Intent\.ACTION_VIEW/);
  assert.match(networkPolicy, /<base-config cleartextTrafficPermitted="false"/);
  assert.match(networkPolicy, />HOST</);
  assert.match(networkPolicy, />PUBLIC_DOMAIN</);
  assert.doesNotMatch(manifest, /android:usesCleartextTraffic="true"/);
  assert.match(manifest, /android\.permission\.INTERNET/);
  assert.match(manifest, /android:networkSecurityConfig="@xml\/network_security_config"/);
});

test("Android WebView resumes without reloading and refreshes via pageshow", () => {
  assert.match(activity, /private boolean webViewPausedSinceResume;/);
  assert.match(
    activity,
    /protected void onPause\(\)[\s\S]*webViewPausedSinceResume = true;[\s\S]*webView\.onPause\(\)/,
  );
  const onResume = activity.match(
    /protected void onResume\(\) \{([\s\S]*?)\n    \}\n\n    @Override\n    protected void onDestroy/,
  )?.[1] ?? "";
  assert.ok(onResume, "onResume body must be extractable");
  assert.doesNotMatch(onResume, /webView\.reload\(\)/, "an ordinary resume must not reload the page");
  assert.match(
    onResume,
    /webViewPausedSinceResume = false;[\s\S]*dispatchEvent\(new Event\('pageshow'\)\)[\s\S]*requestSourceReevaluation\(\)/,
    "returning from pause should dispatch pageshow and re-probe the current origin",
  );
  assert.match(
    terminalJs,
    /function refreshSessionsAfterPageResume\(\)[\s\S]*window\.addEventListener\("pageshow", refreshSessionsAfterPageResume\)/,
    "terminal.js keeps the pageshow resume handler",
  );
});

test("Android exposes the real system-keyboard visibility to terminal focus policy", () => {
  assert.match(activity, /private volatile boolean systemKeyboardVisible;/);
  assert.match(
    activity,
    /VERSION_CODES\.R[\s\S]*setOnApplyWindowInsetsListener[\s\S]*WindowInsets\.Type\.ime\(\)/,
    "API 30+ should read IME visibility from WindowInsets",
  );
  assert.match(
    activity,
    /getViewTreeObserver\(\)\.addOnGlobalLayoutListener[\s\S]*getWindowVisibleDisplayFrame\(visibleFrame\)[\s\S]*rootHeight - visibleFrame\.bottom[\s\S]*systemKeyboardVisible = rootHeight > 0 && obscuredHeight > rootHeight \* 0\.15/,
    "older APIs keep the geometry-based keyboard fallback",
  );
  assert.match(
    activity,
    /@JavascriptInterface\s+public boolean isSystemKeyboardVisible\(\) \{\s*return systemKeyboardVisible;\s*\}/,
  );
});

test("Android recreates the WebView after a render process crash", () => {
  assert.match(activity, /import android\.webkit\.RenderProcessGoneDetail;/);
  assert.match(
    activity,
    /onRenderProcessGone\(WebView view, RenderProcessGoneDetail detail\)[\s\S]*return handleRenderProcessGone\(view\)/,
  );
  assert.match(
    activity,
    /handleRenderProcessGone[\s\S]*fileChooserCallback\.onReceiveValue\(null\)[\s\S]*removeView\(failedView\)[\s\S]*failedView\.destroy\(\)/,
    "the crashed view must be detached and destroyed, never reused",
  );
  assert.match(
    activity,
    /MAX_RENDER_PROCESS_RETRIES[\s\S]*renderProcessFailures > MAX_RENDER_PROCESS_RETRIES[\s\S]*showConnectionFailure\(\)/,
    "repeated crashes before a successful load must stop auto-respawn",
  );
  assert.match(
    activity,
    /renderProcessFailures = 0;/,
    "a successful page load resets the crash counter",
  );
  assert.match(
    activity,
    /onPageStarted[\s\S]*isSelectedSourceUrl\(url\) \|\| isHarnessOrigin\(uri\)[\s\S]*lastLoadedUrl = url;/,
    "only selected-source or harness URLs may be restored after a crash",
  );
  assert.doesNotMatch(activity, /failedView\.reload\(\)/);
});

test("Android terminal exposes a one-way portrait recovery without accidental rotation", () => {
  assert.match(appPreferences, /KEY_SCREEN_ORIENTATION = "display\.screen_orientation"/);
  assert.match(appPreferences, /ORIENTATION_PORTRAIT = "portrait"/);
  assert.match(appPreferences, /ORIENTATION_LANDSCAPE = "landscape"/);
  assert.match(appPreferences, /ORIENTATION_SYSTEM = "system"/);
  assert.match(appPreferences, /static String screenOrientation\(Context context\)/);
  assert.match(activity, /applyScreenOrientation\(AppPreferences\.screenOrientation\(this\)\)/);
  assert.match(activity, /ActivityInfo\.SCREEN_ORIENTATION_PORTRAIT/);
  assert.match(activity, /ActivityInfo\.SCREEN_ORIENTATION_LANDSCAPE/);
  assert.match(activity, /ActivityInfo\.SCREEN_ORIENTATION_UNSPECIFIED/);
  assert.match(
    activity,
    /@JavascriptInterface\s+public boolean setScreenOrientation\(String mode\)/,
  );
  assert.match(
    terminalHtml,
    /id="terminal-orientation-toggle"[\s\S]*data-action="restore_portrait"[\s\S]*hidden/,
  );
  // The orientation recovery lives in terminal.js; assert the module is
  // actually loaded (any cache key) rather than pinning a stale date token.
  assert.match(terminalHtml, /terminal\.js\?v=[A-Za-z0-9_-]+/);
  assert.match(terminalJs, /function restoreTerminalPortraitOrientation\(\)/);
  assert.match(terminalJs, /bridge\.setScreenOrientation\("portrait"\)/);
  assert.match(
    terminalJs,
    /const needsRecovery = mode !== "portrait";[\s\S]*terminalOrientationToggleButton\.hidden = !bridge \|\| !needsRecovery/,
  );
  assert.doesNotMatch(terminalJs, /current === "portrait"[\s\S]*\? "landscape"/);
});

test("Android exposes a native clipboard writer for WebView copy actions", () => {
  assert.match(activity, /import android\.content\.ClipData;/);
  assert.match(activity, /import android\.content\.ClipboardManager;/);
  assert.match(
    activity,
    /@JavascriptInterface\s+public boolean copyText\(String text\)[\s\S]*text == null[\s\S]*text\.length\(\) > 1_048_576[\s\S]*ClipboardManager\.class[\s\S]*setPrimaryClip\(ClipData\.newPlainText\("webClx", text\)\)/,
  );
});

test("Android client follows system light and dark mode", () => {
  assert.match(lightTheme, /parent="android:style\/Theme\.Material\.Light\.NoActionBar"/);
  assert.match(nightTheme, /parent="android:style\/Theme\.Material\.NoActionBar"/);
  assert.match(nightTheme, /<item name="android:windowLightStatusBar">false<\/item>/);
  assert.doesNotMatch(manifest, /android:configChanges="[^"]*uiMode/);
  assert.doesNotMatch(activity, /setStatusBarColor|setNavigationBarColor/);
  assert.doesNotMatch(activity, /SYSTEM_UI_FLAG_LIGHT_STATUS_BAR/);
});

test("Android WebView resize surface uses the current theme background", () => {
  assert.match(
    activity,
    /int contentBackgroundColor = resolveWindowBackgroundColor\(\);/,
  );
  assert.match(
    activity,
    /contentRoot\.setBackgroundColor\(contentBackgroundColor\);[\s\S]*webView = createWebView\(contentBackgroundColor\);[\s\S]*view\.setBackgroundColor\(backgroundColor\);/,
  );
  assert.match(
    activity,
    /obtainStyledAttributes\(\s*new int\[\]\{android\.R\.attr\.windowBackground\}\s*\)/,
  );
  assert.match(activity, /attributes\.recycle\(\)/);
});

test("Android client gives the WebView the full content height", () => {
  assert.match(
    activity,
    /contentRoot\.addView\(webView, new FrameLayout\.LayoutParams\(\s*ViewGroup\.LayoutParams\.MATCH_PARENT,\s*ViewGroup\.LayoutParams\.MATCH_PARENT\s*\)\)/,
  );
  assert.doesNotMatch(activity, /createAppBar\(\)/);
  assert.doesNotMatch(activity, /R\.drawable\.ic_download_tasks/);
  assert.doesNotMatch(activity, /R\.drawable\.ic_settings/);
});

test("Android download center opens the system download manager", () => {
  assert.match(activity, /getUserAgentString\(\) \+ " webClxAndroid\/"/);
  assert.match(activity, /"webclx"\.equalsIgnoreCase\(scheme\)/);
  assert.match(activity, /"downloads"\.equalsIgnoreCase\(uri\.getHost\(\)\)/);
  assert.match(activity, /new Intent\(DownloadManager\.ACTION_VIEW_DOWNLOADS\)/);
  assert.match(artifacts, /id="manage-downloads"/);
  assert.ok(artifacts.includes("webClxAndroid\\\\/"));
  assert.match(artifacts, /window\.location\.href = "webclx:\/\/downloads"/);
});

test("Android client installs only its own completed APK downloads", () => {
  assert.match(manifest, /android\.permission\.REQUEST_INSTALL_PACKAGES/);
  assert.match(activity, /APK_MIME_TYPE = "application\/vnd\.android\.package-archive"/);
  assert.match(activity, /long downloadId = manager\.enqueue\(request\)/);
  assert.match(activity, /isApkDownload\(filename, download\.mimeType\)/);
  assert.match(activity, /DownloadManager\.ACTION_DOWNLOAD_COMPLETE/);
  assert.match(
    activity,
    /ContextCompat\.registerReceiver\(\s*this,\s*downloadCompleteReceiver,\s*filter,\s*ContextCompat\.RECEIVER_EXPORTED\s*\)/,
    "the download receiver must register through ContextCompat as EXPORTED",
  );
  assert.doesNotMatch(
    activity,
    /registerReceiver\(downloadCompleteReceiver, filter/,
    "the legacy flag-less registerReceiver overload must be gone",
  );
  assert.match(activity, /DownloadManager\.EXTRA_DOWNLOAD_ID/);
  assert.match(activity, /downloadId != pendingApkDownloadId\(\)/);
  assert.match(activity, /new DownloadManager\.Query\(\)\.setFilterById\(downloadId\)/);
  assert.match(activity, /DownloadManager\.STATUS_SUCCESSFUL/);
  assert.match(activity, /manager\.getMimeTypeForDownloadedFile\(downloadId\)/);
  assert.match(activity, /manager\.getUriForDownloadedFile\(downloadId\)/);
  assert.match(activity, /canRequestPackageInstalls\(\)/);
  assert.match(activity, /Settings\.ACTION_MANAGE_UNKNOWN_APP_SOURCES/);
  assert.match(activity, /Uri\.parse\("package:" \+ getPackageName\(\)\)/);
  assert.match(activity, /setDataAndType\(apkUri, APK_MIME_TYPE\)/);
  assert.match(activity, /Intent\.FLAG_GRANT_READ_URI_PERMISSION/);
  assert.match(activity, /onResume\(\)[\s\S]*resumePendingInstallPermission\(\)/);
});

test("Android release identity comes from the authoritative Cargo package version", () => {
  assert.match(gradle, /applicationId = "com\.webclx\.app"/);
  assert.match(gradle, /providers\.gradleProperty\("webclxVersion"\)/);
  assert.match(buildScript, /cargo metadata[\s\S]*select\(\.name == "webclx"\)/);
  assert.match(buildScript, /apksigner" verify --verbose --print-certs/);
  assert.match(buildScript, /webClx-\$\{client_version\}\.apk/);
});

test("Android settings expose stable common, theme, data-source, and update tabs", () => {
  assert.match(settingsActivity, /TAB_KEYS = \{"general", "theme", "data-source", "update"\}/);
  assert.match(appPreferences, /KEY_THEME_MODE = "theme\.mode"/);
  assert.match(appPreferences, /THEME_SYSTEM = "system"/);
  assert.match(appPreferences, /THEME_LIGHT = "light"/);
  assert.match(appPreferences, /THEME_DARK = "dark"/);
  assert.match(appPreferences, /KEY_TEXT_ZOOM = "display\.text_zoom"/);
  assert.match(appPreferences, /KEY_START_PATH = "general\.start_path"/);
  assert.match(settingsActivity, /testSources\(test, statuses\)/);
  assert.match(settingsActivity, /SourceRegistry\.LABELS/);
  assert.match(settingsActivity, /UpdateManager\.check/);
  assert.match(manifest, /android:name="\.SettingsActivity"/);
});

test("Android settings menu and radio choices use compact dimensions", () => {
  assert.match(settingsActivity, /MENU_FONT_SP = 12/);
  assert.match(settingsActivity, /TAB_MIN_WIDTH_DP = 52/);
  assert.match(settingsActivity, /TAB_HEIGHT_DP = 28/);
  assert.match(settingsActivity, /RADIO_ROW_HEIGHT_DP = 28/);
  assert.match(settingsActivity, /button\.setMinWidth\(dp\(TAB_MIN_WIDTH_DP\)\)/);
  assert.match(settingsActivity, /button\.setTextSize\(MENU_FONT_SP\)/);
  assert.match(settingsActivity, /option\.setTextSize\(MENU_FONT_SP\)/);
  assert.match(
    settingsActivity,
    /new LinearLayout\.LayoutParams\(ViewGroup\.LayoutParams\.WRAP_CONTENT, dp\(RADIO_ROW_HEIGHT_DP\)\)/,
  );
  assert.doesNotMatch(settingsActivity, /group\.addView\((?:option|automatic), rowParams\(\)\)/);
});

test("Android automatic updates validate manifest, bytes, package, version, and signer", () => {
  assert.match(gradleProperties, /^android\.useAndroidX=true$/m);
  assert.match(sourceRegistry, /UPDATE_MANIFEST_PATH = "api\/artifacts\/update\/android\/webClx"/);
  assert.match(updateManager, /Pattern\.compile\("\[0-9a-f\]\{64\}"\)/);
  assert.match(updateManager, /downloadUrl\.startsWith\("\/api\/artifacts\/download\/"\)/);
  assert.match(updateManager, /EXPECTED_SHA256/);
  assert.match(updateManager, /EXPECTED_SIZE/);
  assert.match(updateManager, /EXPECTED_VERSION_CODE/);
  assert.match(updateManager, /getPackageInfo\(context\.getPackageName\(\), 0\)/);
  assert.match(settingsActivity, /UpdateManager\.currentVersionName\(this\)/);
  assert.doesNotMatch(settingsActivity, /BuildConfig\.VERSION_NAME/);
  assert.match(updateManager, /setDestinationInExternalFilesDir/);
  assert.doesNotMatch(updateManager, /setDestinationInExternalPublicDir/);
  assert.match(activity, /MessageDigest\.getInstance\("SHA-256"\)/);
  assert.match(activity, /copied != expectedSize/);
  assert.match(activity, /getPackageArchiveInfo/);
  assert.match(activity, /getPackageName\(\)\.equals\(archive\.packageName\)/);
  assert.match(activity, /signatureDigests\(archive\)\.equals\(signatureDigests\(installed\)\)/);
  assert.match(activity, /FileProvider\.getUriForFile/);
  assert.match(manifest, /android:name="androidx\.core\.content\.FileProvider"/);
  assert.match(manifest, /android:authorities="\$\{applicationId\}\.updateprovider"/);
  assert.match(manifest, /android:resource="@xml\/update_file_paths"/);
  assert.match(activity, /showUpdateProgress\(downloadId\)/);
});

test("Android checks for updates once on every app start", () => {
  const automaticCheck = activity.match(
    /private void maybeCheckForUpdate\(\) \{([\s\S]*?)\n    \}\n\n    private void showUpdatePrompt/,
  )?.[1] ?? "";
  assert.match(automaticCheck, /automaticUpdateCheckStarted/);
  assert.match(automaticCheck, /UpdateManager\.check\(this, selectedSource/);
  assert.doesNotMatch(automaticCheck, /KEY_LAST_UPDATE_CHECK/);
});

test("Android update transactions stay scoped to their own download id", () => {
  assert.match(updateManager, /static Future<\?> check\(Context context/);
  assert.match(updateManager, /context\.getApplicationContext\(\)/);
  assert.match(updateManager, /static boolean clearPending\(Context context, long downloadId\)/);
  assert.match(
    updateManager,
    /static boolean commitVerifiedApk\(Context context, long downloadId, String stagedPath\)[\s\S]*isCurrentPending\(context, downloadId\)[\s\S]*putLong\(VERIFIED_DOWNLOAD_ID, downloadId\)[\s\S]*putString\(VERIFIED_APK_PATH, stagedPath\)/,
  );
  assert.match(updateManager, /synchronized \(PREFERENCES_LOCK\)/);
  assert.match(
    activity,
    /File\.createTempFile\(\s*"verified-webclx-update-" \+ snapshot\.downloadId \+ "-"[\s\S]*"\.apk"[\s\S]*updateDir/,
    "each verification stages a unique file so stale results cannot collide",
  );
  assert.match(
    activity,
    /!UpdateManager\.isCurrentPending\(this, snapshot\.downloadId\)/,
    "a late verification result must not commit or clear a newer pending download",
  );
  assert.match(
    activity,
    /verifiedDownloadId == downloadId\s*\? preferences\.getString\(UpdateManager\.VERIFIED_APK_PATH/,
    "the staged APK path is only trusted for the download it was verified for",
  );
  assert.doesNotMatch(activity, /UpdateManager\.clearPending\(this\)/);
  assert.match(activity, /updateVerifyExecutor/);
  assert.doesNotMatch(
    activity,
    /resolverExecutor\.execute\(\(\) -> \{\s*int error = validatePendingUpdate/,
    "APK verification must not share the source-resolution executor",
  );
});

test("Android probes servers via fetch only — no synchronous native bridge", () => {
  assert.doesNotMatch(activity, /probeHost/);
  assert.match(serverProbeSource, /async function probeHostViaFetch\(host\)/);
  assert.doesNotMatch(serverProbeSource, /probeHostViaNative|WebClxAndroid/);
  assert.match(
    serverProbeSource,
    /finally \{\s*clearTimeout\(timer\);\s*\}/,
    "the abort timer must be released on success and failure",
  );
});

test("Android update installer is launched only once automatically per download", () => {
  assert.match(updateManager, /INSTALLER_LAUNCHED_DOWNLOAD_ID/);
  assert.match(
    activity,
    /installerLaunchedDownloadId == downloadId/,
  );
  assert.match(
    activity,
    /putLong\(UpdateManager\.INSTALLER_LAUNCHED_DOWNLOAD_ID, downloadId\)[\s\S]*startActivity\(installIntent\)/,
  );
  assert.doesNotMatch(activity, /allowInstallerRelaunch/);
  assert.match(activity, /onResume\(\)[\s\S]*installPendingApk\(true\)/);
});

test("artifact service exposes hashed Android manifests and resumable downloads", () => {
  assert.match(artifactRoutes, /\/api\/artifacts\/update\/android\/\{project\}/);
  assert.match(artifacts, /pub async fn android_update_manifest/);
  assert.match(artifacts, /sha256_file\(&stored\)/);
  assert.match(artifacts, /header::ACCEPT_RANGES/);
  assert.match(artifacts, /StatusCode::PARTIAL_CONTENT/);
  assert.match(artifacts, /StatusCode::RANGE_NOT_SATISFIABLE/);
});
