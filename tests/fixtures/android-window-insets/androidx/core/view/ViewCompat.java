package androidx.core.view;

import android.view.View;
import java.util.WeakHashMap;

public final class ViewCompat {
    public interface OnApplyWindowInsetsListener {
        WindowInsetsCompat onApplyWindowInsets(View view, WindowInsetsCompat insets);
    }

    private static final WeakHashMap<View, OnApplyWindowInsetsListener> listeners =
        new WeakHashMap<>();

    public static void setOnApplyWindowInsetsListener(View view, OnApplyWindowInsetsListener listener) {
        listeners.put(view, listener);
    }

    public static void requestApplyInsets(View view) {
        view.insetRequests++;
    }

    public static WindowInsetsCompat dispatchApplyWindowInsets(View view, WindowInsetsCompat insets) {
        return listeners.get(view).onApplyWindowInsets(view, insets);
    }
}
