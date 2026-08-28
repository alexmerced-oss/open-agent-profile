package oap

import (
	"bytes"
	_ "embed"
	"fmt"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/santhosh-tekuri/jsonschema/v5"
)

//go:embed schema/v1/agent-profile.schema.json
var profileSchema []byte

//go:embed schema/v1/agent-state-delta.schema.json
var deltaSchema []byte

var versionRE = regexp.MustCompile(`^(\d+)\.(\d+)$`)
var envRE = regexp.MustCompile(`^\$\{[A-Z][A-Z0-9_]{0,63}\}$`)
var headerRE = regexp.MustCompile(`^(Bearer )?\$\{[A-Z][A-Z0-9_]{0,63}\}$`)
var varRE = regexp.MustCompile(`\$\{\{\s*vars\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}`)
var secretREs = []struct {
	pattern *regexp.Regexp
	label   string
}{
	{regexp.MustCompile(`\bsk-[A-Za-z0-9_-]{16,}`), "OpenAI-style API key"}, {regexp.MustCompile(`\bsk-ant-[A-Za-z0-9_-]{16,}`), "Anthropic API key"}, {regexp.MustCompile(`\bgh[pousr]_[A-Za-z0-9]{20,}`), "GitHub token"}, {regexp.MustCompile(`\bAKIA[0-9A-Z]{16}\b`), "AWS access key id"}, {regexp.MustCompile(`\bxox[baprs]-[A-Za-z0-9-]{10,}`), "Slack token"}, {regexp.MustCompile(`-----BEGIN (?:RSA |EC |OPENSSH |PGP )?PRIVATE KEY-----`), "private key"}, {regexp.MustCompile(`\bey[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}`), "JWT"},
}

func issue(target *[]Issue, pointer, message string) {
	*target = append(*target, Issue{Pointer: pointer, Message: message})
}
func compile(raw []byte, name string) (*jsonschema.Schema, error) {
	compiler := jsonschema.NewCompiler()
	if err := compiler.AddResource(name, bytes.NewReader(raw)); err != nil {
		return nil, err
	}
	return compiler.Compile(name)
}

func Validate(document Document, filename ...string) Report {
	errors := []Issue{}
	warnings := []Issue{}
	kind := text(document["kind"])
	if kind == "" {
		kind = "unknown"
	}
	match := versionRE.FindStringSubmatch(text(document["oap"]))
	if match == nil {
		issue(&errors, "/oap", "missing or malformed spec version string")
		return Report{Kind: kind, Document: document, Errors: errors, Warnings: warnings}
	}
	major, _ := strconv.Atoi(match[1])
	minor, _ := strconv.Atoi(match[2])
	if major != 1 || minor > 0 {
		issue(&errors, "/oap", fmt.Sprintf("unsupported OAP version %s; unsupported versions fail closed", text(document["oap"])))
		return Report{Kind: kind, Document: document, Errors: errors, Warnings: warnings}
	}
	var schemaRaw []byte
	if kind == "AgentProfile" {
		schemaRaw = profileSchema
	} else if kind == "AgentStateDelta" {
		schemaRaw = deltaSchema
	} else {
		issue(&errors, "/kind", fmt.Sprintf("%q is not a known 1.x kind", kind))
		return Report{Kind: kind, Document: document, Errors: errors, Warnings: warnings}
	}
	schema, err := compile(schemaRaw, kind+".json")
	if err != nil {
		issue(&errors, "", err.Error())
	} else if err = schema.Validate(map[string]any(document)); err != nil {
		if validation, ok := err.(*jsonschema.ValidationError); ok {
			schemaErrors(validation, &errors)
		} else {
			issue(&errors, "", err.Error())
		}
	}
	walkStrings(document, "", func(pointer, value string) {
		for _, candidate := range secretREs {
			if candidate.pattern.MatchString(value) {
				issue(&errors, pointer, "looks like a literal "+candidate.label+"; use a ${VARIABLE} reference")
				break
			}
		}
	})
	if kind == "AgentProfile" {
		name := ""
		if len(filename) > 0 {
			name = filename[0]
		}
		checkProfile(document, name, &errors, &warnings)
	} else {
		checkDelta(document, &errors, &warnings)
	}
	report := Report{Kind: kind, Document: document, Errors: errors, Warnings: warnings, OK: len(errors) == 0}
	if kind == "AgentProfile" {
		if digests, e := ProfileDigests(document); e == nil {
			report.Digests = &digests
		}
	}
	return report
}

