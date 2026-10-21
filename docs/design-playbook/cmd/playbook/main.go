// Command playbook searches and checks the design playbook.
package main

import (
	"flag"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"strings"

	"playbook"
	"playbook/internal/contrast"
	"playbook/internal/search"
)

const usage = `playbook: design rules for accessible, non-generic UI

  playbook search [-n N] QUERY...   ranked sections matching the query
  playbook rule ID                  print a rule by id, or all rules with a prefix (A11Y)
  playbook list                     list every rule id and title
  playbook doc [PATH]               print a document; no PATH lists them
  playbook contrast FG BG           WCAG ratio for two hex colors (exit 1 below 4.5)
  playbook tokens [FILE...]         check contrastPairs in DTCG token files
                                    (no FILE: the shipped light and dark templates)
  playbook skills install [DIR...]  copy the playbook skills into agent skill dirs
                                    (default: ~/.claude/skills and ~/.agents/skills)

Set PLAYBOOK_DIR to read a live checkout instead of the embedded copy.
`

func main() { os.Exit(run(os.Args[1:], os.Stdout, os.Stderr)) }

func corpus() fs.FS {
	if dir := os.Getenv("PLAYBOOK_DIR"); dir != "" {
		return os.DirFS(dir)
	}
	return playbook.Corpus
}

func run(args []string, out, errw io.Writer) int {
	if len(args) == 0 || args[0] == "-h" || args[0] == "--help" || args[0] == "help" {
		fmt.Fprint(out, usage)
		if len(args) == 0 {
			return 2
		}
		return 0
	}
	rest := args[1:]
	switch args[0] {
	case "search":
		return cmdSearch(rest, out, errw)
	case "rule":
		return cmdRule(rest, out, errw)
	case "list":
		return cmdList(out, errw)
	case "doc":
		return cmdDoc(rest, out, errw)
	case "contrast":
		return cmdContrast(rest, out, errw)
	case "tokens":
		return cmdTokens(rest, out, errw)
	case "skills":
		return cmdSkills(rest, out, errw)
	}
	fmt.Fprintf(errw, "unknown command %q\n\n%s", args[0], usage)
	return 2
}

func sections(errw io.Writer) ([]search.Section, bool) {
	secs, err := search.Sections(corpus())
	if err != nil {
		fmt.Fprintln(errw, err)
		return nil, false
	}
	return secs, true
}

func show(out io.Writer, s search.Section, full bool) {
	fmt.Fprintf(out, "%s:%d  %s\n", s.Path, s.Line, s.Title)
	text := []rune(s.Text)
	t := s.Text
	if !full && len(text) > 420 {
		t = string(text[:420])
		if i := strings.LastIndex(t, " "); i >= 0 {
			t = t[:i]
		}
		t += " ..."
	}
	for _, line := range strings.Split(t, "\n") {
		fmt.Fprintln(out, "    "+line)
	}
	fmt.Fprintln(out)
}

func cmdSearch(args []string, out, errw io.Writer) int {
	fl := flag.NewFlagSet("search", flag.ContinueOnError)
	fl.SetOutput(errw)
	n := fl.Int("n", 5, "number of results")
	if fl.Parse(args) != nil {
		return 2
	}
	if fl.NArg() == 0 {
		fmt.Fprint(errw, "usage: playbook search [-n N] QUERY...\n")
		return 2
	}
	secs, ok := sections(errw)
	if !ok {
		return 1
	}
	hits := search.Rank(strings.Join(fl.Args(), " "), secs, *n)
	if len(hits) == 0 {
		fmt.Fprintln(errw, "no match; widen the query, or follow sources/SOURCES.md to research")
		return 1
	}
	for _, h := range hits {
		show(out, h.Section, false)
	}
	return 0
}

func cmdRule(args []string, out, errw io.Writer) int {
	if len(args) != 1 {
		fmt.Fprint(errw, "usage: playbook rule ID\n")
		return 2
	}
	secs, ok := sections(errw)
	if !ok {
		return 1
	}
	hits := search.ByID(secs, args[0])
	if len(hits) == 0 {
		fmt.Fprintf(errw, "no rule %s\n", strings.ToUpper(args[0]))
		return 1
	}
	for _, s := range hits {
		show(out, s, true)
	}
	return 0
}

