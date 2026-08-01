#!/bin/bash
# This file is part of the uutils grep package.
#
# For the full copyright and license information, please view the LICENSE
# file that was distributed with this source code.
#
# Run the upstream GNU grep testsuite against the Rust grep implementation.
#
# Unlike GNU coreutils, we do *not* build GNU grep here. Instead we reuse the
# gnulib test framework (tests/init.sh + tests/init.cfg) shipped in the GNU grep
# release tarball and inject our Rust `grep` binary via PATH, replicating the
# environment that tests/Makefile.am's TESTS_ENVIRONMENT would normally set up.
# Each test is classified by its gnulib exit code: 0 = PASS, 77 = SKIP, anything
# else = FAIL (timeouts and framework failures count as FAIL).
#
# Get the GNU grep sources with:
#   mkdir -p ../gnu.grep && (cd ../gnu.grep && bash ../grep/util/fetch-gnu.sh)
#
# Usage: ./util/run-gnu-testsuite.sh [options]
#
# Options:
#   -h, --help                Show this help message
#   -v, --verbose             Show diagnostics for failing/skipped tests
#   -q, --quiet               Only print failures and the final summary
#   --json-output FILE        Write results to FILE as JSON
#
# Environment variables:
#   GNU_GREP_DIR              Path to the extracted GNU grep source tree
#                             (default: ../gnu.grep)
#   RUN_EXPENSIVE_TESTS       Set to "yes" to run expensive tests (default: no)
#   PER_TEST_TIMEOUT          Per-test timeout in seconds (default: 30)

# Don't exit on failure since test failures are expected.
set -o pipefail

# Configuration
RUST_GREP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GNU_GREP_DIR="${GNU_GREP_DIR:-${RUST_GREP_DIR}/../gnu.grep}"
GNU_TESTS_DIR=""
VERBOSE=false
QUIET=false
JSON_OUTPUT_FILE=""
PER_TEST_TIMEOUT="${PER_TEST_TIMEOUT:-30}"
DETAILED_RESULTS=()

# Statistics
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
SKIPPED_TESTS=0

usage() {
    echo "Usage: $0 [options]"
    echo
    echo "Options:"
    echo "  -h, --help                Show this help message"
    echo "  -v, --verbose             Show diagnostics for failing/skipped tests"
    echo "  -q, --quiet               Only print failures and the final summary"
    echo "  --json-output FILE        Write results to FILE as JSON"
    echo
    echo "Environment variables:"
    echo "  GNU_GREP_DIR              Path to the extracted GNU grep source tree"
    echo "                            (default: ../gnu.grep)"
    echo "  RUN_EXPENSIVE_TESTS       Set to 'yes' to run expensive tests"
    echo "  PER_TEST_TIMEOUT          Per-test timeout in seconds (default: 30)"
    echo
    echo "Setup:"
    echo "  mkdir -p ../gnu.grep && (cd ../gnu.grep && bash ../grep/util/fetch-gnu.sh)"
}

log_info()    { [[ "$QUIET" != "true" ]] && echo "[INFO] $1"; return 0; }
log_success() { [[ "$QUIET" != "true" ]] && echo "[PASS] $1"; return 0; }
log_skip()    { [[ "$QUIET" != "true" ]] && echo "[SKIP] $1"; return 0; }
log_warning() { echo "[WARN] $1"; }
log_error()   { echo "[FAIL] $1"; }

