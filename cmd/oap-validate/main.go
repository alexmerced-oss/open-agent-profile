package main

import (
	"flag"
	"fmt"
	oap "github.com/alexmerced-oss/open-agent-profile"
	"os"
)

func main() {
	strict := flag.Bool("strict", false, "treat warnings as failures")
	digest := flag.Bool("digest", false, "print digests")
	flag.Parse()
	if flag.NArg() == 0 {
		fmt.Fprintln(os.Stderr, "usage: oap-validate [--strict] [--digest] <profile-or-delta> [...]")
		os.Exit(2)
	}
	failed := false
	for _, path := range flag.Args() {
		report := oap.ValidatePath(path)
		for _, item := range report.Errors {
			fmt.Fprintf(os.Stderr, "error: %s: %s\n", item.Pointer, item.Message)
		}
		for _, item := range report.Warnings {
			fmt.Fprintf(os.Stderr, "warn: %s: %s\n", item.Pointer, item.Message)
		}
		if !report.OK || (*strict && len(report.Warnings) > 0) {
			failed = true
		} else {
			fmt.Printf("%s: valid %s\n", path, report.Kind)
		}
		if *digest && report.Digests != nil {
			fmt.Printf("  profile %s\n  spec    %s\n", report.Digests.Profile, report.Digests.Spec)
		}
	}
	if failed {
		os.Exit(1)
	}
}
