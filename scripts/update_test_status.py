#!/usr/bin/env python3
"""
Update TEST_STATUS.md with the latest cargo test results (including ignored tests).
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, Iterable, List, Tuple

REPO_ROOT = Path(__file__).resolve().parents[1]
TEST_STATUS_FILE = REPO_ROOT / "TEST_STATUS.md"
TESTS_DIR = REPO_ROOT / "crates" / "vibe-emu-core" / "tests"

TEST_RESULT_RE = re.compile(
    r"^test\s+(?P<name>.+?)\s+\.\.\.\s+(?P<status>ok|FAILED|ignored|measured)(?:\s+\([^)]*\))?$"
)

STATUS_DISPLAY = {
    "passed": "✅ Pass",
    "failed": "❌ Fail",
    "ignored": "⚪ Ignored",
    "measured": "📏 Measured",
    "other": "❔ Unknown",
}

STATUS_ORDER = {"passed": 0, "failed": 1, "ignored": 2, "measured": 3, "other": 4}


@dataclass
class ModuleSummary:
    name: str
    tests: List[Tuple[str, str]] = field(default_factory=list)  # (test_name, status_key)
    counts: Counter = field(default_factory=Counter)

    def add(self, test_name: str, status_key: str) -> None:
        self.tests.append((test_name, status_key))
        self.counts[status_key] += 1

    @property
    def total(self) -> int:
        return sum(self.counts.values())

    @property
    def passed(self) -> int:
        return self.counts.get("passed", 0)

    @property
    def failed(self) -> int:
        return self.counts.get("failed", 0)

    @property
    def ignored(self) -> int:
        return self.counts.get("ignored", 0)

    @property
    def measured(self) -> int:
        return self.counts.get("measured", 0)

    @property
    def other(self) -> int:
        return self.counts.get("other", 0)

    @property
    def pass_percentage(self) -> float:
        total = self.total
        if total == 0:
            return 0.0
        return (self.passed / total) * 100.0


@dataclass
class CategorySummary:
    name: str
    modules: Dict[str, ModuleSummary] = field(default_factory=dict)

    def add(self, module_name: str, test_name: str, status_key: str) -> None:
        module = self.modules.setdefault(module_name, ModuleSummary(module_name))
        module.add(test_name, status_key)

    @property
    def counts(self) -> Counter:
        total = Counter()
        for module in self.modules.values():
            total.update(module.counts)
        return total

    @property
    def total(self) -> int:
        return sum(module.total for module in self.modules.values())

    @property
    def pass_percentage(self) -> float:
        total = self.total
        if total == 0:
            return 0.0
        passed = sum(module.passed for module in self.modules.values())
        return (passed / total) * 100.0


@dataclass
class CommandRun:
    command: List[str]
    hint: str
    exit_code: int
    output: List[str]


def gather_integration_modules() -> Dict[str, str]:
    modules: Dict[str, str] = {}
    if not TESTS_DIR.exists():
        return modules

    for path in TESTS_DIR.glob("*.rs"):
        if path.stem == "common":
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            continue
        category = "rom" if "rom_path" in text else "integration"
        modules[path.stem] = category
    return modules


def build_test_commands(integration_modules: Dict[str, str]) -> List[Tuple[List[str], str]]:
    commands: List[Tuple[List[str], str]] = []
    commands.append((["cargo", "test", "--lib", "--", "--include-ignored"], "lib"))

    bin_target = REPO_ROOT.name
    if (REPO_ROOT / "src" / "main.rs").exists():
        commands.append((["cargo", "test", "--bin", bin_target, "--", "--include-ignored"], f"bin:{bin_target}"))

    commands.append((["cargo", "test", "--doc"], "doc"))

    for module in sorted(integration_modules):
        commands.append((["cargo", "test", "--test", module, "--", "--include-ignored"], f"test:{module}"))

    return commands


def run_command(cmd: List[str]) -> Tuple[int, List[str]]:
    print(f"\n=== Running: {' '.join(cmd)} ===")
    sys.stdout.flush()

    output_lines: List[str] = []
    process = subprocess.Popen(
        cmd,
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    assert process.stdout is not None
    for line in process.stdout:
        print(line, end="")
        output_lines.append(line)
    return_code = process.wait()
    return return_code, output_lines


def run_cargo_tests(commands: List[Tuple[List[str], str]]) -> Tuple[int, List[CommandRun]]:
    runs: List[CommandRun] = []
    overall_exit = 0
    for cmd, hint in commands:
        code, output = run_command(cmd)
        runs.append(CommandRun(command=cmd, hint=hint, exit_code=code, output=output))
        if code != 0:
            overall_exit = code
    return overall_exit, runs


def normalize_status(raw_status: str) -> str:
    status = raw_status.strip().upper()
    if status == "OK":
        return "passed"
    if status == "FAILED":
        return "failed"
    if status == "IGNORED":
        return "ignored"
    if status == "MEASURED":
        return "measured"
    return "other"


def collect_test_results(runs: Iterable[CommandRun]) -> Dict[str, Tuple[str, str]]:
    results: Dict[str, Tuple[str, str]] = {}
    for run in runs:
        for raw_line in run.output:
            line = raw_line.strip()
            match = TEST_RESULT_RE.match(line)
            if not match:
                continue
            test_name = match.group("name").strip()
            raw_status = match.group("status")
            status_key = normalize_status(raw_status)
            previous = results.get(test_name)
            if previous and previous[0] != status_key:
                print(
                    f"Warning: conflicting results for {test_name}: {previous[0]} vs {status_key}",
                    file=sys.stderr,
                )
            results[test_name] = (status_key, run.hint)
    return results


def categorize_tests(
    tests: Dict[str, Tuple[str, str]],
    integration_modules: Dict[str, str],
) -> Dict[str, CategorySummary]:
    categories: Dict[str, CategorySummary] = {
        "ROM Test Suites": CategorySummary("ROM Test Suites"),
        "Integration Tests": CategorySummary("Integration Tests"),
        "Unit Tests": CategorySummary("Unit Tests"),
        "Doc Tests": CategorySummary("Doc Tests"),
        "Other": CategorySummary("Other"),
    }

    rom_modules = {name for name, cat in integration_modules.items() if cat == "rom"}
    integration_only = set(integration_modules)

    for full_name, (status_key, hint) in tests.items():
        if hint == "doc" or full_name.startswith("src/"):
            module_name = full_name.split(" - ", 1)[0]
            categories["Doc Tests"].add(module_name, full_name, status_key)
            continue

        if hint == "lib" or hint.startswith("bin:") or "::tests::" in full_name:
            module_name = full_name.split("::tests::", 1)[0]
            categories["Unit Tests"].add(module_name, full_name, status_key)
            continue

        if hint.startswith("test:"):
            target = hint.split(":", 1)[1]
            if target in rom_modules:
                categories["ROM Test Suites"].add(target, full_name, status_key)
            elif target in integration_only:
                categories["Integration Tests"].add(target, full_name, status_key)
            else:
                categories["Unit Tests"].add(target, full_name, status_key)
            continue

        primary = full_name.split("::", 1)[0]
        if primary in rom_modules:
            categories["ROM Test Suites"].add(primary, full_name, status_key)
        elif primary in integration_only:
            categories["Integration Tests"].add(primary, full_name, status_key)
        else:
            categories["Unit Tests"].add(primary, full_name, status_key)

    # Remove empty categories to avoid clutter
    return {name: cat for name, cat in categories.items() if cat.modules}


def build_summary_table(categories: Dict[str, CategorySummary]) -> str:
    headers = ["Category", "Passed", "Failed", "Ignored", "Measured", "Total", "Pass %"]
    lines = ["| " + " | ".join(headers) + " |", "| " + " | ".join(["---"] * len(headers)) + " |"]

    overall_counts = Counter()
    for category_name, category in categories.items():
        counts = category.counts
        total = category.total
        overall_counts.update(counts)
        row = [
            category_name,
            str(counts.get("passed", 0)),
            str(counts.get("failed", 0)),
            str(counts.get("ignored", 0)),
            str(counts.get("measured", 0)),
            str(total),
            f"{category.pass_percentage:.1f}%",
        ]
        lines.append("| " + " | ".join(row) + " |")

    overall_total = sum(overall_counts.values())
    if overall_total:
        overall_pass_pct = (overall_counts.get("passed", 0) / overall_total) * 100.0
    else:
        overall_pass_pct = 0.0
    overall_row = [
        "**Overall**",
        str(overall_counts.get("passed", 0)),
        str(overall_counts.get("failed", 0)),
        str(overall_counts.get("ignored", 0)),
        str(overall_counts.get("measured", 0)),
        str(overall_total),
        f"{overall_pass_pct:.1f}%",
    ]
    lines.append("| " + " | ".join(overall_row) + " |")
    return "\n".join(lines)


def summarize_failures(categories: Dict[str, CategorySummary]) -> List[str]:
    failures: List[Tuple[str, str, str]] = []
    for category_name, category in categories.items():
        for module_name, module in category.modules.items():
            for test_name, status_key in module.tests:
                if status_key == "failed":
                    failures.append((category_name, module_name, test_name))
    failures.sort()
    return [f"- `{test}` _(Category: {category}; Module: {module})_" for category, module, test in failures]


def format_module_section(module: ModuleSummary) -> str:
    heading = f"#### {module.name} ({module.passed}/{module.total} passing, {module.pass_percentage:.1f}%)"
    lines = [heading, "", "| Test | Result |", "| --- | --- |"]
    for test_name, status_key in sorted(module.tests, key=lambda item: (STATUS_ORDER.get(item[1], 99), item[0])):
        display = STATUS_DISPLAY.get(status_key, STATUS_DISPLAY["other"])
        lines.append(f"| `{test_name}` | {display} |")
    lines.append("")
    return "\n".join(lines)


def build_category_sections(categories: Dict[str, CategorySummary]) -> str:
    sections: List[str] = []
    for category_name, category in categories.items():
        sections.append(f"### {category_name}\n")
        for module_name in sorted(category.modules):
            module = category.modules[module_name]
            sections.append(format_module_section(module))
        sections.append("")
    return "\n".join(sections).strip() + "\n"


def render_markdown(
    categories: Dict[str, CategorySummary],
    commands: List[List[str]],
    cargo_exit_code: int,
) -> str:
    summary_table = build_summary_table(categories)
    failure_lines = summarize_failures(categories)
    failure_section = "\n".join(failure_lines) if failure_lines else "- None"
    category_sections = build_category_sections(categories)

    command_lines = "\n".join(f"- `{' '.join(cmd)}`" for cmd in commands)

    markdown_parts = [
        "# Test Status\n",
        "_Generated by `scripts/update_test_status.py`_\n",
        "\nCommands executed:",
        command_lines,
        f"\n\nCombined exit code: {cargo_exit_code}\n",
        "\n## Overall Summary\n",
        summary_table,
        "\n## Failing Tests\n",
        failure_section,
        "\n## Detailed Results\n",
        category_sections,
    ]
    return "\n".join(markdown_parts).strip() + "\n"


def main() -> int:
    os.chdir(REPO_ROOT)
    integration_modules = gather_integration_modules()
    commands_with_hints = build_test_commands(integration_modules)
    cargo_exit_code, runs = run_cargo_tests(commands_with_hints)
    results = collect_test_results(runs)

    if not results:
        print("Warning: No test results were parsed.", file=sys.stderr)

    categories = categorize_tests(results, integration_modules)
    markdown = render_markdown(
        categories,
        [run.command for run in runs],
        cargo_exit_code,
    )
    TEST_STATUS_FILE.write_text(markdown, encoding="utf-8")
    print(f"\nUpdated {TEST_STATUS_FILE.relative_to(REPO_ROOT)}")
    if cargo_exit_code != 0:
        print(
            "Note: one or more test commands exited with a non-zero status.",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
