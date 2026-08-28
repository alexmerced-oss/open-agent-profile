package oap

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"gopkg.in/yaml.v3"
)

type ApplyError struct{ Message string }

func (e *ApplyError) Error() string { return e.Message }

type ConflictError struct{ Message string }

func (e *ConflictError) Error() string { return e.Message }

type ApplyOptions struct {
	Approved bool
	Actor    string
	Now      func() time.Time
}

func pointerTokens(pointer string) []string {
	if pointer == "" {
		return nil
	}
	parts := strings.Split(strings.TrimPrefix(pointer, "/"), "/")
	for i, part := range parts {
		parts[i] = strings.ReplaceAll(strings.ReplaceAll(part, "~1", "/"), "~0", "~")
	}
	return parts
}
func applyOperation(document Document, operation map[string]any, warnings *[]string) error {
	kind := text(operation["op"])
	pointer := text(operation["path"])
	if pointer != "/state" && !strings.HasPrefix(pointer, "/state/") {
		return &ApplyError{fmt.Sprintf("operation path %q is outside /state", pointer)}
	}
	if kind != "add" && kind != "replace" && kind != "remove" {
		return &ApplyError{fmt.Sprintf("unknown operation %q", kind)}
	}
	updated, missing, err := modify(map[string]any(document), pointerTokens(pointer), kind, operation["value"])
	if err != nil {
		return err
	}
	if missing {
		if kind == "remove" {
			*warnings = append(*warnings, fmt.Sprintf("remove on missing path %q ignored", pointer))
			return nil
		}
		return &ApplyError{fmt.Sprintf("path %q does not resolve", pointer)}
	}
	_ = updated // nested containers are mutated in place; arrays are assigned by their parent.
	return nil
}
func modify(current any, tokens []string, kind string, value any) (any, bool, error) {
	if len(tokens) == 0 {
		return cloneValue(value), false, nil
	}
	token := tokens[0]
	last := len(tokens) == 1
	switch container := current.(type) {
	case map[string]any:
		if last {
			_, exists := container[token]
			switch kind {
			case "remove":
				if !exists {
					return container, true, nil
				}
				delete(container, token)
			case "replace", "add":
				container[token] = cloneValue(value)
			}
			return container, false, nil
		}
		child, exists := container[token]
		if !exists {
			child = map[string]any{}
			container[token] = child
		}
		updated, missing, err := modify(child, tokens[1:], kind, value)
		if err == nil && !missing {
			container[token] = updated
		}
		return container, missing, err
	case []any:
		index := -1
		if strings.HasPrefix(token, "id:") {
			id := strings.TrimPrefix(token, "id:")
			for i, item := range container {
				if text(obj(item)["id"]) == id {
					index = i
					break
				}
			}
		} else if token == "-" {
			index = len(container)
		} else {
			parsed, err := strconv.Atoi(token)
			if err != nil {
				return current, false, &ApplyError{"array index expected"}
			}
			index = parsed
		}
		if last {
			if kind == "add" {
				if index < 0 || index > len(container) {
					return current, true, nil
				}
				container = append(container, nil)
				copy(container[index+1:], container[index:])
				container[index] = cloneValue(value)
				return container, false, nil
			}
			if index < 0 || index >= len(container) {
				return current, true, nil
			}
			if kind == "remove" {
				return append(container[:index], container[index+1:]...), false, nil
			}
			container[index] = cloneValue(value)
			return container, false, nil
		}
		if index < 0 || index >= len(container) {
			return current, true, nil
		}
		updated, missing, err := modify(container[index], tokens[1:], kind, value)
		if err == nil && !missing {
			container[index] = updated
		}
		return container, missing, err
	default:
		return current, true, nil
	}
}

