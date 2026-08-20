#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

BRANCH = "agent/vendor-cosmic-tweaks"
ROOT = Path(__file__).resolve().parents[1]
RESOURCES = ROOT / "src/tools/mattos-build/src/resources.rs"
SELF = ROOT / "DevUtils/fix_scheduler_pressure_starvation.py"
RESOURCES_REL = "src/tools/mattos-build/src/resources.rs"

OLD_POLICY = '''fn pressure_candidate(
    budget: &ResourceBudget,
    _swap_in_rate: f64,
    swap_out_rate: f64,
    psi_some_avg10: Option<f64>,
) -> PressureLevel {
    if budget.build_memory_bytes == 0
        || swap_out_rate >= 8.0
        || psi_some_avg10.is_some_and(|value| value >= 0.20)
    {
        PressureLevel::Critical
    } else if budget.available_memory_bytes <= budget.reserved_memory_bytes.saturating_mul(2)
        || swap_out_rate > 0.0
        || psi_some_avg10.is_some_and(|value| value >= 0.05)
    {
        PressureLevel::Constrained
    } else {
        PressureLevel::Healthy
    }
}
'''

NEW_POLICY = '''fn pressure_candidate(
    budget: &ResourceBudget,
    _swap_in_rate: f64,
    swap_out_rate: f64,
    psi_some_avg10: Option<f64>,
) -> PressureLevel {
    // Linux PSI `some` is the percentage of time in which at least one task is
    // stalled on memory while other work may still be making progress.  It is
    // therefore a throttling signal, not by itself a reason to stop every
    // memory-heavy stage.  Treating low-single-digit `some` PSI as Critical
    // caused permanent admission starvation: Critical permits zero heavy jobs,
    // so no new work could run to change the condition.
    //
    // Keep Critical for conditions that mean the scheduler has no safe memory
    // budget left, or the host is actively writing pages to swap at a sustained
    // rate.  `some` PSI still constrains parallelism to one heavy job.
    if budget.build_memory_bytes == 0 || swap_out_rate >= 8.0 {
        PressureLevel::Critical
    } else if budget.available_memory_bytes <= budget.reserved_memory_bytes.saturating_mul(2)
        || swap_out_rate > 0.0
        || psi_some_avg10.is_some_and(|value| value >= 0.05)
    {
        PressureLevel::Constrained
    } else {
        PressureLevel::Healthy
    }
}
'''

TEST_MARKER = "mod pressure_starvation_regression_tests"
TESTS = r'''

#[cfg(test)]
mod pressure_starvation_regression_tests {
    use super::*;

    fn roomy_budget() -> ResourceBudget {
        ResourceBudget {
            cpu_tokens: 12,
            build_memory_bytes: 6 * GIB,
            reserved_memory_bytes: 2 * GIB,
            available_memory_bytes: 8 * GIB,
        }
    }

    #[test]
    fn low_single_digit_some_psi_is_constrained_not_critical() {
        let budget = ResourceBudget {
            cpu_tokens: 12,
            // Mirrors the stalled laptop trace closely: about 1.4 GiB remains
            // available to builds after MattOS keeps its reserve.
            build_memory_bytes: 1400 * MIB,
            reserved_memory_bytes: 2 * GIB,
            available_memory_bytes: 3500 * MIB,
        };
        assert_eq!(
            pressure_candidate(&budget, 0.0, 0.0, Some(1.40)),
            PressureLevel::Constrained
        );
    }

    #[test]
    fn some_psi_alone_never_escalates_to_critical() {
        assert_eq!(
            pressure_candidate(&roomy_budget(), 0.0, 0.0, Some(100.0)),
            PressureLevel::Constrained
        );
    }

    #[test]
    fn sustained_swap_out_remains_critical() {
        assert_eq!(
            pressure_candidate(&roomy_budget(), 0.0, 8.0, Some(0.0)),
            PressureLevel::Critical
        );
    }

    #[test]
    fn exhausted_safe_build_memory_remains_critical() {
        let budget = ResourceBudget {
            cpu_tokens: 12,
            build_memory_bytes: 0,
            reserved_memory_bytes: 2 * GIB,
            available_memory_bytes: 2 * GIB,
        };
        assert_eq!(
            pressure_candidate(&budget, 0.0, 0.0, Some(0.0)),
            PressureLevel::Critical
        );
    }
}
'''


def run(*args: str) -> None:
    print("+", " ".join(args), flush=True)
    subprocess.run(args, cwd=ROOT, check=True)


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def tracked_changes() -> list[str]:
    raw = subprocess.check_output(
        ["git", "status", "--porcelain=v1", "--untracked-files=no"],
        cwd=ROOT,
        text=True,
    )
    return [line for line in raw.splitlines() if line]


def verify_recovery_state() -> None:
    unexpected: list[str] = []
    for line in tracked_changes():
        if len(line) < 4:
            unexpected.append(line)
            continue
        path = line[3:]
        if " -> " in path:
            path = path.split(" -> ", 1)[1]
        # The previous helper attempt intentionally modified only resources.rs
        # before failing on the invalid `--lib` test selector. Preserve and
        # resume that exact change; anything else still fails closed.
        if path != RESOURCES_REL:
            unexpected.append(line)
    if unexpected:
        raise SystemExit(
            "refusing to resume scheduler repair with unrelated tracked changes:\n"
            + "\n".join(unexpected)
        )


def apply_or_verify_fix() -> None:
    text = RESOURCES.read_text(encoding="utf-8")
    if NEW_POLICY not in text:
        if text.count(OLD_POLICY) != 1:
            raise SystemExit("resources.rs pressure policy is not in the expected state")
        text = text.replace(OLD_POLICY, NEW_POLICY, 1)

    if TEST_MARKER not in text:
        text = text.rstrip() + TESTS + "\n"

    RESOURCES.write_text(text, encoding="utf-8")


def main() -> None:
    if output("git", "branch", "--show-current") != BRANCH:
        raise SystemExit(f"expected branch {BRANCH!r}")

    verify_recovery_state()
    apply_or_verify_fix()

    # Format only the file we intentionally changed. `cargo fmt -p mattos-build`
    # formats unrelated existing drift elsewhere in the package.
    run("rustfmt", "--edition", "2024", RESOURCES_REL)
    run(
        "cargo",
        "test",
        "-p",
        "mattos-build",
        "resources::pressure_starvation_regression_tests",
        "--",
        "--nocapture",
    )
    # mattos-build is a binary crate (src/main.rs), not a library crate. Running
    # the package test command executes its complete unit-test suite.
    run("cargo", "test", "-p", "mattos-build")
    run("git", "diff", "--check")

    SELF.unlink()
    run(
        "git",
        "add",
        "-A",
        "--",
        RESOURCES_REL,
        str(SELF.relative_to(ROOT)),
    )
    run("git", "diff", "--cached", "--check")
    run("git", "commit", "-m", "Prevent scheduler PSI starvation")
    run("git", "push", "origin", f"HEAD:{BRANCH}")

    print("Scheduler PSI starvation fix tested, committed, and pushed.", flush=True)


if __name__ == "__main__":
    main()
