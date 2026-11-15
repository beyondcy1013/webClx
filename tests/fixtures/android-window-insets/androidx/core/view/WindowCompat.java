package androidx.core.view;

import android.view.Window;

public final class WindowCompat {
    public static void setDecorFitsSystemWindows(Window window, boolean fits) {
        window.decorFitsSystemWindows = fits;
    }
}