func ApplyDelta(profile AgentProfile, delta AgentStateDelta, options ApplyOptions) (DeltaApplication, error) {
	metadata := obj(profile["metadata"])
	current := intValue(metadata["revision"])
	target := obj(delta["target"])
	if text(target["name"]) != text(metadata["name"]) {
		return DeltaApplication{}, &ApplyError{fmt.Sprintf("delta targets %q but profile is %q", text(target["name"]), text(metadata["name"]))}
	}
	if intValue(target["revision"]) != current {
		return DeltaApplication{}, &ConflictError{fmt.Sprintf("delta targets revision %d but profile is at %d", intValue(target["revision"]), current)}
	}
	if pinned := text(target["digest"]); pinned != "" {
		actual, err := ProfileDigest(profile)
		if err != nil || pinned != actual {
			return DeltaApplication{}, &ApplyError{"target.digest does not match profile"}
		}
	}
	writeback := text(obj(obj(profile["spec"])["lifecycle"])["writeback"])
	if writeback == "" {
		writeback = "propose"
	}
	if writeback == "off" {
		return DeltaApplication{}, &ApplyError{"lifecycle.writeback is 'off'"}
	}
	if writeback == "propose" && !options.Approved {
		return DeltaApplication{}, &ApplyError{"lifecycle.writeback is 'propose'; explicit approval is required"}
	}
	working := AgentProfile(cloneMap(profile))
	if working["state"] == nil {
		working["state"] = map[string]any{}
	}
	warnings := []string{}
	for i, raw := range arr(delta["operations"]) {
		if err := applyOperation(working, obj(raw), &warnings); err != nil {
			return DeltaApplication{}, &ApplyError{fmt.Sprintf("operation %d: %v", i, err)}
		}
	}
	stamp := time.Now().UTC()
	if options.Now != nil {
		stamp = options.Now().UTC()
	}
	stampText := stamp.Format(time.RFC3339)
	enforceRetention(working, &warnings, stampText)
	obj(working["metadata"])["revision"] = current + 1
	obj(working["metadata"])["updated_at"] = stampText
	if len(arr(delta["operations"])) > 0 {
		state := obj(working["state"])
		state["updated_at"] = stampText
		state["revision"] = intValue(state["revision"]) + 1
	}
	session := obj(delta["session"])
	actor := options.Actor
	if actor == "" {
		actor = "oap-go"
	}
	by := actor
	if text(session["id"]) != "" {
		by = text(session["id"])
	}
	change := text(delta["summary"])
	if change == "" {
		change = fmt.Sprintf("%d state operations", len(arr(delta["operations"])))
	}
	entry := map[string]any{"revision": current + 1, "at": stampText, "by": by, "change": change, "sections": []any{"state"}}
	if id := text(session["id"]); id != "" {
		entry["session_id"] = id
	}
	if harness := text(session["harness"]); harness != "" {
		entry["harness"] = harness
	}
	if options.Approved {
		entry["approved_by"] = actor
	}
	history := arr(working["history"])
	working["history"] = append(history, entry)
	enforceRetention(working, &warnings, stampText)
	pending := []Document{}
	for _, raw := range arr(delta["proposals"]) {
		proposal := Document(cloneMap(obj(raw)))
		path := text(proposal["path"])
		for _, prefix := range []string{"/spec/tools", "/spec/permissions", "/spec/memory", "/spec/runtime/subagents"} {
			if strings.HasPrefix(path, prefix) {
				proposal["risk"] = "high"
			}
		}
		pending = append(pending, proposal)
	}
	return DeltaApplication{Profile: working, Warnings: warnings, PendingProposals: pending}, nil
}

