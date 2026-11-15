package com.webclx.app;

import android.app.Activity;
import android.view.View;

import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;

final class WindowInsetsLayout {
    interface ImeVisibilityListener {
        void onVisibilityChanged(boolean visible);
    }

    private WindowInsetsLayout() {}

    static void install(Activity activity, View root, ImeVisibilityListener imeListener) {
        // One owner on every API, including Android 15's enforced edge-to-edge.
        WindowCompat.setDecorFitsSystemWindows(activity.getWindow(), false);
        int initialLeft = root.getPaddingLeft();
        int initialTop = root.getPaddingTop();
        int initialRight = root.getPaddingRight();
        int initialBottom = root.getPaddingBottom();
        ViewCompat.setOnApplyWindowInsetsListener(root, (view, insets) -> {
            if (imeListener != null) {
                imeListener.onVisibilityChanged(insets.isVisible(WindowInsetsCompat.Type.ime()));
            }
            // The type union takes the maximum per edge, never IME + navigation bar.
            Insets safe = insets.getInsets(
                WindowInsetsCompat.Type.systemBars()
                    | WindowInsetsCompat.Type.displayCutout()
                    | WindowInsetsCompat.Type.ime()
            );
            int left = initialLeft + safe.left;
            int top = initialTop + safe.top;
            int right = initialRight + safe.right;
            int bottom = initialBottom + safe.bottom;
            if (view.getPaddingLeft() != left || view.getPaddingTop() != top
                || view.getPaddingRight() != right || view.getPaddingBottom() != bottom) {
                view.setPadding(left, top, right, bottom);
            }
            // Children receive an already-safe viewport, including the WebView.
            return WindowInsetsCompat.CONSUMED;
        });
        ViewCompat.requestApplyInsets(root);
    }
}
