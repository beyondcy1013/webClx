package androidx.core.view;

import androidx.core.graphics.Insets;

// Immutable event double: platform/AndroidX inset detection is outside this harness.
public final class WindowInsetsCompat {
    public static final WindowInsetsCompat CONSUMED = new WindowInsetsCompat(
        Insets.NONE, Insets.NONE, Insets.NONE, false
    );
    private final Insets bars;
    private final Insets cutout;
    private final Insets ime;
    private final boolean imeVisible;

    public WindowInsetsCompat(Insets bars, Insets cutout, Insets ime, boolean imeVisible) {
        this.bars = bars;
        this.cutout = cutout;
        this.ime = ime;
        this.imeVisible = imeVisible;
    }

    public Insets getInsets(int types) {
        int left = 0;
        int top = 0;
        int right = 0;
        int bottom = 0;
        Insets[] values = {bars, cutout, ime};
        for (int index = 0; index < values.length; index++) {
            if ((types & (1 << index)) != 0) {
                Insets value = values[index];
                left = Math.max(left, value.left);
                top = Math.max(top, value.top);
                right = Math.max(right, value.right);
                bottom = Math.max(bottom, value.bottom);
            }
        }
        return Insets.of(left, top, right, bottom);
    }

    public boolean isVisible(int types) {
        return (types & Type.ime()) != 0 && imeVisible;
    }

    public static final class Type {
        public static int systemBars() { return 1; }
        public static int displayCutout() { return 2; }
        public static int ime() { return 4; }
    }
}
