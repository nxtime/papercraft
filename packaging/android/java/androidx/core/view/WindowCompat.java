package androidx.core.view;

import android.os.Build;
import android.view.Window;

/**
 * Stand-in for AndroidX core's WindowCompat. Edge to edge only on API 30+, where
 * {@link WindowInsetsCompat} reports the real insets; below 30 the window keeps fitting inside the
 * system bars (AndroidX would use deprecated system-UI flags there).
 */
public final class WindowCompat {
    private WindowCompat() {}

    /** Whether the window's decor keeps its content inside the system bars. */
    public static void setDecorFitsSystemWindows(Window window, boolean fits) {
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(fits);
        }
    }
}
