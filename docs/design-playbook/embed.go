// Package playbook embeds the design playbook corpus so the CLI works without a
// checkout. Set PLAYBOOK_DIR to read a live checkout instead.
package playbook

import "embed"

//go:embed README.md AGENTS.md llms.txt principles platforms checklists templates sources skills
var Corpus embed.FS
