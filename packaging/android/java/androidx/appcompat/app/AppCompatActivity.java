package androidx.appcompat.app;

import android.app.Activity;

/**
 * Stand-in for AppCompat's AppCompatActivity. GameActivity 4.4.0 only extends it and calls the
 * lifecycle methods every Activity has (onCreate, onStart, onKeyDown, ...), so a plain Activity
 * is enough; PaperCraft uses no AppCompat theme, action bar or fragment.
 */
public class AppCompatActivity extends Activity {}