func ValidatePath(path string) Report {
	document, err := Load(path)
	if err != nil {
		return Report{Kind: "unknown", Errors: []Issue{{Message: err.Error()}}, OK: false}
	}
	return Validate(document, path)
}
func schemaErrors(validation *jsonschema.ValidationError, target *[]Issue) {
	if len(validation.Causes) > 0 {
		for _, cause := range validation.Causes {
			schemaErrors(cause, target)
		}
		return
	}
	issue(target, validation.InstanceLocation, validation.Message)
}
func walkStrings(value any, pointer string, visit func(string, string)) {
	switch v := value.(type) {
	case string:
		visit(pointer, v)
	case []any:
		for i, child := range v {
			walkStrings(child, fmt.Sprintf("%s/%d", pointer, i), visit)
		}
	case map[string]any:
		for key, child := range v {
			escaped := strings.ReplaceAll(strings.ReplaceAll(key, "~", "~0"), "/", "~1")
			walkStrings(child, pointer+"/"+escaped, visit)
		}
	}
}

func EscapesWorkspace(path string) bool {
	if strings.HasPrefix(path, "/") || strings.HasPrefix(path, "\\\\") || (len(path) > 2 && ((path[0] >= 'A' && path[0] <= 'Z') || (path[0] >= 'a' && path[0] <= 'z')) && path[1] == ':' && (path[2] == '/' || path[2] == '\\')) {
		return true
	}
	depth := 0
	for _, part := range regexp.MustCompile(`[\\/]+`).Split(path, -1) {
		if part == "" || part == "." {
			continue
		}
		if part == ".." {
			depth--
			if depth < 0 {
				return true
			}
		} else {
			depth++
		}
	}
	return false
}

