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

test("Android client races every configured source and selects the fastest healthy result", () => {
  for (const source of [
    "http://192.168.3.2:11111/",
    "http://fpsq.xyz:11112/",
  ]) {
    assert.match(sourceRegistry, new RegExp(`"${source.replaceAll(".", "\\.")}"`));
  }
  assert.doesNotMatch(sourceRegistry, /"http:\/\/fpsq\.xyz:11111\/"/);
  assert.doesNotMatch(sourceRegistry, /webclx\.739964\.xyz/);
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
    /if \(foreground\) \{\s*showConnectionFailure\(\);\s*\} else \{\s*sourceResolutionInProgress = false;/,
  );
});

test("Android WebView keeps navigation and cleartext access on known data sources", () => {
  assert.match(activity, /isTrustedOrigin\(uri\)/);
  assert.match(activity, /Intent\.ACTION_VIEW/);
  assert.match(networkPolicy, /<base-config cleartextTrafficPermitted="false"/);
  assert.match(networkPolicy, />192\.168\.3\.2</);
  assert.match(networkPolicy, />fpsq\.xyz</);
  assert.doesNotMatch(manifest, /android:usesCleartextTraffic="true"/);
  assert.match(manifest, /android\.permission\.INTERNET/);
  assert.match(manifest, /android:networkSecurityConfig="@xml\/network_security_config"/);
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
    /root\.setBackgroundColor\(contentBackgroundColor\);[\s\S]*webView = new WebView\(this\);[\s\S]*webView\.setBackgroundColor\(contentBackgroundColor\);/,
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
    /root\.addView\(webView, new FrameLayout\.LayoutParams\(\s*ViewGroup\.LayoutParams\.MATCH_PARENT,\s*ViewGroup\.LayoutParams\.MATCH_PARENT\s*\)\)/,
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
