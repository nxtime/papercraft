package androidx.core.view;

import android.os.Build;
import android.view.WindowInsets;

import androidx.core.graphics.Insets;

/**
 * Stand-in for AndroidX core's WindowInsetsCompat, only what GameActivity 4.4.0 calls. The insets
 * are real on API 30+ (where the window draws edge to edge, see {@link WindowCompat}); below 30
 * the window still fits inside the system bars, so every inset reads as {@link Insets#NONE} and
 * the content rectangle alone places the app.
 */
public final class WindowInsetsCompat {
    private final WindowInsets platform;

    WindowInsetsCompat(WindowInsets platform) {
        this.platform = platform;
    }

    /** The inset of `typeMask` (an OR of {@link Type} values). */
    public Insets getInsets(int typeMask) {
        if (Build.VERSION.SDK_INT < 30) {
            return Insets.NONE;
        }
        return Insets.of(platform.getInsets(Type.toPlatform(typeMask)));
    }

    /** Whether the bars or panels of `typeMask` are showing (false below API 30). */
    public boolean isVisible(int typeMask) {
        return Build.VERSION.SDK_INT >= 30 && platform.isVisible(Type.toPlatform(typeMask));
    }

    /** The platform insets this wraps; never null. */
    public WindowInsets toWindowInsets() {
        return platform;
    }

    /** The display cutout, or null when the screen has none. */
    public DisplayCutoutCompat getDisplayCutout() {
        android.view.DisplayCutout cutout = platform.getDisplayCutout();
        return cutout == null ? null : new DisplayCutoutCompat(cutout);
    }

    /**
     * Inset types, with AndroidX's bit values. The native glue looks these nine methods up by name
     * (GameActivity.cpp); keep every one.
     */
    public static final class Type {
        private static final int STATUS_BARS = 1;
        private static final int NAVIGATION_BARS = 1 << 1;
        private static final int CAPTION_BAR = 1 << 2;
        private static final int IME = 1 << 3;
        private static final int SYSTEM_GESTURES = 1 << 4;
        private static final int MANDATORY_SYSTEM_GESTURES = 1 << 5;
        private static final int TAPPABLE_ELEMENT = 1 << 6;
        private static final int DISPLAY_CUTOUT = 1 << 7;

        private Type() {}

        public static int statusBars() { return STATUS_BARS; }
        public static int navigationBars() { return NAVIGATION_BARS; }
        public static int captionBar() { return CAPTION_BAR; }
        public static int ime() { return IME; }
        public static int systemGestures() { return SYSTEM_GESTURES; }
        public static int mandatorySystemGestures() { return MANDATORY_SYSTEM_GESTURES; }
        public static int tappableElement() { return TAPPABLE_ELEMENT; }
        public static int displayCutout() { return DISPLAY_CUTOUT; }
        public static int systemBars() { return STATUS_BARS | NAVIGATION_BARS | CAPTION_BAR; }

        /** The platform's `WindowInsets.Type` mask for an AndroidX mask (API 30+). */
        static int toPlatform(int mask) {
            int out = 0;
            if ((mask & STATUS_BARS) != 0) out |= WindowInsets.Type.statusBars();
            if ((mask & NAVIGATION_BARS) != 0) out |= WindowInsets.Type.navigationBars();
            if ((mask & CAPTION_BAR) != 0) out |= WindowInsets.Type.captionBar();
            if ((mask & IME) != 0) out |= WindowInsets.Type.ime();
            if ((mask & SYSTEM_GESTURES) != 0) out |= WindowInsets.Type.systemGestures();
            if ((mask & MANDATORY_SYSTEM_GESTURES) != 0) out |= WindowInsets.Type.mandatorySystemGestures();
            if ((mask & TAPPABLE_ELEMENT) != 0) out |= WindowInsets.Type.tappableElement();
            if ((mask & DISPLAY_CUTOUT) != 0) out |= WindowInsets.Type.displayCutout();
            return out;
        }
    }
}
