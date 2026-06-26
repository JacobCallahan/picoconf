PicoConf
========
PicoConf is a tiny, opinionated, lightning fast, and easy to use configuration library for Python. It is designed to be used in small to medium sized projects where a full blown configuration library is overkill.

This project is a Rust port of my NanoConf project, so inherits the usage patterns from that. However, it is roughtly 40x faster!

Installation
------------
```bash
uv pip install picoconf
```

Usage
-----
```python
from picoconf import PicoConf
# or if PicoConf if too long of a name
from picoconf import PC

# Create a new configuration object
config = PicoConf("/path/to/config.pconf")

# Access config values using dictionary-style access
print(config["some_key"])

# Or use dotted attribute access (recommended for cleaner code)
print(config.some_key)

# Both methods work interchangeably
assert config["some_key"] == config.some_key

# Nested values support both access methods too
print(config.database.host)  # attribute access
print(config["database"]["host"])  # dictionary access

# Convert to plain Python dict (recursively)
plain_dict = config.to_dict()
# All nested PicoConf objects become regular dicts
```

Key Normalization
-----------------
PicoConf is opinionated: **all config keys are normalized to lowercase** regardless of how they are defined. This applies to keys loaded from `.pconf` files, kwargs passed to the constructor, and keys introduced via environment variable overrides. It ensures consistent, cross-platform behavior (Windows treats environment variable names as case-insensitive).

```python
# Keys are always stored and accessed in lowercase
config = PC(**{"LOG_LEVEL": "debug", "Database_Host": "localhost"})
print(config.log_level)      # "debug"
print(config.database_host)  # "localhost"
```

Always use lowercase when reading config values, even if the source uses uppercase or mixed case.

Configuration File Format
-------------------------
PicoConf uses a simple configuration file format that is easy to read and write.
Each File is YAML formatted and contains a single top-level dictionary.
Even though the top-level must be a dictionary, you can nest dictionaries and lists as deep as you want.
Each config file also must have the .pconf extension. This ensures that PicoConf will only load files that are meant to be configuration files.
```yaml
key: value
test: 1
overriden: false
things:
    - thing1
    - thing2
    - thing3
top:
    v1: 1
    middle:
        v2: 2
        inner:
            v3: 3
            deep:
                v4: 4
```
If you have multiple config files you want to load into a single config object, you can put them all in the same directory and pass that directory to PicoConf.
PicoConf will automatically place sub-files by their filename as an attribute of the parent file.
The contents of that file will be accessible as you'd expect under the corresponding filename attribute.

```
<project root>
conf_dir
  |__ cfg1.pconf
  |__ cfg2.pconf
  |__ cfg3.pconf
```

```python
# load an entire directory
proj_config = PicoConf("/path/to/conf_dir")
print(proj_config.cfg1.test)
```

Or you can import additional files or directories from within any config file by using the `_import` keyword.
```yaml
# main.pconf
_import:
    - /path/to/project/more_config
key: value
test: 1
```

```
<project root>
main.pconf
more_config
  |__ subcfg1.pconf
  |__ subcfg2.pconf
  |__ subcfg3.pconf
```

```python
# loading the main config file will also load the sub-configs
proj_config = PicoConf("/path/to/project/main.pconf")
print(proj_config.more_config.subcfg1.test)
```
Notice how the directory structure was also maintained in the attribute path. This makes it easier to find the file that a value came from.

Environment Variables
---------------------
PicoConf supports environment variables either as overrides to existing values or as additions to the loaded config.
Envars are evaluated on a per-file basis, so you can have different envars for different config files.
The way we manage this is by having a special `_envar_prefix` key in the config file.
Because all keys are normalized to lowercase (see above), env var suffixes are matched case-insensitively by design — `MYAPP_LOG_LEVEL` and `myapp_log_level` both map to the `log_level` config key.
```yaml
_envar_prefix: myapp
key: value
overrideme: original
```
```bash
export myapp_overrideme=changed
```
```python
config = PicoConf("/path/to/config.pconf")
print(config.overrideme)
```

You can also pass complex data structures as JSON strings in environment variables.
```bash
export myapp_abc='{"a": 1, "b": 2, "c": 3}'
```
```python
config = PicoConf("/path/to/config.pconf")
print(config.abc.b)
```

### Overriding Individual Keys in Nested Sections

Because env vars are matched flat against each file's own prefix, there is no built-in delimiter (like `__`) for drilling into a nested section. The idiomatic way to get per-key env var control over a nested section is to split that section into its own file with its own `_envar_prefix`, then import it from the parent.

```
<project root>
main.pconf
connection.pconf
```

**`connection.pconf`** — owns the prefix for its own keys:
```yaml
_envar_prefix: myapp_connection
host: db.example.com
port: 5432
```

**`main.pconf`** — imports the file so the nesting is preserved:
```yaml
_envar_prefix: myapp
_import:
    - connection.pconf
key: value
```

```python
config = PicoConf("/path/to/main.pconf")
print(config.connection.host)  # db.example.com
```

Now individual keys in the nested section can be overridden without touching the rest:
```bash
export myapp_connection_host=prod-db.example.com
```

The access path (`config.connection.host`) stays the same — picoconf nests the imported file under its filename, so the structure is identical to having the values inline in `main.pconf`.

Converting to Plain Dictionaries
---------------------------------
PicoConf objects can be recursively converted to plain Python dictionaries using the `to_dict()` method. This is useful for serialization, passing to libraries that expect plain dicts, or API responses.

```python
config = PicoConf("/path/to/config.pconf")

# Convert entire config to plain dict
plain = config.to_dict()

# All nested PicoConf objects become regular dicts
assert isinstance(plain, dict)
assert not isinstance(plain, PicoConf)

# Works with deeply nested structures
if "database" in config:
    db_dict = config.database.to_dict()
    # Can now be serialized to JSON, YAML, etc.
```