# Generate JSON output (schema shared with ../sed so compare_test_results.py works).
generate_json_output() {
    cd "$RUST_GREP_DIR" || return

    local timestamp
    timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    local rust_version
    rust_version=$(cargo metadata --no-deps --format-version 1 2>/dev/null | jq -r '.packages[0].version // "unknown"')

    local tests_json="[]"
    if [[ ${#DETAILED_RESULTS[@]} -gt 0 ]]; then
        local temp_file
        temp_file=$(mktemp)
        printf "%s\n" "${DETAILED_RESULTS[@]}" > "$temp_file"
        tests_json=$(jq -s '.' < "$temp_file" 2>/dev/null) || tests_json="[]"
        rm -f "$temp_file"
    fi

    jq -n \
        --arg timestamp "$timestamp" \
        --argjson total "$TOTAL_TESTS" \
        --argjson passed "$PASSED_TESTS" \
        --argjson failed "$FAILED_TESTS" \
        --argjson skipped "$SKIPPED_TESTS" \
        --argjson duration "$duration" \
        --arg rust_version "$rust_version" \
        --arg gnu_testsuite_dir "$GNU_TESTS_DIR" \
        --argjson tests "$tests_json" \
        '{
            timestamp: $timestamp,
            summary: {
                total: $total,
                passed: $passed,
                failed: $failed,
                skipped: $skipped,
                duration_seconds: $duration
            },
            environment: {
                rust_grep_version: $rust_version,
                gnu_testsuite_dir: $gnu_testsuite_dir
            },
            tests: $tests
        }' > "$JSON_OUTPUT_FILE"

    log_info "JSON results written to: $JSON_OUTPUT_FILE"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help) usage; exit 0 ;;
        -v|--verbose) VERBOSE=true; shift ;;
        -q|--quiet) QUIET=true; shift ;;
        --json-output) JSON_OUTPUT_FILE="$2"; shift 2 ;;
        *) echo "Unknown argument: $1"; usage; exit 1 ;;
    esac
done

# Validate environment
if [[ -d "$GNU_GREP_DIR" ]]; then
    GNU_GREP_DIR="$(cd "$GNU_GREP_DIR" && pwd)"
    GNU_TESTS_DIR="$GNU_GREP_DIR/tests"
fi

if [[ ! -f "$GNU_TESTS_DIR/init.sh" ]]; then
    log_error "GNU grep testsuite not found at: $GNU_GREP_DIR"
    log_error "Fetch it with:"
    log_error "  mkdir -p ${RUST_GREP_DIR}/../gnu.grep && (cd ${RUST_GREP_DIR}/../gnu.grep && bash ${RUST_GREP_DIR}/util/fetch-gnu.sh)"
    exit 1
fi

if [[ ! -f "$RUST_GREP_DIR/Cargo.toml" ]]; then
    log_error "Not in a Rust project directory: $RUST_GREP_DIR"
    exit 1
fi

# Build the Rust grep implementation
log_info "Building Rust grep implementation..."
cd "$RUST_GREP_DIR" || exit 1
if ! cargo build --release --quiet; then
    log_error "Failed to build Rust grep implementation"
    exit 1
fi

RUST_GREP_BIN="$RUST_GREP_DIR/target/release/grep"
if [[ ! -x "$RUST_GREP_BIN" ]]; then
    log_error "Built grep binary not found at: $RUST_GREP_BIN"
    exit 1
fi
log_info "Using Rust grep binary: $RUST_GREP_BIN"

# Create a temporary work tree that mimics a GNU grep build directory.
TEST_WORK_DIR=$(mktemp -d)
trap 'rm -rf "$TEST_WORK_DIR"' EXIT
log_info "Test working directory: $TEST_WORK_DIR"

# A fake $abs_top_builddir whose src/ holds the binaries the tests expect.
BUILD_DIR="$TEST_WORK_DIR/build"
BIN_DIR="$BUILD_DIR/src"
mkdir -p "$BIN_DIR"

# grep, plus the egrep/fgrep wrappers a handful of tests rely on.
cat > "$BIN_DIR/grep" <<WRAPPER_EOF
#!/bin/sh
exec "$RUST_GREP_BIN" "\$@"
WRAPPER_EOF
cat > "$BIN_DIR/egrep" <<WRAPPER_EOF
#!/bin/sh
exec "$RUST_GREP_BIN" -E "\$@"
WRAPPER_EOF
cat > "$BIN_DIR/fgrep" <<WRAPPER_EOF
#!/bin/sh
exec "$RUST_GREP_BIN" -F "\$@"
WRAPPER_EOF
chmod +x "$BIN_DIR/grep" "$BIN_DIR/egrep" "$BIN_DIR/fgrep"

