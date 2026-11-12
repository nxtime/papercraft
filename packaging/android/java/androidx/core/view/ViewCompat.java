package androidx.core.view;

import android.view.View;
import android.view.WindowInsets;

/** Stand-in for AndroidX core's ViewCompat: the three calls GameActivity 4.4.0 makes. */
public final class ViewCompat {
    private ViewCompat() {}

    /** The insets at the root of `view`'s window, or null while it is not attached. */
    public static WindowInsetsCompat getRootWindowInsets(View view) {
        WindowInsets insets = view.getRootWindowInsets();
        return insets == null ? null : new WindowInsetsCompat(insets);
    }

    /** Routes `view`'s inset updates to `listener`. */
    public static void setOnApplyWindowInsetsListener(View view, final OnApplyWindowInsetsListener listener) {
        if (listener == null) {
            view.setOnApplyWindowInsetsListener(null);
            return;
        }
        view.setOnApplyWindowInsetsListener(new View.OnApplyWindowInsetsListener() {
            @Override
            public WindowInsets onApplyWindowInsets(View target, WindowInsets insets) {
                return listener.onApplyWindowInsets(target, new WindowInsetsCompat(insets)).toWindowInsets();
            }
        });
    }

    /** A fresh view id. */
    public static int generateViewId() {
        return View.generateViewId();
    }
}
