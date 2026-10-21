// Package search ranks playbook sections. A section is a Markdown heading plus
// its body; rule ids come from headings like "### A11Y-10 Title". Ranking is BM25
// over section text with a boost for title/id hits, enough for a corpus this size.
package search

import (
	"bufio"
	"io/fs"
	"math"
	"path"
	"regexp"
	"sort"
	"strings"
)

var (
	dirs      = []string{"principles", "platforms", "checklists", "templates", "sources", "skills"}
	top       = []string{"README.md", "AGENTS.md"}
	headingRE = regexp.MustCompile(`^(#{1,6})\s+(.*\S)\s*$`)
	ruleIDRE  = regexp.MustCompile(`^([A-Z0-9]+-\d+)\b`)
	tokenRE   = regexp.MustCompile(`[a-z0-9][a-z0-9\-_.]*[a-z0-9]|[a-z0-9]`)
)

type Section struct {
	Path  string
	Line  int
	Title string
	ID    string
	Text  string
}

// Stem folds plurals and gerunds so "transitions" matches "transition".
func Stem(t string) string {
	if len(t) > 5 && strings.HasSuffix(t, "ing") {
		return t[:len(t)-3]
	}
	if len(t) > 3 && strings.HasSuffix(t, "s") && !strings.HasSuffix(t, "ss") {
		return t[:len(t)-1]
	}
	return t
}

func Tokenize(text string) []string {
	raw := tokenRE.FindAllString(strings.ToLower(text), -1)
	for i, t := range raw {
		raw[i] = Stem(t)
	}
	return raw
}

func markdownFiles(fsys fs.FS) []string {
	var out []string
	for _, name := range top {
		if _, err := fs.Stat(fsys, name); err == nil {
			out = append(out, name)
		}
	}
	for _, d := range dirs {
		fs.WalkDir(fsys, d, func(p string, e fs.DirEntry, err error) error {
			if err == nil && !e.IsDir() && strings.HasSuffix(p, ".md") {
				out = append(out, p)
			}
			return nil
		})
	}
	return out
}

// Docs lists every Markdown document the search covers.
func Docs(fsys fs.FS) []string { return markdownFiles(fsys) }

// Sections splits every document into heading-delimited sections.
func Sections(fsys fs.FS) ([]Section, error) {
	var out []Section
	for _, p := range markdownFiles(fsys) {
		f, err := fsys.Open(p)
		if err != nil {
			return nil, err
		}
		sc := bufio.NewScanner(f)
		sc.Buffer(make([]byte, 1<<20), 1<<20)
		inFence := false
		var cur *Section
		var body []string
		flush := func() {
			if cur != nil {
				cur.Text = strings.TrimSpace(strings.Join(body, "\n"))
				out = append(out, *cur)
			}
		}
		for line := 1; sc.Scan(); line++ {
			text := sc.Text()
			if strings.HasPrefix(strings.TrimLeft(text, " \t"), "```") {
				inFence = !inFence
			}
			var m []string
			if !inFence {
				m = headingRE.FindStringSubmatch(text)
			}
			if m != nil {
				flush()
				title := m[2]
				id := ""
				if im := ruleIDRE.FindStringSubmatch(title); im != nil {
					id = im[1]
				}
				cur = &Section{Path: path.Clean(p), Line: line, Title: title, ID: id}
				body = body[:0]
			} else if cur != nil {
				body = append(body, text)
			}
		}
		f.Close()
		flush()
	}
	return out, nil
}

type Hit struct {
	Score float64
	Section
}

// Rank returns the k best sections for a query.
func Rank(query string, secs []Section, k int) []Hit {
	q := Tokenize(query)
	if len(q) == 0 {
		return nil
	}
	qset := map[string]bool{}
	for _, t := range q {
		qset[t] = true
	}
	docs := make([][]string, len(secs))
	df := map[string]int{}
	total := 0
	for i, s := range secs {
		docs[i] = Tokenize(s.Title + " " + s.ID + " " + s.Text)
		total += len(docs[i])
		seen := map[string]bool{}
		for _, t := range docs[i] {
			if !seen[t] {
				seen[t] = true
				df[t]++
			}
		}
	}
	n := float64(len(docs))
	avg := float64(total) / math.Max(n, 1)
	var hits []Hit
	for i, s := range secs {
		tf := map[string]int{}
		for _, t := range docs[i] {
			tf[t]++
		}
		score := 0.0
		for t := range qset {
			f, ok := tf[t]
			if !ok {
				continue
			}
			d := float64(df[t])
			idf := math.Log(1 + (n-d+0.5)/(d+0.5))
			ff := float64(f)
			score += idf * ff * 2.2 / (ff + 1.2*(0.25+0.75*float64(len(docs[i]))/avg))
		}
		head := map[string]bool{}
		for _, t := range Tokenize(s.Title + " " + s.ID) {
			if qset[t] {
				head[t] = true
			}
		}
		score += 1.5 * float64(len(head))
		if score > 0 {
			hits = append(hits, Hit{score, s})
		}
	}
	sort.SliceStable(hits, func(a, b int) bool { return hits[a].Score > hits[b].Score })
	if len(hits) > k {
		hits = hits[:k]
	}
	return hits
}

// ByID returns rules whose id equals want or starts with want+"-" (a prefix).
func ByID(secs []Section, want string) []Section {
	want = strings.ToUpper(want)
	var out []Section
	for _, s := range secs {
		if s.ID != "" && (s.ID == want || strings.HasPrefix(s.ID, want+"-")) {
			out = append(out, s)
		}
	}
	return out
}
