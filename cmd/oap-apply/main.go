package main

import (
	"flag"
	"fmt"
	oap "github.com/alexmerced-oss/open-agent-profile"
	"os"
	"path/filepath"
)

func main() {
	approved := flag.Bool("approve", false, "record human approval")
	dry := flag.Bool("dry-run", false, "print without writing")
	actor := flag.String("actor", "oap-go", "approval actor")
	flag.Parse()
	if flag.NArg() != 2 {
		fmt.Fprintln(os.Stderr, "usage: oap-apply [--approve] [--dry-run] [--actor name] <profile> <delta>")
		os.Exit(2)
	}
	profilePath := flag.Arg(0)
	profile, err := oap.Load(profilePath)
	if err != nil {
		fail(err)
	}
	delta, err := oap.Load(flag.Arg(1))
	if err != nil {
		fail(err)
	}
	for _, input := range []struct {
		path     string
		document oap.Document
	}{{profilePath, profile}, {flag.Arg(1), delta}} {
		report := oap.Validate(input.document, input.path)
		if !report.OK {
			for _, item := range report.Errors {
				fmt.Fprintf(os.Stderr, "%s: %s: %s\n", input.path, item.Pointer, item.Message)
			}
			os.Exit(1)
		}
	}
	result, err := oap.ApplyDelta(profile, delta, oap.ApplyOptions{Approved: *approved, Actor: *actor})
	if err != nil {
		fail(err)
	}
	format := "yaml"
	if filepath.Ext(profilePath) == ".json" {
		format = "json"
	} else if filepath.Ext(profilePath) == ".md" {
		format = "markdown"
	}
	encoded, err := oap.Serialize(result.Profile, format)
	if err != nil {
		fail(err)
	}
	for _, warning := range result.Warnings {
		fmt.Fprintln(os.Stderr, "warn:", warning)
	}
	if len(result.PendingProposals) > 0 {
		fmt.Fprintf(os.Stderr, "%d proposal(s) require human review and were not applied\n", len(result.PendingProposals))
	}
	if *dry {
		fmt.Print(encoded)
	} else if err = oap.WriteAtomically(profilePath, []byte(encoded)); err != nil {
		fail(err)
	}
	fmt.Fprintf(os.Stderr, "%s revision %d\n", map[bool]string{true: "would write", false: "wrote"}[*dry], intValue(result.Profile))
}
func intValue(profile oap.Document) int {
	metadata, _ := profile["metadata"].(map[string]any)
	switch value := metadata["revision"].(type) {
	case int:
		return value
	case int64:
		return int(value)
	case float64:
		return int(value)
	}
	return 0
}
func fail(err error) { fmt.Fprintln(os.Stderr, "error:", err); os.Exit(1) }