func checkProfile(document Document, filename string, errors, warnings *[]Issue) {
	spec := obj(document["spec"])
	tools := obj(spec["tools"])
	for i, raw := range arr(tools["mcp_servers"]) {
		server := obj(raw)
		for key, value := range obj(server["env"]) {
			if s, ok := value.(string); !ok || !envRE.MatchString(s) {
				issue(errors, fmt.Sprintf("/spec/tools/mcp_servers/%d/env/%s", i, key), "must be a same-name ${VARIABLE} reference, not a literal")
			}
		}
		for key, value := range obj(server["headers"]) {
			if s, ok := value.(string); !ok || !headerRE.MatchString(s) {
				issue(errors, fmt.Sprintf("/spec/tools/mcp_servers/%d/headers/%s", i, key), "must be '${VARIABLE}' or 'Bearer ${VARIABLE}'")
			}
		}
	}
	context := obj(spec["context"])
	paths := map[string]string{}
	for i, raw := range arr(context["files"]) {
		if path := text(obj(raw)["path"]); path != "" {
			paths[fmt.Sprintf("/spec/context/files/%d/path", i)] = path
		}
	}
	if path := text(context["working_directory"]); path != "" {
		paths["/spec/context/working_directory"] = path
	}
	filesystem := obj(obj(spec["permissions"])["filesystem"])
	for _, key := range []string{"read_roots", "write_roots", "deny_paths"} {
		for i, path := range strs(filesystem[key]) {
			paths[fmt.Sprintf("/spec/permissions/filesystem/%s/%d", key, i)] = path
		}
	}
	for pointer, path := range paths {
		if EscapesWorkspace(path) {
			issue(errors, pointer, fmt.Sprintf("%q resolves outside the workspace", path))
		}
	}
	variables := obj(context["variables"])
	role := obj(spec["role"])
	checkVars := func(pointer, value string) {
		for _, match := range varRE.FindAllStringSubmatch(value, -1) {
			if _, ok := variables[match[1]]; !ok {
				issue(errors, pointer, fmt.Sprintf("references undefined variable %q", match[1]))
			}
		}
	}
	checkVars("/spec/role/instructions", text(role["instructions"]))
	for _, key := range []string{"objectives", "constraints"} {
		for i, value := range strs(role[key]) {
			checkVars(fmt.Sprintf("/spec/role/%s/%d", key, i), value)
		}
	}
	walkStrings(document["state"], "/state", func(pointer, value string) {
		if varRE.MatchString(value) {
			issue(warnings, pointer, "contains a ${{ vars.* }} template; substitution never runs inside state")
		}
	})
	state := obj(document["state"])
	for _, key := range []string{"facts", "preferences", "open_threads"} {
		seen := map[string]bool{}
		for i, raw := range arr(state[key]) {
			id := text(obj(raw)["id"])
			if seen[id] {
				issue(errors, fmt.Sprintf("/state/%s/%d/id", key, i), fmt.Sprintf("duplicate id %q", id))
			}
			seen[id] = true
		}
	}
	metadata := obj(document["metadata"])
	if _, ok := metadata["trust"]; ok {
		issue(warnings, "/metadata/trust", "trust in the file must be discarded and recomputed from the discovery root")
	}
	if filename != "" {
		base := filepath.Base(filename)
		for _, suffix := range []string{".agent.yaml", ".agent.yml", ".agent.json", ".agent.md"} {
			base = strings.TrimSuffix(base, suffix)
		}
		if name := text(metadata["name"]); base != "" && name != base {
			issue(warnings, "/metadata/name", fmt.Sprintf("%q does not match file name %q; metadata.name wins", name, base))
		}
	}
	revisions := []int{}
	for _, raw := range arr(document["history"]) {
		revisions = append(revisions, intValue(obj(raw)["revision"]))
	}
	for i := 1; i < len(revisions); i++ {
		if revisions[i] < revisions[i-1] {
			issue(errors, "/history", "entries must be ordered oldest first by revision")
		}
	}
	if len(revisions) > 0 && revisions[len(revisions)-1] > intValue(metadata["revision"]) {
		issue(errors, "/history", "newest history revision exceeds metadata.revision")
	}
	policy := text(tools["policy"])
	if policy == "inherit" && (tools["allow"] != nil || tools["deny"] != nil) {
		issue(warnings, "/spec/tools", "policy is 'inherit', so allow and deny are ignored")
	}
	if policy == "allowlist" && tools["allow"] == nil && len(tools) > 0 {
		issue(warnings, "/spec/tools", "allowlist has an empty allow list, so the agent gets no tools")
	}
}

func checkDelta(document Document, errors, warnings *[]Issue) {
	for i, raw := range arr(document["operations"]) {
		path := text(obj(raw)["path"])
		if path != "/state" && !strings.HasPrefix(path, "/state/") {
			issue(errors, fmt.Sprintf("/operations/%d/path", i), "operation is outside /state; contract changes belong in proposals")
		}
	}
	for i, raw := range arr(document["proposals"]) {
		proposal := obj(raw)
		path := text(proposal["path"])
		high := false
		for _, prefix := range []string{"/spec/tools", "/spec/permissions", "/spec/memory", "/spec/runtime/subagents"} {
			high = high || strings.HasPrefix(path, prefix)
		}
		if high && text(proposal["risk"]) != "high" {
			issue(warnings, fmt.Sprintf("/proposals/%d", i), path+" must be treated as high risk regardless of its declared risk")
		}
	}
}
func intValue(value any) int {
	switch v := value.(type) {
	case int:
		return v
	case int64:
		return int(v)
	case float64:
		return int(v)
	case jsonNumber:
		n, _ := strconv.Atoi(string(v))
		return n
	case fmt.Stringer:
		n, _ := strconv.Atoi(v.String())
		return n
	}
	return 0
}

type jsonNumber string