func sortKey(entry map[string]any, strategy string) any {
	if strategy == "least_confident" {
		if value, ok := number(entry["confidence"]); ok {
			return value
		}
		return float64(1)
	}
	if strategy == "oldest" {
		if value := text(entry["learned_at"]); value != "" {
			return value
		}
		return text(entry["opened_at"])
	}
	for _, key := range []string{"last_used_at", "updated_at", "learned_at"} {
		if value := text(entry[key]); value != "" {
			return value
		}
	}
	return ""
}
func number(value any) (float64, bool) {
	switch v := value.(type) {
	case float64:
		return v, true
	case int64:
		return float64(v), true
	case int:
		return float64(v), true
	case fmt.Stringer:
		n, err := strconv.ParseFloat(v.String(), 64)
		return n, err == nil
	}
	return 0, false
}
func enforceRetention(profile AgentProfile, warnings *[]string, currentTime string) {
	retention := obj(obj(obj(profile["spec"])["lifecycle"])["retention"])
	state := obj(profile["state"])
	strategy := text(retention["eviction"])
	if strategy == "" {
		strategy = "least_recently_used"
	}
	for _, collection := range []string{"facts", "preferences"} {
		entries := arr(state[collection])
		if entries == nil {
			continue
		}
		retained := entries[:0]
		for _, raw := range entries {
			entry := obj(raw)
			expired := retention["fact_ttl_days"] != nil && text(entry["expires_at"]) != "" && text(entry["expires_at"]) < currentTime && entry["pinned"] != true
			if expired {
				*warnings = append(*warnings, fmt.Sprintf("evicted expired %s entry %q", collection, text(entry["id"])))
			} else {
				retained = append(retained, raw)
			}
		}
		entries = retained
		state[collection] = entries
		if collection == "facts" && retention["max_facts"] != nil {
			cap := intValue(retention["max_facts"])
			if len(entries) > cap {
				pinned := []any{}
				rest := []any{}
				for _, raw := range entries {
					if obj(raw)["pinned"] == true {
						pinned = append(pinned, raw)
					} else {
						rest = append(rest, raw)
					}
				}
				sort.SliceStable(rest, func(i, j int) bool {
					a := sortKey(obj(rest[i]), strategy)
					b := sortKey(obj(rest[j]), strategy)
					if af, ok := a.(float64); ok {
						bf, _ := b.(float64)
						return af < bf
					}
					return fmt.Sprint(a) < fmt.Sprint(b)
				})
				room := cap - len(pinned)
				if room < 0 {
					room = 0
				}
				drop := len(rest) - room
				if drop < 0 {
					drop = 0
				}
				dropped := map[string]bool{}
				for _, raw := range rest[:drop] {
					id := text(obj(raw)["id"])
					dropped[id] = true
					*warnings = append(*warnings, fmt.Sprintf("evicted %s entry %q (%s)", collection, id, strategy))
				}
				kept := []any{}
				for _, raw := range entries {
					if !dropped[text(obj(raw)["id"])] {
						kept = append(kept, raw)
					}
				}
				state[collection] = kept
			}
		}
	}
	if retention["max_open_threads"] != nil {
		cap := intValue(retention["max_open_threads"])
		threads := arr(state["open_threads"])
		if len(threads) > cap {
			closed := []any{}
			active := 0
			for _, raw := range threads {
				status := text(obj(raw)["status"])
				if status == "done" || status == "abandoned" {
					closed = append(closed, raw)
				} else {
					active++
				}
			}
			sort.SliceStable(closed, func(i, j int) bool { return text(obj(closed[i])["updated_at"]) < text(obj(closed[j])["updated_at"]) })
			overflow := len(threads) - cap
			if overflow > len(closed) {
				overflow = len(closed)
			}
			dropped := map[string]bool{}
			for _, raw := range closed[:overflow] {
				id := text(obj(raw)["id"])
				dropped[id] = true
				*warnings = append(*warnings, fmt.Sprintf("evicted closed thread %q", id))
			}
			remaining := []any{}
			for _, raw := range threads {
				if !dropped[text(obj(raw)["id"])] {
					remaining = append(remaining, raw)
				}
			}
			if len(remaining) > cap {
				remaining = remaining[len(remaining)-cap:]
				*warnings = append(*warnings, fmt.Sprintf("open_threads still over cap after evicting closed threads (%d active)", active))
			}
			state["open_threads"] = remaining
		}
	}
	historyCap := 50
	if retention["max_history"] != nil {
		historyCap = intValue(retention["max_history"])
	}
	history := arr(profile["history"])
	if len(history) > historyCap {
		profile["history"] = history[len(history)-historyCap:]
	}
}

func Serialize(document Document, format string) (string, error) {
	if format == "json" {
		raw, err := json.MarshalIndent(document, "", "  ")
		return string(raw) + "\n", err
	}
	if format == "markdown" {
		copy := Document(cloneMap(document))
		role := obj(obj(copy["spec"])["role"])
		instructions := strings.TrimSpace(text(role["instructions"]))
		delete(role, "instructions")
		raw, err := yaml.Marshal(copy)
		if err != nil {
			return "", err
		}
		return "---\n" + strings.TrimSpace(string(raw)) + "\n---\n" + instructions + "\n", nil
	}
	raw, err := yaml.Marshal(document)
	return string(raw), err
}
func WriteAtomically(path string, data []byte) error {
	directory := filepath.Dir(path)
	file, err := os.CreateTemp(directory, "."+filepath.Base(path)+".*.tmp")
	if err != nil {
		return err
	}
	temporary := file.Name()
	ok := false
	defer func() {
		if !ok {
			_ = os.Remove(temporary)
		}
	}()
	if err = file.Chmod(0o600); err == nil {
		_, err = file.Write(data)
	}
	if err == nil {
		err = file.Sync()
	}
	closeErr := file.Close()
	if err == nil {
		err = closeErr
	}
	if err == nil {
		err = os.Rename(temporary, path)
	}
	if err != nil {
		return err
	}
	directoryHandle, err := os.Open(directory)
	if err == nil {
		err = directoryHandle.Sync()
		_ = directoryHandle.Close()
	}
	if err == nil {
		ok = true
	}
	return err
}
