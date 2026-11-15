package com.webclx.app;

import android.app.Activity;
import android.view.View;
import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;
import java.util.ArrayList;
import java.util.List;

public final class WindowInsetsHarness {
    private static int checks;

    private static void check(boolean condition, String name) {
        if (!condition) {
            throw new AssertionError(name);
        }
        System.out.println("ok " + (++checks) + " - " + name);
    }

    private static void padding(View view, int left, int top, int right, int bottom, String name) {
        check(view.getPaddingLeft() == left && view.getPaddingTop() == top
            && view.getPaddingRight() == right && view.getPaddingBottom() == bottom, name);
    }

    public static void main(String[] args) {
        Activity activity = new Activity();
        View root = new View();
        root.setPadding(3, 5, 7, 11);
        List<Boolean> keyboardEvents = new ArrayList<>();
        WindowInsetsLayout.install(activity, root, keyboardEvents::add);
        check(!activity.getWindow().decorFitsSystemWindows, "root is the only inset owner");
        check(root.insetRequests == 1, "initial window insets requested");

        WindowInsetsCompat portrait = new WindowInsetsCompat(
            Insets.of(0, 24, 0, 48), Insets.of(0, 40, 0, 0), Insets.NONE, false
        );
        WindowInsetsCompat child = ViewCompat.dispatchApplyWindowInsets(root, portrait);
        padding(root, 3, 45, 7, 59, "portrait clears status bar, taller cutout and navigation bar");
        check(child == WindowInsetsCompat.CONSUMED, "WebView cannot apply the same insets twice");
        check(keyboardEvents.equals(List.of(false)), "closed IME published before consumption");

        int writes = root.paddingWrites;
        for (int repeat = 0; repeat < 10; repeat++) {
            ViewCompat.dispatchApplyWindowInsets(root, portrait);
        }
        check(root.paddingWrites == writes, "repeated insets do not accumulate or rewrite padding");

        WindowInsetsCompat keyboard = new WindowInsetsCompat(
            Insets.of(0, 24, 0, 48), Insets.of(0, 40, 0, 0), Insets.of(0, 0, 0, 300), true
        );
        ViewCompat.dispatchApplyWindowInsets(root, keyboard);
        padding(root, 3, 45, 7, 311, "IME replaces bottom bar inset instead of adding its height");
        check(keyboardEvents.get(keyboardEvents.size() - 1), "open IME reaches native bridge state");

        ViewCompat.dispatchApplyWindowInsets(root, portrait);
        padding(root, 3, 45, 7, 59, "Back dismissal restores the original safe viewport");
        check(!keyboardEvents.get(keyboardEvents.size() - 1), "Back dismissal clears native IME state");

        WindowInsetsCompat landscape = new WindowInsetsCompat(
            Insets.of(0, 0, 24, 0), Insets.of(80, 0, 0, 0), Insets.NONE, false
        );
        ViewCompat.dispatchApplyWindowInsets(root, landscape);
        padding(root, 83, 5, 31, 11, "rotation moves cutout and navigation safety to the sides");

        WindowInsetsCompat floatingKeyboard = new WindowInsetsCompat(
            Insets.of(0, 24, 0, 48), Insets.NONE, Insets.NONE, true
        );
        ViewCompat.dispatchApplyWindowInsets(root, floatingKeyboard);
        padding(root, 3, 29, 7, 59, "floating IME leaves the available viewport unchanged");
        check(keyboardEvents.get(keyboardEvents.size() - 1), "IME visibility is not guessed from height");

        ViewCompat.dispatchApplyWindowInsets(root, WindowInsetsCompat.CONSUMED);
        padding(root, 3, 5, 7, 11, "cleared insets restore baseline padding");

        Activity settings = new Activity();
        View settingsRoot = new View();
        WindowInsetsLayout.install(settings, settingsRoot, null);
        ViewCompat.dispatchApplyWindowInsets(settingsRoot, portrait);
        padding(settingsRoot, 0, 40, 0, 48, "settings uses the same policy without an IME subscriber");
        padding(root, 3, 5, 7, 11, "independent roots cannot change each other's safe area");
        System.out.println("window-insets harness: " + checks + " checks passed");
    }
}
