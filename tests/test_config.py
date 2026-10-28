import json

import pytest

from ak980.config import Config, Profile, Rule, export_profile, import_profile, peek_language


def test_migrates_version_1(tmp_path):
    path = tmp_path / "config.json"
    path.write_text(json.dumps({
        "lighting": {"mode": 1, "r": 1, "g": 2, "b": 3}, "options": {"sleep_time": 0},
        "keymap_top": {"5": {"kind": "off"}}, "macros": [{"name": "m", "steps": [["down", 4, 10]]}],
        "clock_slot": 2, "last_image": "/x.gif",
    }))
    c = Config.load(str(path))
    p = c.profile
    assert len(c.profiles) == 1 and c.active == p.name
    assert p.lighting.rgb == (1, 2, 3) and p.options.sleep_time == 0
    assert p.keymap_top[5].kind == "off" and p.macros[0].steps[0].code == 4
    assert c.settings.clock_slot == 2 and c.settings.last_image == "/x.gif"
    c.save()
    again = Config.load(str(path))
    assert again.to_dict() == c.to_dict()


def test_profile_operations(tmp_path):
    c = Config(path=str(tmp_path / "c.json"))
    first = c.profile.name
    game = c.add(Profile(name="Game"))
    c.settings.rules.append(Rule("cs2", "Game"))
    c.settings.default_profile = "Game"
    assert c.add(Profile(name="Game")).name == "Game 2"
    c.rename("Game", "CS2")
    assert c.settings.rules[0].profile == "CS2" and c.settings.default_profile == "CS2" and game.name == "CS2"
    with pytest.raises(ValueError):
        c.rename(first, "CS2")
    c.remove("CS2")
    assert not c.settings.rules and c.settings.default_profile == ""
    c.remove("Game 2")
    with pytest.raises(ValueError):
        c.remove(first)


def test_export_import(tmp_path):
    p = Profile(name="Mine", light_source="host", host_effect="fire", host_values={"fire": {"height": 80}})
    export_profile(p, str(tmp_path / "p.json"))
    q = import_profile(str(tmp_path / "p.json"))
    assert q.to_dict() == p.to_dict()
    (tmp_path / "bad.json").write_text("{}")
    with pytest.raises(ValueError):
        import_profile(str(tmp_path / "bad.json"))


def test_peek_language(tmp_path):
    path = tmp_path / "c.json"
    assert peek_language(str(path)) == "auto"
    path.write_text(json.dumps({"settings": {"language": "uk"}}))
    assert peek_language(str(path)) == "uk"