# Empty config.h: tests that probe it for build-time features just skip.
: > "$BUILD_DIR/config.h"

# get-mb-cur-max is a tiny standalone helper used by the locale require_ checks.
if [[ -f "$GNU_TESTS_DIR/get-mb-cur-max.c" ]]; then
    if cc -I"$BUILD_DIR" -o "$BIN_DIR/get-mb-cur-max" "$GNU_TESTS_DIR/get-mb-cur-max.c" 2>/dev/null; then
        log_info "Built get-mb-cur-max helper"
    else
        log_warning "Could not build get-mb-cur-max; multibyte/locale tests may skip"
    fi
fi

# Replicate the PCRE_WORKS probe from tests/Makefile.am's TESTS_ENVIRONMENT.
PCRE_WORKS=0
if err=$(echo . | "$BIN_DIR/grep" -Pq . 2>&1); then
    [[ -z "$err" ]] && PCRE_WORKS=1
fi
log_info "PCRE_WORKS=$PCRE_WORKS"

GREP_VERSION=$(basename "$GNU_GREP_DIR" | sed 's/^grep-//')
[[ "$GREP_VERSION" == "$(basename "$GNU_GREP_DIR")" ]] && GREP_VERSION="unknown"
HOST_TRIPLET="$(uname -m)-pc-linux-gnu"

# Record a test result (for JSON output)
record_result() {
    if [[ -n "$JSON_OUTPUT_FILE" ]]; then
        DETAILED_RESULTS+=("$(jq -n \
            --arg name "$1" --arg status "$2" --arg error "$3" \
            '{name: $name, status: $status, error: $error}')")
    fi
}

# Run a single GNU testsuite script with the Rust grep on PATH.
run_gnu_test() {
    local test_script="$1"
    local test_name
    test_name=$(basename "$test_script")

    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    local test_output_file="$TEST_WORK_DIR/test_output_$$"
    local test_exit_code=0

    # When not the process-group leader (e.g. in CI), GNU timeout falls back to
    # "foreground" mode and SIGTERMs the whole group on timeout. Shield the
    # parent script so a single hung test doesn't take the run down.
    trap '' TERM

    (
        cd "$TEST_WORK_DIR" || exit 99
        # init.cfg refuses to run if these are set.
        unset GREP_COLOR GREP_COLORS TERM CDPATH
        export PATH="$BIN_DIR:$PATH"
        export srcdir="$GNU_TESTS_DIR" abs_srcdir="$GNU_TESTS_DIR"
        export abs_top_srcdir="$GNU_GREP_DIR" top_srcdir="$GNU_GREP_DIR"
        export abs_top_builddir="$BUILD_DIR"
        export CONFIG_HEADER="$BUILD_DIR/config.h"
        export built_programs="grep egrep fgrep"
        export AWK=awk PERL=perl SHELL=/bin/sh MAKE=make CC=cc
        export LC_ALL=C MALLOC_PERTURB_=87
        export VERSION="$GREP_VERSION" PACKAGE_VERSION="$GREP_VERSION"
        export host_triplet="$HOST_TRIPLET"
        export PCRE_WORKS="$PCRE_WORKS"
        export GREP_TEST_NAME="$test_name"
        export RUN_EXPENSIVE_TESTS="${RUN_EXPENSIVE_TESTS:-no}"

        # fd 9 is the framework's stderr (init.cfg's stderr_fileno_=9).
        if [[ "$test_name" == *.pl ]]; then
            exec timeout --kill-after=5 "$PER_TEST_TIMEOUT" \
                perl -w -I"$GNU_TESTS_DIR" -MCoreutils -MCuSkip "$test_script" 9>&2
        else
            exec timeout --kill-after=5 "$PER_TEST_TIMEOUT" \
                /bin/sh "$test_script" 9>&2
        fi
    ) </dev/null >"$test_output_file" 2>&1
    test_exit_code=$?

    trap - TERM

    # Strip NUL bytes: some tests (e.g. z-anchor-newline) emit binary output,
    # which would otherwise trigger a "ignored null byte" warning from $(...).
    local test_output=""
    [[ -f "$test_output_file" ]] && test_output=$(tr -d '\0' < "$test_output_file")
    rm -f "$test_output_file"

    # 124 = GNU timeout, 125 = uutils timeout, >=128 = killed by signal.
    if [[ $test_exit_code -eq 124 || $test_exit_code -eq 125 || $test_exit_code -ge 128 ]]; then
        log_error "$test_name (timeout)"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        record_result "$test_name" "FAIL" "Test timed out after ${PER_TEST_TIMEOUT}s"
        return
    fi

    case $test_exit_code in
        0)
            log_success "$test_name"
            PASSED_TESTS=$((PASSED_TESTS + 1))
            record_result "$test_name" "PASS" ""
            ;;
        77)
            log_skip "$test_name"
            SKIPPED_TESTS=$((SKIPPED_TESTS + 1))
            [[ "$VERBOSE" == "true" ]] && echo "$test_output" | head -3 | sed 's/^/  | /'
            record_result "$test_name" "SKIP" "$test_output"
            ;;
        *)
            log_error "$test_name (exit $test_exit_code)"
            FAILED_TESTS=$((FAILED_TESTS + 1))
            [[ "$VERBOSE" == "true" ]] && echo "$test_output" | head -10 | sed 's/^/  | /'
            record_result "$test_name" "FAIL" "Exit code: $test_exit_code"
            ;;
    esac
}

