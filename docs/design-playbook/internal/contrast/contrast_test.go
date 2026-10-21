package contrast

import (
	"bytes"
	"math"
	"strings"
	"testing"
	"testing/fstest"
)

func ratio(t *testing.T, a, b string) float64 {
	t.Helper()
	r, err := Ratio(a, b)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

func TestBlackWhite(t *testing.T) {
	if math.Abs(ratio(t, "#000", "#fff")-21) > 0.01 {
		t.Fatal("black on white should be 21:1")
	}
}

func TestKnownGrayBoundary(t *testing.T) {
	if ratio(t, "#767676", "#ffffff") < 4.5 {
		t.Fatal("#767676 on white must pass AA")
	}
	if ratio(t, "#777777", "#ffffff") >= 4.5 {
		t.Fatal("#777777 on white must fail AA")
	}
}

func TestSymmetry(t *testing.T) {
	if math.Abs(ratio(t, "#123456", "#fedcba")-ratio(t, "#fedcba", "#123456")) > 1e-9 {
		t.Fatal("ratio must be symmetric")
	}
}

func TestTranslucentForegroundIsFlattened(t *testing.T) {
	if r := ratio(t, "#00000080", "#ffffff"); r >= 8 || r <= 3 {
		t.Fatalf("50%% black on white = %.2f", r)
	}
}

func TestRejectsBadInput(t *testing.T) {
	if _, err := Ratio("blue", "#fff"); err == nil {
		t.Fatal("named color must be rejected")
	}
	if _, err := Ratio("#000", "#ffffff80"); err == nil {
		t.Fatal("translucent background must be rejected")
	}
}

func tokenFile(fg string) fstest.MapFS {
	return fstest.MapFS{"t.json": {Data: []byte(`{
	  "color": {"$type": "color", "a": {"$value": "` + fg + `"}, "b": {"$value": "#ffffff"},
	            "c": {"$value": "{color.a}"}},
	  "$extensions": {"dev.playbook": {"contrastPairs": [["color.c", "color.b", 4.5, "alias"]]}}}`)}}
}

func TestTokenPairs(t *testing.T) {
	var out bytes.Buffer
	if _, failed, err := CheckTokens(tokenFile("#767676"), "t.json", &out); err != nil || failed != 0 {
		t.Fatalf("expected pass, failed=%d err=%v\n%s", failed, err, out.String())
	}
	out.Reset()
	if _, failed, _ := CheckTokens(tokenFile("#777777"), "t.json", &out); failed != 1 || !strings.Contains(out.String(), "FAIL") {
		t.Fatalf("expected one failure\n%s", out.String())
	}
}
