package oap

import "fmt"

type ProfileReference struct {
	Name     string
	URI      string
	Revision int
	Digest   string
}
type ProfileLoader func(ProfileReference) (AgentProfile, error)
type CompositionError struct{ Message string }

func (e *CompositionError) Error() string { return e.Message }
func MergeProfileValues(base, child Document) Document {
	result := Document(cloneMap(base))
	for key, value := range child {
		if value == nil {
			delete(result, key)
		} else if childMap, ok := value.(map[string]any); ok {
			if baseMap, ok := result[key].(map[string]any); ok {
				result[key] = MergeProfileValues(baseMap, childMap)
			} else {
				result[key] = cloneMap(childMap)
			}
		} else {
			result[key] = cloneValue(value)
		}
	}
	return result
}
func cloneValue(value any) any { return cloneMap(map[string]any{"value": value})["value"] }
func ResolveComposition(profile AgentProfile, load ProfileLoader) (AgentProfile, error) {
	return resolveComposition(profile, load, nil)
}
func resolveComposition(profile AgentProfile, load ProfileLoader, active []string) (AgentProfile, error) {
	name := text(obj(profile["metadata"])["name"])
	for _, item := range active {
		if item == name {
			return nil, &CompositionError{"inheritance cycle"}
		}
	}
	if len(active) >= 8 {
		return nil, &CompositionError{"inheritance depth exceeds 8"}
	}
	merged := Document{}
	for _, raw := range arr(profile["extends"]) {
		referenceDoc := obj(raw)
		reference := ProfileReference{Name: text(referenceDoc["name"]), URI: text(referenceDoc["uri"]), Revision: intValue(referenceDoc["revision"]), Digest: text(referenceDoc["digest"])}
		base, err := load(reference)
		if err != nil {
			return nil, err
		}
		if reference.Revision != 0 && intValue(obj(base["metadata"])["revision"]) != reference.Revision {
			return nil, &CompositionError{reference.Name + " revision does not match pin"}
		}
		if reference.Digest != "" {
			digest, err := SpecDigest(base)
			if err != nil || digest != reference.Digest {
				return nil, &CompositionError{reference.Name + " digest does not match pin"}
			}
		}
		resolved, err := resolveComposition(base, load, append(active, name))
		if err != nil {
			return nil, err
		}
		copy := Document(cloneMap(resolved))
		delete(copy, "extends")
		delete(copy, "state")
		delete(copy, "history")
		metadata := obj(copy["metadata"])
		delete(metadata, "name")
		delete(metadata, "id")
		delete(metadata, "revision")
		merged = MergeProfileValues(merged, copy)
	}
	merged = MergeProfileValues(merged, profile)
	merged["metadata"] = cloneValue(profile["metadata"])
	if profile["state"] != nil {
		merged["state"] = cloneValue(profile["state"])
	} else {
		delete(merged, "state")
	}
	if profile["history"] != nil {
		merged["history"] = cloneValue(profile["history"])
	} else {
		delete(merged, "history")
	}
	delete(merged, "extends")
	if text(obj(merged["metadata"])["name"]) == "" {
		return nil, fmt.Errorf("composed profile has no name")
	}
	return merged, nil
}
