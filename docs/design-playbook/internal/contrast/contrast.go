// Package contrast implements WCAG 2.x contrast ratios and DTCG token-pair checks.
//
// WCAG 2 ratios are compared unrounded: 4.499 fails 4.5. APCA is not implemented.
package contrast

import (
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"math"
	"regexp"
	"strings"
)

var hexRE = regexp.MustCompile(`^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$`)

// RGBA holds 0-255 channels and a 0-1 alpha.
type RGBA struct{ R, G, B, A float64 }

func ParseHex(value string) (RGBA, error) {
	m := hexRE.FindStringSubmatch(strings.TrimSpace(value))
	if m == nil {
		return RGBA{}, fmt.Errorf("not a hex color: %q", value)
	}
	h := m[1]
	if len(h) == 3 {
		var b strings.Builder
		for _, c := range h {
			b.WriteRune(c)
			b.WriteRune(c)
		}
		h = b.String()
	}
	var v [4]int
	for i := 0; i < len(h)/2; i++ {
		fmt.Sscanf(h[i*2:i*2+2], "%x", &v[i])
	}
	a := 1.0
	if len(h) == 8 {
		a = float64(v[3]) / 255
	}
	return RGBA{float64(v[0]), float64(v[1]), float64(v[2]), a}, nil
}

func lin(c float64) float64 {
	c /= 255
	if c <= 0.03928 {
		return c / 12.92
	}
	return math.Pow((c+0.055)/1.055, 2.4)
}

func luminance(c RGBA) float64 {
	return 0.2126*lin(c.R) + 0.7152*lin(c.G) + 0.0722*lin(c.B)
}

// composite flattens a translucent foreground over an opaque background.
func composite(fg, bg RGBA) RGBA {
	mix := func(f, b float64) float64 { return math.RoundToEven(f*fg.A + b*(1-fg.A)) }
	return RGBA{mix(fg.R, bg.R), mix(fg.G, bg.G), mix(fg.B, bg.B), 1}
}

// Ratio returns the WCAG contrast ratio of fg over bg (hex strings).
func Ratio(fgHex, bgHex string) (float64, error) {
	bg, err := ParseHex(bgHex)
	if err != nil {
		return 0, err
	}
	if bg.A < 1 {
		return 0, fmt.Errorf("background must be opaque")
	}
	fg, err := ParseHex(fgHex)
	if err != nil {
		return 0, err
	}
	l1, l2 := luminance(composite(fg, bg)), luminance(bg)
	return (math.Max(l1, l2) + 0.05) / (math.Min(l1, l2) + 0.05), nil
}

// Verdict is one WCAG threshold and whether a ratio meets it.
type Verdict struct {
	Name string
	Pass bool
}

func Verdicts(r float64) []Verdict {
	return []Verdict{
		{"AA normal (4.5)", r >= 4.5},
		{"AA large / UI (3)", r >= 3},
		{"AAA normal (7)", r >= 7},
		{"AAA large (4.5)", r >= 4.5},
	}
}

func flatten(node any, path []string, out map[string]map[string]any) {
	obj, ok := node.(map[string]any)
	if !ok {
		return
	}
	if _, has := obj["$value"]; has {
		out[strings.Join(path, ".")] = obj
		return
	}
	for k, v := range obj {
		if !strings.HasPrefix(k, "$") {
			flatten(v, append(append([]string{}, path...), k), out)
		}
	}
}

func resolveHex(tokens map[string]map[string]any, name string, seen map[string]bool) (string, error) {
	if seen[name] {
		return "", fmt.Errorf("alias cycle at %s", name)
	}
	tok, ok := tokens[name]
	if !ok {
		return "", fmt.Errorf("unknown token %s", name)
	}
	seen[name] = true
	switch v := tok["$value"].(type) {
	case map[string]any:
		hex, _ := v["hex"].(string)
		if hex == "" {
			return "", fmt.Errorf("%s: color object has no hex", name)
		}
		return hex, nil
	case string:
		if strings.HasPrefix(v, "{") && strings.HasSuffix(v, "}") {
			return resolveHex(tokens, v[1:len(v)-1], seen)
		}
		return v, nil
	}
	return "", fmt.Errorf("%s: unsupported $value", name)
}

// CheckTokens verifies every contrastPairs entry declared in a DTCG token file
// and prints one line per pair. It returns the pair count and failure count.
func CheckTokens(fsys fs.FS, path string, w io.Writer) (total, failed int, err error) {
	data, err := fs.ReadFile(fsys, path)
	if err != nil {
		return 0, 0, err
	}
	var doc map[string]any
	if err := json.Unmarshal(data, &doc); err != nil {
		return 0, 0, fmt.Errorf("%s: %w", path, err)
	}
	tokens := map[string]map[string]any{}
	flatten(doc, nil, tokens)
	ext, _ := doc["$extensions"].(map[string]any)
	pb, _ := ext["dev.playbook"].(map[string]any)
	pairs, _ := pb["contrastPairs"].([]any)
	if len(pairs) == 0 {
		fmt.Fprintf(w, "%s: no contrastPairs declared\n", path)
		return 0, 0, nil
	}
	for _, p := range pairs {
		pair, _ := p.([]any)
		if len(pair) < 3 {
			fmt.Fprintf(w, "ERROR %s: malformed pair %v\n", path, p)
			failed++
			continue
		}
		fg, _ := pair[0].(string)
		bg, _ := pair[1].(string)
		need, _ := pair[2].(float64)
		note := ""
		if len(pair) > 3 {
			note, _ = pair[3].(string)
		}
		fgHex, e1 := resolveHex(tokens, fg, map[string]bool{})
		bgHex, e2 := resolveHex(tokens, bg, map[string]bool{})
		var r float64
		e := firstErr(e1, e2)
		if e == nil {
			r, e = Ratio(fgHex, bgHex)
		}
		if e != nil {
			fmt.Fprintf(w, "ERROR %s: %s on %s: %v\n", path, fg, bg, e)
			failed++
			continue
		}
		ok := r >= need
		if !ok {
			failed++
		}
		status := "PASS"
		if !ok {
			status = "FAIL"
		}
		fmt.Fprintf(w, "%s %6.2f (need %g)  %s on %s  %s\n", status, r, need, fg, bg, note)
	}
	return len(pairs), failed, nil
}

func firstErr(errs ...error) error {
	for _, e := range errs {
		if e != nil {
			return e
		}
	}
	return nil
}
