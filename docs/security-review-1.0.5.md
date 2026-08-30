# OAP 1.0.5 internal security review

The 1.0.5 change adds discovery guidance and a prompt-to-profile skill; it does not add executable
profile fields or widen OAP authority. Files from `~/.agentprofiles/` retain `user` trust, native
user roots win collisions, and every collision must be reported. Generated profiles are untrusted
model output until they pass schema validation, secret scanning, dependency resolution, effective
authority preview, explicit review, and atomic persistence. Autonomous subagent generation remains
a proposal unless local policy explicitly authorizes activation.

All language validators continue to reject unsupported versions, executable or secret-like content,
workspace escapes, invalid composition, and writeback outside `/state`. Publication remains blocked
on the complete multi-language tests, dependency audits, package inspection, and registry smoke tests.
