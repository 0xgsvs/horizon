import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location(
    "merge_lang_json", ROOT / "tools/merge_lang_json.py"
)
merge_lang_json = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(merge_lang_json)


class MergeLangJsonTest(unittest.TestCase):
    def test_jj_conflict_lines_with_padding_are_parsed(self):
        output = "launcher/lang/ar.json       2-sided conflict\n"
        self.assertEqual(merge_lang_json.jj_paths(output), ["launcher/lang/ar.json"])
        self.assertEqual(merge_lang_json.jj_paths("No conflicts found at this revision\n"), [])

    def test_keys_added_on_either_side_are_kept(self):
        self.assertEqual(
            merge_lang_json.merge({"a": "A"}, {"a": "A", "b": "B"}, {"a": "A", "c": "C"}),
            {"a": "A", "b": "B", "c": "C"},
        )

    def test_identical_changes_are_accepted(self):
        self.assertEqual(
            merge_lang_json.merge({"a": "old"}, {"a": "new"}, {"a": "new"}),
            {"a": "new"},
        )

    def test_one_sided_changes_are_accepted(self):
        self.assertEqual(
            merge_lang_json.merge({"a": "old"}, {"a": "new"}, {"a": "old"}),
            {"a": "new"},
        )
        self.assertEqual(
            merge_lang_json.merge({"a": "old"}, {"a": "old"}, {"a": "new"}),
            {"a": "new"},
        )

    def test_different_changes_are_refused(self):
        with self.assertRaises(ValueError):
            merge_lang_json.merge({"a": "old"}, {"a": "left"}, {"a": "right"})

    def test_a_key_the_other_side_deleted_goes(self):
        self.assertEqual(
            merge_lang_json.merge({"a": "A", "b": "B"}, {"a": "A", "b": "B"}, {"a": "A"}),
            {"a": "A"},
        )
        self.assertEqual(
            merge_lang_json.merge({"a": "A", "b": "B"}, {"a": "A"}, {"a": "A", "b": "B"}),
            {"a": "A"},
        )

    def test_changing_a_key_the_other_side_deleted_is_refused(self):
        with self.assertRaises(ValueError):
            merge_lang_json.merge({"a": "A", "b": "B"}, {"a": "A", "b": "B2"}, {"a": "A"})


if __name__ == "__main__":
    unittest.main()
