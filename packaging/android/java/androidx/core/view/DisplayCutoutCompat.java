package androidx.core.view;

import android.os.Build;
import android.view.DisplayCutout;

import androidx.core.graphics.Insets;

/** Stand-in for AndroidX core's DisplayCutoutCompat: the waterfall insets GameActivity asks for. */
public final class DisplayCutoutCompat {
    private final DisplayCutout platform;

    DisplayCutoutCompat(DisplayCutout platform) {
        this.platform = platform;
    }

    /** The curved-edge (waterfall) insets; none below API 30. */
    public Insets getWaterfallInsets() {
        if (Build.VERSION.SDK_INT < 30) {
            return Insets.NONE;
        }
        return Insets.of(platform.getWaterfallInsets());
    }
}
