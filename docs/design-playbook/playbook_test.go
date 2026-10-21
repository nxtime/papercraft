package playbook

import (
	"io/fs"
	"regexp"
	"strings"
	"testing"

	"playbook/internal/search"
)

// Every rule id cited anywhere must exist, so cross-references cannot rot.
func TestCrossReferencesResolve(t *testing.T) {
	secs, err := search.Sections(Corpus)
	if err != nil {
		t.Fatal(err)
	}
	known := map[string]bool{}
	for _, s := range secs {
		if s.ID != "" {
			if known[s.ID] {
				t.Errorf("duplicate rule id %s", s.ID)
			}
			known[s.ID] = true
		}
	}
	if len(known) < 80 {
		t.Fatalf("only %d rules found", len(known))
	}
	ref := regexp.MustCompile(`\b(INT|PSY|VIS|MOT|A11Y|FBK|SYS|BAD|CSS|PAT)-(\d+)\b`)
	fs.WalkDir(Corpus, ".", func(p string, e fs.DirEntry, err error) error {
		if err != nil || e.IsDir() || !strings.HasSuffix(p, ".md") && !strings.HasSuffix(p, ".json") && !strings.HasSuffix(p, ".txt") {
			return nil
		}
		data, _ := fs.ReadFile(Corpus, p)
		for _, id := range ref.FindAllString(string(data), -1) {
			if !known[id] {
				t.Errorf("%s cites unknown rule %s", p, id)
			}
		}
		return nil
	})
}
