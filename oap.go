// Package oap implements Open Agent Profile 1.0 parsing, validation,
// composition, policy narrowing, prompt rendering, and safe state deltas.
package oap

const (
	OAPVersion     = "1.0"
	SupportVersion = "1.0.5"
)

type Document map[string]any
type AgentProfile = Document
type AgentStateDelta = Document

type Issue struct {
	Pointer string `json:"pointer"`
	Message string `json:"message"`
}
type Digests struct {
	Profile string `json:"profile"`
	Spec    string `json:"spec"`
}
type Report struct {
	Kind     string   `json:"kind"`
	Document Document `json:"document,omitempty"`
	Errors   []Issue  `json:"errors"`
	Warnings []Issue  `json:"warnings"`
	Digests  *Digests `json:"digests,omitempty"`
	OK       bool     `json:"ok"`
}
type Adjustment struct {
	Field     string `json:"field"`
	Requested any    `json:"requested"`
	Effective any    `json:"effective"`
	Reason    string `json:"reason"`
}
type DeltaApplication struct {
	Profile          AgentProfile
	Warnings         []string
	PendingProposals []Document
}

func obj(value any) map[string]any {
	if v, ok := value.(map[string]any); ok {
		return v
	}
	return map[string]any{}
}
func arr(value any) []any {
	if v, ok := value.([]any); ok {
		return v
	}
	return nil
}
func strs(value any) []string {
	out := []string{}
	for _, v := range arr(value) {
		if s, ok := v.(string); ok {
			out = append(out, s)
		}
	}
	return out
}
func text(value any) string { s, _ := value.(string); return s }
