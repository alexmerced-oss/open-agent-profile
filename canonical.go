package oap

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"

	"github.com/gowebpki/jcs"
)

func CanonicalJSON(value any) ([]byte, error) {
	raw, err := json.Marshal(value)
	if err != nil {
		return nil, err
	}
	return jcs.Transform(raw)
}
func digest(value any) (string, error) {
	canonical, err := CanonicalJSON(value)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(canonical)
	return "sha256:" + hex.EncodeToString(sum[:]), nil
}
func ProfileDigest(profile AgentProfile) (string, error) { return digest(profile) }
func SpecDigest(profile AgentProfile) (string, error) {
	metadata := cloneMap(obj(profile["metadata"]))
	delete(metadata, "revision")
	delete(metadata, "updated_at")
	delete(metadata, "trust")
	return digest(Document{"metadata": metadata, "spec": profile["spec"]})
}
func ProfileDigests(profile AgentProfile) (Digests, error) {
	p, err := ProfileDigest(profile)
	if err != nil {
		return Digests{}, err
	}
	s, err := SpecDigest(profile)
	return Digests{Profile: p, Spec: s}, err
}
