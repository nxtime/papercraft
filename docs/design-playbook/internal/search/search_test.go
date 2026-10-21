package search

import (
	"strings"
	"testing"
	"testing/fstest"
)

var fsys = fstest.MapFS{
	"README.md": {Data: []byte("# Title\nintro\n")},
	"principles/a.md": {Data: []byte("### INT-06 Undo over confirm\nPrefer undo to a confirmation dialog for delete.\n" +
		"```\n# not a heading\n```\n### MOT-07 Reduced motion\nUse 1ms, not none, for view transitions.\n")},
}

func TestSectionsSkipFences(t *testing.T) {
	secs, err := Sections(fsys)
	if err != nil {
		t.Fatal(err)
	}
	for _, s := range secs {
		if s.Title == "not a heading" {
			t.Fatal("fenced line parsed as heading")
		}
	}
	if len(ByID(secs, "int-06")) != 1 || len(ByID(secs, "MOT")) != 1 || len(ByID(secs, "NOPE-99")) != 0 {
		t.Fatal("id and prefix lookup wrong")
	}
}

func TestRankFindsRuleWithStemming(t *testing.T) {
	secs, _ := Sections(fsys)
	hits := Rank("reduced motion view transition frozen", secs, 1)
	if len(hits) != 1 || hits[0].ID != "MOT-07" {
		t.Fatalf("got %+v", hits)
	}
	if h := Rank("confirmations of deleting", secs, 1); len(h) != 1 || h[0].ID != "INT-06" {
		t.Fatalf("stemming failed: %+v", h)
	}
	if len(Rank("   ", secs, 3)) != 0 {
		t.Fatal("empty query must return nothing")
	}
}

func TestStem(t *testing.T) {
	for in, want := range map[string]string{"transitions": "transition", "scrolling": "scroll", "class": "class", "css": "css"} {
		if got := Stem(in); got != want {
			t.Errorf("Stem(%q)=%q want %q", in, got, want)
		}
	}
	_ = strings.ToLower
}
