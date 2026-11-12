package dev.marcospaulo.papercraft;

import android.os.Bundle;
import android.text.InputType;
import android.view.View;
import android.view.inputmethod.EditorInfo;

import androidx.core.graphics.Insets;
import androidx.core.view.WindowInsetsCompat;

import com.google.androidgamesdk.GameActivity;
import com.google.androidgamesdk.gametextinput.State;

/**
 * The PaperCraft activity: GameActivity (the Rust app in libpapercraft_android.so, with a real
 * InputConnection for the soft keyboard) plus the system bar and keyboard insets for the Rust
 * side ({@link #safeInsets}) and the keyboard buffer helpers the Rust side reads each frame.
 */
public final class PaperActivity extends GameActivity {
    /** Left, top, right, bottom in pixels: system bars and cutout, the keyboard at the bottom. */
    private volatile int[] safe = new int[4];
    /** Whether the keyboard has written into GameTextInput's buffer (its native text is null before). */
    private volatile boolean keyboardWrote;
    /** The keyboard buffer as the keyboard last left it. UI thread only. */
    private State latest = new State("", 0, 0, -1, -1);
    /** Whether a {@link #consumeInput} is waiting for the UI thread; Rust does not read the buffer meanwhile. */
    private volatile boolean consumePending;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        // After super.onCreate: the call reaches the surface view's input connection. No auto-capitals:
        // the buffer is emptied after each commit, so every letter would look like a sentence start.
        setImeEditorInfoFields(
                InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_MULTI_LINE,
                EditorInfo.IME_ACTION_NONE,
                EditorInfo.IME_FLAG_NO_FULLSCREEN | EditorInfo.IME_FLAG_NO_ENTER_ACTION);
    }

    @Override
    public WindowInsetsCompat onApplyWindowInsets(View view, WindowInsetsCompat insets) {
        Insets bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() | WindowInsetsCompat.Type.displayCutout());
        Insets keyboard = insets.getInsets(WindowInsetsCompat.Type.ime());
        int[] next = {bars.left, bars.top, bars.right, Math.max(bars.bottom, keyboard.bottom)};
        boolean changed = !java.util.Arrays.equals(next, safe);
        safe = next;
        WindowInsetsCompat answer = super.onApplyWindowInsets(view, insets);
        // winit ignores GameActivity's insets event, so ask for a frame that lays out with the new ones.
        if (changed && mSurfaceView != null) {
            surfaceRedrawNeeded(mSurfaceView.getHolder());
        }
        return answer;
    }

    @Override
    public void stateChanged(State state, boolean dismissed) {
        keyboardWrote = true;
        latest = state;
        super.stateChanged(state, dismissed);
    }

    /** Whether the keyboard buffer can be read: false until the keyboard first changed it. Called from Rust. */
    public boolean keyboardWrote() {
        return keyboardWrote;
    }

    /**
     * Takes text the app has read out of the keyboard buffer: drops {@code before} from its start and
     * {@code after} from its end, when they are still there. Runs on the UI thread, between the
     * keyboard's own edits, so letters typed after Rust read the buffer stay in it. Called from Rust.
     */
    public void consumeInput(final String before, final String after) {
        consumePending = true;
        runOnUiThread(() -> {
            try {
                trimInput(before, after);
            } finally {
                consumePending = false;
            }
        });
    }

    /** Whether a {@link #consumeInput} has not reached the buffer yet. Called from Rust each frame. */
    public boolean inputPending() {
        return consumePending;
    }

    private void trimInput(String before, String after) {
        State state = latest;
        String text = state.text == null ? "" : state.text;
        int start = text.startsWith(before) ? before.length() : 0;
        int end = text.length();
        if (!after.isEmpty() && text.endsWith(after) && end - after.length() >= start) {
            end -= after.length();
        }
        if (start == 0 && end == text.length()) {
            return;
        }
        String rest = text.substring(start, end);
        int caret = clamp(state.selectionEnd - start, rest.length());
        int composeStart = -1;
        int composeEnd = -1;
        if (state.composingRegionStart >= 0 && state.composingRegionEnd > state.composingRegionStart) {
            composeStart = clamp(state.composingRegionStart - start, rest.length());
            composeEnd = clamp(state.composingRegionEnd - start, rest.length());
            if (composeStart >= composeEnd) {
                composeStart = -1;
                composeEnd = -1;
            }
        }
        State next = new State(rest, caret, caret, composeStart, composeEnd);
        setTextInputState(next);
        // setState updates the keyboard but not the native copy Rust reads; report it like a keyboard edit.
        stateChanged(next, false);
    }

    private static int clamp(int value, int max) {
        return Math.max(0, Math.min(value, max));
    }

    /** The area the app must keep clear, in pixels (left, top, right, bottom). Called from Rust each frame. */
    public int[] safeInsets() {
        return safe.clone();
    }
}
