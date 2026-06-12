import os

import pytest

from picoconf import PC


@pytest.fixture
def set_envars(request):
    if isinstance(request.param, list):
        for pair in request.param:
            os.environ[pair[0]] = pair[1]
        yield
        for pair in request.param:
            del os.environ[pair[0]]
    else:
        os.environ[request.param[0]] = request.param[1]
        yield
        del os.environ[request.param[0]]


@pytest.mark.parametrize("set_envars", [("simple_overriden", "True")], indirect=True)
def test_simple(set_envars):
    pconf = PC("tests/data/simple.pconf")
    assert pconf._name == "simple"
    assert pconf._envar_prefix == "simple"
    assert isinstance(pconf.things, list)
    assert len(pconf.things) == 3
    assert pconf.overriden == "True"


@pytest.mark.parametrize(
    "set_envars", [("simple_overriden", '{"a": 1, "b": 2}')], indirect=True
)
def test_json_envar(set_envars):
    pconf = PC("tests/data/simple.pconf")
    assert isinstance(pconf.overriden, PC)
    assert pconf.overriden.a == 1
    assert pconf.overriden.b == 2


def test_nested():
    pconf = PC("tests/data/nested.pconf")
    assert pconf._name == "nested"
    assert pconf.top.v1 == 1
    assert pconf.top.middle.v2 == 2
    assert pconf.top.middle.lowest.v3 == 3
    assert pconf.animals.panda.diet == "Herbivore"
    assert all([pconf.animals.dog, pconf.animals.lion, pconf.animals.penguin])


@pytest.mark.parametrize("set_envars", [("testdog_diet", "Treats!")], indirect=True)
def test_nested_envar(set_envars):
    pconf = PC("tests/data/nested.pconf")
    assert pconf.animals.dog.diet == "Treats!"


def test_dotted_attribute_access():
    """Test that top-level config items support both dict and attribute access."""
    pconf = PC("tests/data/simple.pconf")

    # Dictionary access
    assert pconf["key"] == "value"
    assert pconf["test"] == 1
    assert pconf["overriden"] is False

    # Attribute access (dotted notation)
    assert pconf.key == "value"
    assert pconf.test == 1
    assert pconf.overriden is False

    # Both should return the same values
    assert pconf["key"] == pconf.key
    assert pconf["test"] == pconf.test
    assert pconf["overriden"] == pconf.overriden


def test_nested_dotted_attribute_access():
    """Test that nested config items support both dict and attribute access."""
    pconf = PC("tests/data/nested.pconf")

    # Top-level access (both methods)
    assert pconf["test"] == pconf.test == "value"

    # Nested access
    assert pconf["top"]["v1"] == pconf.top.v1 == 1
    assert pconf["top"]["middle"]["v2"] == pconf.top.middle.v2 == 2
    assert pconf["top"]["middle"]["lowest"]["v3"] == pconf.top.middle.lowest.v3 == 3


def test_imported_config_dotted_access():
    """Test that imported configs support both dict and attribute access."""
    pconf = PC("tests/data/nested.pconf")

    # Imported config access
    assert pconf["animals"]["dog"]["name"] == pconf.animals.dog.name == "Dog"
    assert pconf["animals"]["dog"]["diet"] == pconf.animals.dog.diet == "Omnivore"
    assert pconf["animals"]["panda"]["diet"] == pconf.animals.panda.diet == "Herbivore"

    # Mixed access patterns
    assert pconf.animals["dog"].name == "Dog"
    assert pconf["animals"].dog.name == "Dog"


def test_to_dict_conversion():
    """Test that to_dict() recursively converts PicoConf to plain dicts."""
    pconf = PC("tests/data/nested.pconf")

    # Convert to plain dict
    plain_dict = pconf.to_dict()

    # Verify it's a plain dict, not PicoConf
    assert isinstance(plain_dict, dict)
    assert not isinstance(plain_dict, PC)

    # Verify nested structures are also plain dicts
    assert isinstance(plain_dict["top"], dict)
    assert not isinstance(plain_dict["top"], PC)
    assert isinstance(plain_dict["top"]["middle"], dict)
    assert not isinstance(plain_dict["top"]["middle"], PC)

    # Verify values are preserved
    assert plain_dict["test"] == "value"
    assert plain_dict["top"]["v1"] == 1
    assert plain_dict["top"]["middle"]["v2"] == 2

    # Verify animals sub-config
    animals_dict = pconf.animals.to_dict()
    assert isinstance(animals_dict["dog"], dict)
    assert not isinstance(animals_dict["dog"], PC)
    assert animals_dict["dog"]["name"] == "Dog"
