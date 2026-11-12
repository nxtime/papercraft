package androidx.core.graphics;

/**
 * Stand-in for AndroidX core's Insets, only what GameActivity 4.4.0 uses: its Java code reads
 * {@link #NONE} and its native glue reads the four int fields over JNI (names must not change).
 * PaperCraft ships these few classes instead of AppCompat and AndroidX core (twenty-odd AARs with
 * resources, which would need Gradle); see docs/ARCHITECTURE.md, Android shell.
 */
public final class Insets {
    /** No inset on any side. */
    public static final Insets NONE = new Insets(0, 0, 0, 0);

    public final int left;
    public final int top;
    public final int right;
    public final int bottom;

    private Insets(int left, int top, int right, int bottom) {
        this.left = left;
        this.top = top;
        this.right = right;
        this.bottom = bottom;
    }

    /** Insets with these sides, in pixels. */
    public static Insets of(int left, int top, int right, int bottom) {
        if (left == 0 && top == 0 && right == 0 && bottom == 0) {
            return NONE;
        }
        return new Insets(left, top, right, bottom);
    }

    /** The platform insets (API 29+) as these. */
    public static Insets of(android.graphics.Insets platform) {
        return of(platform.left, platform.top, platform.right, platform.bottom);
    }

    @Override
    public boolean equals(Object other) {
        if (!(other instanceof Insets)) {
            return false;
        }
        Insets that = (Insets) other;
        return left == that.left && top == that.top && right == that.right && bottom == that.bottom;
    }

    @Override
    public int hashCode() {
        return ((left * 31 + top) * 31 + right) * 31 + bottom;
    }
}
