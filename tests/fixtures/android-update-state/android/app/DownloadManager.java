package android.app;

import android.content.Context;
import android.net.Uri;

/** DownloadManager stub: enqueue returns an incrementing id, no real IO. */
public class DownloadManager {
    public static final String ACTION_DOWNLOAD_COMPLETE = "android.intent.action.DOWNLOAD_COMPLETE";
    public static final String EXTRA_DOWNLOAD_ID = "extra_download_id";
    public static final String COLUMN_STATUS = "status";
    public static final String COLUMN_BYTES_DOWNLOADED_SO_FAR = "bytes_so_far";
    public static final String COLUMN_TOTAL_SIZE_BYTES = "total_size";
    public static final int STATUS_SUCCESSFUL = 8;
    public static final int STATUS_FAILED = 16;

    private long nextId = 1000;
    private int enqueueCount;

    public long enqueue(Request request) {
        enqueueCount++;
        return nextId++;
    }

    public int enqueueCount() {
        return enqueueCount;
    }

    public static class Query {
        public Query setFilterById(long... ids) {
            return this;
        }
    }

    public static class Request {
        public static final int VISIBILITY_VISIBLE_NOTIFY_COMPLETED = 1;

        public Request(Uri uri) {}

        public Request setTitle(CharSequence title) { return this; }
        public Request setDescription(CharSequence description) { return this; }
        public Request setMimeType(String mimeType) { return this; }
        public Request setNotificationVisibility(int visibility) { return this; }
        public Request setVisibleInDownloadsUi(boolean visible) { return this; }
        public Request setDestinationInExternalFilesDir(Context context, String dir, String sub) {
            return this;
        }
    }
}
