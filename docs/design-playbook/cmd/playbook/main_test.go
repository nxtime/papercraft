package main

import (
	"bytes"
	"strings"
	"testing"
)

func do(args ...string) (int, string, string) {
	var o, e bytes.Buffer
	c := run(args, &o, &e)
	return c, o.String(), e.String()
}

func TestRuleLookup(t *testing.T) {
	if c, out, _ := do("rule", "A11Y-10"); c != 0 || !strings.Contains(out, "Visible, unobscured focus") {
		t.Fatalf("rule A11Y-10: %d %q", c, out)
	}
	if _, out, _ := do("rule", "MOT"); !strings.Contains(out, "MOT-07") {
		t.Fatal("prefix lookup failed")
	}
	if c, _, _ := do("rule", "NOPE-99"); c != 1 {
		t.Fatal("unknown id must exit 1")
	}
}

func TestSearchFindsUndoRule(t *testing.T) {
	if _, out, _ := do("search", "-n", "3", "confirmation dialog for delete undo"); !strings.Contains(out, "INT-06") {
		t.Fatalf("INT-06 not in top 3:\n%s", out)
	}
}

func TestShippedThemesPass(t *testing.T) {
	if c, out, _ := do("tokens"); c != 0 {
		t.Fatalf("shipped tokens failed:\n%s", out)
	}
}

func TestContrastExitCode(t *testing.T) {
	if c, _, _ := do("contrast", "#767676", "#fff"); c != 0 {
		t.Fatal("4.54 should pass")
	}
	if c, _, _ := do("contrast", "#777", "#fff"); c != 1 {
		t.Fatal("4.48 should fail")
	}
}

func TestDocRejectsTraversal(t *testing.T) {
	if c, _, _ := do("doc", "../etc/passwd"); c == 0 {
		t.Fatal("traversal must fail")
	}
}