# Discover the canonical test list from tests/Makefile.am's TESTS variable.
collect_tests() {
    awk '
        /^TESTS *\+?=/ { collect=1; sub(/^TESTS *\+?=/, "") }
        collect {
            line=$0
            cont=sub(/\\[ \t]*$/, "", line)
            n=split(line, a, /[ \t]+/)
            for (i=1; i<=n; i++) if (a[i] != "") print a[i]
            if (!cont) collect=0
        }
    ' "$GNU_TESTS_DIR/Makefile.am"
}

log_info "Discovering tests from $GNU_TESTS_DIR/Makefile.am"
mapfile -t TEST_LIST < <(collect_tests | sort -u)
log_info "Found ${#TEST_LIST[@]} tests"

log_info "Starting test execution..."
start_time=$(date +%s)

for t in "${TEST_LIST[@]}"; do
    [[ -z "$t" ]] && continue
    test_path="$GNU_TESTS_DIR/$t"
    [[ -f "$test_path" ]] || { log_warning "Listed test not found: $t"; continue; }
    run_gnu_test "$test_path"
done

end_time=$(date +%s)
duration=$((end_time - start_time))

# Print summary
echo
echo "========================================="
echo "GNU grep testsuite results"
echo "========================================="
echo "Total tests:   $TOTAL_TESTS"
echo "Passed:        $PASSED_TESTS"
echo "Failed:        $FAILED_TESTS"
echo "Skipped:       $SKIPPED_TESTS"
echo "Duration:      ${duration}s"

if [[ -n "$JSON_OUTPUT_FILE" ]]; then
    generate_json_output
fi

if [[ $((PASSED_TESTS + FAILED_TESTS)) -gt 0 ]]; then
    pass_rate=$(( (PASSED_TESTS * 100) / (PASSED_TESTS + FAILED_TESTS) ))
    echo "Pass rate:     ${pass_rate}%"
fi

# Mirror the script's exit convention to ../sed: nonzero if anything failed.
[[ $FAILED_TESTS -eq 0 ]] && exit 0 || exit 1
