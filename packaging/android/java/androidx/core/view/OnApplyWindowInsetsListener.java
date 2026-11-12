package androidx.core.view;

import android.view.View;

/** Stand-in for AndroidX core's OnApplyWindowInsetsListener (GameActivity implements it). */
public interface OnApplyWindowInsetsListener {
    /** Called when `view` receives new insets; answers the insets to pass on. */
    WindowInsetsCompat onApplyWindowInsets(View view, WindowInsetsCompat insets);
}
