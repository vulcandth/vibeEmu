import unittest

from update_test_status import CommandRun, categorize_tests, collect_test_results


class TestStatusIdentityTests(unittest.TestCase):
    def test_shared_rom_names_remain_in_both_suites(self):
        runs = [
            CommandRun([], "test:mooneye_extended", 0, ["test utils/dump_boot_hwio.gb ... ok\n"]),
            CommandRun([], "test:wilbertpol", 101, ["test utils/dump_boot_hwio.gb ... FAILED\n"]),
        ]
        results = collect_test_results(runs)
        categories = categorize_tests(results, {"mooneye_extended": "rom", "wilbertpol": "rom"})
        roms = categories["ROM Test Suites"]
        self.assertEqual(roms.total, 2)
        self.assertEqual(roms.modules["mooneye_extended"].passed, 1)
        self.assertEqual(roms.modules["wilbertpol"].failed, 1)

    def test_repeated_output_within_one_command_is_not_double_counted(self):
        run = CommandRun([], "lib", 0, ["test nested_subprocess_test ... ok\n"] * 2)
        results = collect_test_results([run])
        self.assertEqual(results, {("lib", "nested_subprocess_test"): "passed"})


if __name__ == "__main__":
    unittest.main()
