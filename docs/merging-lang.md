# Merging launcher translations

`launcher/lang/*.json` are keyed by the English text. Two branches that add translations append
to the same end of every file, so a line-based merge reports conflicts even when the keys are
different. `tools/merge_lang_json.py` merges the JSON objects by key instead.

## Git

The repository's `.gitattributes` marks the files with the `json-union` driver. Configure the
driver once per clone:

```sh
git config merge.json-union.driver 'python3 tools/merge_lang_json.py %O %A %B'
```

Git then resolves added keys automatically, and drops a key one side deleted while the other left it
alone. A key changed differently on both sides is refused, not picked silently.

## Jujutsu

Jujutsu does not read Git merge drivers. Resolve the language files after a rebase with the
script directly:

```sh
python3 tools/merge_lang_json.py
```

It reads the conflicted sides from Git's index, keeps both sides' added keys, drops a key one side
deleted while the other left it alone, and writes valid JSON. A key changed differently on both
sides is refused.

For `jj resolve --tool`, add the same script as a Jujutsu merge tool:

```toml
[merge-tools.json-union]
program = "python3"
merge-args = ["tools/merge_lang_json.py", "$base", "$left", "$right", "$output"]
merge-conflict-exit-codes = [1]
```

Then `jj resolve --tool json-union` works on the language files.
