package com.webclx.app;

import android.content.Context;
import android.content.SharedPreferences;
import java.io.File;
import java.io.FileWriter;

/**
 * JVM behavior harness for the real UpdateManager: exercises the
 * update-transaction identity guards against in-memory Android stubs.
 * Exits non-zero on the first failed assertion.
 */
public final class UpdateStateHarness {
    private static int checks;

    private static void check(boolean condition, String name) {
        checks++;
        if (!condition) {
            throw new AssertionError("FAILED: " + name);
        }
        System.out.println("ok " + checks + " - " + name);
    }

    private static SharedPreferences prefs(Context context) {
        return UpdateManager.preferences(context);
    }

    private static void seedPending(Context context, long id, String sha, long size, long version) {
        UpdateManager.mutatePreferences(context, editor -> editor
            .putLong(UpdateManager.PENDING_APK_DOWNLOAD_ID, id)
            .putString(UpdateManager.EXPECTED_SHA256, sha)
            .putLong(UpdateManager.EXPECTED_SIZE, size)
            .putLong(UpdateManager.EXPECTED_VERSION_CODE, version));
    }

    public static void main(String[] args) throws Exception {
        // All scratch state lives under the caller-provided directory so the
        // invoking script's trap removes everything the harness creates.
        if (args.length == 0) {
            throw new IllegalArgumentException("usage: UpdateStateHarness <work-dir>");
        }
        File workDir = new File(args[0]);
        Context context = new Context();
        File filesDir = new File(workDir, "files");
        filesDir.mkdirs();
        context.setFilesDir(filesDir);

        // --- snapshot only exists for the live pending id --------------------
        check(
            UpdateManager.pendingSnapshot(context, 7) == null,
            "snapshot is null while no pending download exists"
        );
        seedPending(context, 7, "a".repeat(64), 100L, 1008021L);
        UpdateManager.PendingUpdateSnapshot snapshot =
            UpdateManager.pendingSnapshot(context, 7);
        check(snapshot != null, "snapshot exists for the live pending id");
        check(snapshot.downloadId == 7 && snapshot.size == 100L
            && snapshot.versionCode == 1008021L
            && "a".repeat(64).equals(snapshot.sha256),
            "snapshot captured all expected fields");

        // --- snapshot is immutable after preferences move on -----------------
        seedPending(context, 9, "b".repeat(64), 200L, 1008022L);
        check(snapshot.downloadId == 7 && snapshot.size == 100L,
            "snapshot stays immutable after preferences mutate");
        check(UpdateManager.pendingSnapshot(context, 7) == null,
            "snapshot refused for a superseded pending id");
        check(UpdateManager.pendingSnapshot(context, UpdateManager.NO_DOWNLOAD) == null,
            "NO_DOWNLOAD never yields a snapshot");

        // --- commitVerifiedApk is id-scoped ----------------------------------
        File staged = new File(new File(filesDir, "updates"), "verified-webclx-update-9-1.apk");
        staged.getParentFile().mkdirs();
        try (FileWriter writer = new FileWriter(staged)) {
            writer.write("apk");
        }
        check(!UpdateManager.commitVerifiedApk(context, 7, "/tmp/stale.apk"),
            "stale id cannot commit a verified path");
        check(prefs(context).getLong(UpdateManager.VERIFIED_DOWNLOAD_ID, -2) == -2,
            "stale commit wrote no verified id");
        check(prefs(context).getString(UpdateManager.VERIFIED_APK_PATH, null) == null,
            "stale commit wrote no verified path");
        check(UpdateManager.commitVerifiedApk(context, 9, staged.getAbsolutePath()),
            "current id commits the verified path");
        check(prefs(context).getLong(UpdateManager.VERIFIED_DOWNLOAD_ID, -2) == 9,
            "commit recorded the verified id");

        // --- enqueue/reset clears verified state for the new transaction -----
        UpdateManager.UpdateManifest manifest = new UpdateManager.UpdateManifest(
            0, "1.8.22", 1008022L, "webclx.apk", "c".repeat(64), 300L, "", "http://x/apk"
        );
        long newId = UpdateManager.enqueue(context, manifest);
        check(newId != UpdateManager.NO_DOWNLOAD, "enqueue returned a download id");
        check(prefs(context).getLong(UpdateManager.PENDING_APK_DOWNLOAD_ID, -2) == newId,
            "enqueue set the new pending id");
        check(prefs(context).getString(UpdateManager.VERIFIED_APK_PATH, null) == null,
            "enqueue cleared the stale VERIFIED_APK_PATH");
        check(prefs(context).getLong(UpdateManager.VERIFIED_DOWNLOAD_ID, -2)
            == UpdateManager.NO_DOWNLOAD,
            "enqueue cleared the stale verified id");

        // --- clearPending is id-scoped; NO_DOWNLOAD is not a wildcard --------
        check(!UpdateManager.clearPending(context, UpdateManager.NO_DOWNLOAD),
            "NO_DOWNLOAD does not clear the pending transaction");
        check(UpdateManager.pendingDownloadId(context) == newId,
            "pending id survived the NO_DOWNLOAD clear");
        check(!UpdateManager.clearPending(context, newId + 1),
            "a foreign id cannot clear the pending transaction");
        check(UpdateManager.pendingDownloadId(context) == newId,
            "pending id survived the foreign-id clear");

        // --- canonical safe-path deletion refuses arbitrary paths ------------
        File outside = new File(filesDir, "not-an-update.apk");
        try (FileWriter writer = new FileWriter(outside)) {
            writer.write("not-an-update");
        }
        UpdateManager.mutatePreferences(context, editor -> editor
            .putLong(UpdateManager.VERIFIED_DOWNLOAD_ID, newId)
            .putString(UpdateManager.VERIFIED_APK_PATH, outside.getAbsolutePath()));
        check(UpdateManager.clearPending(context, newId),
            "id-scoped clear succeeds for the live pending id");
        check(outside.exists(),
            "clearPending refuses to delete a path outside files/updates");
        outside.delete();

        // A file inside files/updates IS removed.
        File inside = new File(filesDir, "updates");
        File victim = new File(inside, "verified-webclx-update-1-9.apk");
        inside.mkdirs();
        try (FileWriter writer = new FileWriter(victim)) {
            writer.write("apk");
        }
        seedPending(context, 42, "d".repeat(64), 10L, 1008023L);
        UpdateManager.mutatePreferences(context, editor -> editor
            .putLong(UpdateManager.VERIFIED_DOWNLOAD_ID, 42)
            .putString(UpdateManager.VERIFIED_APK_PATH, victim.getAbsolutePath()));
        check(UpdateManager.clearPending(context, 42),
            "clear removes a transaction whose staged file is in updates/");
        check(!victim.exists(), "staged file inside updates/ was deleted");
        check(UpdateManager.pendingDownloadId(context) == UpdateManager.NO_DOWNLOAD,
            "pending state fully cleared");

        // isCurrentPending mirrors pendingDownloadId exactly.
        seedPending(context, 55, "e".repeat(64), 10L, 1008024L);
        check(UpdateManager.isCurrentPending(context, 55),
            "isCurrentPending true for the live id");
        check(!UpdateManager.isCurrentPending(context, UpdateManager.NO_DOWNLOAD),
            "isCurrentPending false for NO_DOWNLOAD");
        check(!UpdateManager.isCurrentPending(context, 54),
            "isCurrentPending false for a stale id");

        System.out.println("update-state harness: " + checks + " checks passed");
    }

    private UpdateStateHarness() {}
}
