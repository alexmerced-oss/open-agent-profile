from pathlib import Path
import tomllib

import yaml


ROOT = Path(__file__).resolve().parents[1]


def test_profile_authoring_skill_is_packaged_and_review_first() -> None:
    path = ROOT / "skills" / "oap-profile-authoring" / "SKILL.md"
    text = path.read_text(encoding="utf-8")
    closing = text.find("\n---\n", 4)
    metadata = yaml.safe_load(text[4:closing])

    assert metadata["name"] == "oap-profile-authoring"
    assert "proposal" in text.casefold()
    assert "~/.agentprofiles" in text
    assert "never invent" in text.casefold()

    project = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    wheel = project["tool"]["hatch"]["build"]["targets"]["wheel"]
    assert wheel["force-include"]["skills"] == "oap/skills"


def test_spec_defines_universal_root_and_native_precedence() -> None:
    text = (ROOT / "spec" / "v1" / "SPEC.md").read_text(encoding="utf-8")
    assert "`~/.agentprofiles/` is the RECOMMENDED universal user location" in text
    assert "harness-native user directory SHOULD take" in text