func cmdList(out, errw io.Writer) int {
	secs, ok := sections(errw)
	if !ok {
		return 1
	}
	for _, s := range secs {
		if s.ID != "" {
			fmt.Fprintf(out, "%-10s %s  (%s:%d)\n", s.ID, strings.TrimSpace(s.Title[len(s.ID):]), s.Path, s.Line)
		}
	}
	return 0
}

func cmdDoc(args []string, out, errw io.Writer) int {
	fsys := corpus()
	if len(args) == 0 {
		for _, d := range search.Docs(fsys) {
			fmt.Fprintln(out, d)
		}
		return 0
	}
	p := path.Clean(args[0])
	if !fs.ValidPath(p) {
		fmt.Fprintf(errw, "invalid path %q\n", args[0])
		return 2
	}
	data, err := fs.ReadFile(fsys, p)
	if err != nil {
		fmt.Fprintf(errw, "no document %s (run `playbook doc` to list)\n", p)
		return 1
	}
	out.Write(data)
	return 0
}

func cmdContrast(args []string, out, errw io.Writer) int {
	if len(args) != 2 {
		fmt.Fprint(errw, "usage: playbook contrast FG BG\n")
		return 2
	}
	r, err := contrast.Ratio(args[0], args[1])
	if err != nil {
		fmt.Fprintln(errw, err)
		return 2
	}
	fmt.Fprintf(out, "%.2f:1\n", r)
	for _, v := range contrast.Verdicts(r) {
		mark := "pass"
		if !v.Pass {
			mark = "FAIL"
		}
		fmt.Fprintf(out, "  %s  %s\n", mark, v.Name)
	}
	if r < 4.5 {
		return 1
	}
	return 0
}

func cmdTokens(args []string, out, errw io.Writer) int {
	type src struct {
		fsys fs.FS
		path string
	}
	var srcs []src
	if len(args) == 0 {
		for _, p := range []string{"templates/tokens.light.json", "templates/tokens.dark.json"} {
			srcs = append(srcs, src{corpus(), p})
		}
	}
	for _, a := range args {
		srcs = append(srcs, src{os.DirFS(filepath.Dir(a)), filepath.Base(a)})
	}
	total, bad := 0, 0
	for _, s := range srcs {
		n, f, err := contrast.CheckTokens(s.fsys, s.path, out)
		if err != nil {
			fmt.Fprintln(errw, err)
			return 2
		}
		total += n
		bad += f
	}
	fmt.Fprintf(out, "%d/%d pairs pass\n", total-bad, total)
	if bad > 0 {
		return 1
	}
	return 0
}

func cmdSkills(args []string, out, errw io.Writer) int {
	if len(args) == 0 || args[0] != "install" {
		fmt.Fprint(errw, "usage: playbook skills install [DIR...]\n")
		return 2
	}
	targets := args[1:]
	if len(targets) == 0 {
		home, err := os.UserHomeDir()
		if err != nil {
			fmt.Fprintln(errw, err)
			return 1
		}
		for _, t := range []string{".claude/skills", ".agents/skills"} {
			d := filepath.Join(home, t)
			if st, err := os.Stat(d); err == nil && st.IsDir() {
				targets = append(targets, d)
			} else {
				fmt.Fprintf(out, "skip %s (missing)\n", d)
			}
		}
	}
	fsys := corpus()
	entries, err := fs.ReadDir(fsys, "skills")
	if err != nil {
		fmt.Fprintln(errw, err)
		return 1
	}
	for _, t := range targets {
		for _, e := range entries {
			if !e.IsDir() {
				continue
			}
			dest := filepath.Join(t, e.Name())
			// Migrate the old symlink install: replace the link, never its target.
			if st, err := os.Lstat(dest); err == nil && st.Mode()&os.ModeSymlink != 0 {
				os.Remove(dest)
			}
			if err := copyTree(fsys, path.Join("skills", e.Name()), dest); err != nil {
				fmt.Fprintln(errw, err)
				return 1
			}
			fmt.Fprintf(out, "installed %s\n", dest)
		}
	}
	return 0
}

func copyTree(fsys fs.FS, src, dest string) error {
	return fs.WalkDir(fsys, src, func(p string, e fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		target := filepath.Join(dest, filepath.FromSlash(strings.TrimPrefix(p, src)))
		if e.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, err := fs.ReadFile(fsys, p)
		if err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
}
