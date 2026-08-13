package com.webclx.app;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;

final class AppPreferences {
    static final String FILE = "app_settings";
    static final String KEY_THEME_MODE = "theme.mode";
    static final String KEY_TEXT_ZOOM = "display.text_zoom";
    static final String KEY_START_PATH = "general.start_path";
    static final String KEY_SOURCE_MODE = "data_source.mode";
    static final String KEY_AUTO_UPDATE = "update.auto_check";
    static final String KEY_ACTIVE_SOURCE = "data_source.active_index";
    static final String KEY_ACTIVE_SOURCE_AT = "data_source.active_at";

    static final String THEME_SYSTEM = "system";
    static final String THEME_LIGHT = "light";
    static final String THEME_DARK = "dark";
    static final String SOURCE_AUTO = "auto";

    private AppPreferences() {}

    static SharedPreferences get(Context context) {
        return context.getSharedPreferences(FILE, Context.MODE_PRIVATE);
    }

    static void applyTheme(Activity activity) {
        switch (themeMode(activity)) {
            case THEME_LIGHT:
                activity.setTheme(R.style.Theme_WebClx_Light);
                break;
            case THEME_DARK:
                activity.setTheme(R.style.Theme_WebClx_Dark);
                break;
            default:
                activity.setTheme(R.style.Theme_WebClx);
                break;
        }
    }

    static String themeMode(Context context) {
        String value = get(context).getString(KEY_THEME_MODE, THEME_SYSTEM);
        if (THEME_LIGHT.equals(value) || THEME_DARK.equals(value)) {
            return value;
        }
        return THEME_SYSTEM;
    }

    static int textZoom(Context context) {
        int value = get(context).getInt(KEY_TEXT_ZOOM, 100);
        return value == 85 || value == 100 || value == 115 || value == 130 ? value : 100;
    }

    static String startPath(Context context) {
        String value = get(context).getString(KEY_START_PATH, "terminal");
        if ("workspace".equals(value) || "downloads".equals(value)) {
            return value;
        }
        return "terminal";
    }

    static String sourceMode(Context context) {
        String value = get(context).getString(KEY_SOURCE_MODE, SOURCE_AUTO);
        if (SOURCE_AUTO.equals(value)) {
            return value;
        }
        try {
            int index = Integer.parseInt(value);
            return SourceRegistry.isValidIndex(index) ? value : SOURCE_AUTO;
        } catch (NumberFormatException ignored) {
            return SOURCE_AUTO;
        }
    }

    static int preferredSource(Context context) {
        String value = sourceMode(context);
        return SOURCE_AUTO.equals(value) ? -1 : Integer.parseInt(value);
    }

    static boolean autoUpdate(Context context) {
        return get(context).getBoolean(KEY_AUTO_UPDATE, true);
    }
}
