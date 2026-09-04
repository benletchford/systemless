#!/bin/sh
# Check the Phase 0 unified-runtime shape without requiring a Rust build.
#
# The baseline is intentionally non-zero.  This is a ratchet against adding
# more adapter-owned state, callback trampolines, or direct native-call action
# sites; it is not an assertion that the migration is complete.

set -eu

case "${1:-}" in
    ""|--check)
        run_fixture_tests=yes
        ;;
    --source-only)
        run_fixture_tests=no
        ;;
    --help|-h)
        printf '%s\n' "usage: $0 [--check|--source-only]"
        exit 0
        ;;
    *)
        printf '%s\n' "usage: $0 [--check|--source-only]" >&2
        exit 2
        ;;
esac

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
crate_dir=$(CDPATH= cd -- "${script_dir}/.." && pwd)
source_dir="${crate_dir}/src"
baseline="${crate_dir}/docs/architecture/unified-runtime-guardrail-baseline.txt"

if [ ! -f "${baseline}" ]; then
    printf '%s\n' "guardrail baseline is missing: ${baseline}" >&2
    exit 2
fi

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/systemless-unified-runtime.XXXXXX")
trap 'rm -rf "${tmp_dir}"' EXIT

field_names() {
    source_file=$1
    struct_name=$2

    awk -v wanted="${struct_name}" '
        $0 ~ "^[[:space:]]*(pub\\(crate\\)[[:space:]]+|pub[[:space:]]+)?struct[[:space:]]+" wanted "[[:space:]]*\\{" {
            inside = 1
            next
        }
        inside && $0 ~ "^[[:space:]]*}" { exit }
        inside {
            line = $0
            sub(/^[[:space:]]*(pub\(crate\)[[:space:]]+|pub[[:space:]]+)?/, "", line)
            if (line ~ /^[A-Za-z_][A-Za-z0-9_]*[[:space:]]*:/) {
                sub(/[[:space:]]*:.*$/, "", line)
                gsub(/[[:space:]]/, "", line)
                print line
            }
        }
    ' "${source_file}"
}

baseline_section() {
    section=$1

    awk -v wanted="${section}" '
        $0 == "[" wanted "]" { inside = 1; next }
        inside && /^\[/ { exit }
        inside && /^[A-Za-z_][A-Za-z0-9_]*=/ { exit }
        inside && /^[[:space:]]*#/ { next }
        inside && NF { print }
    ' "${baseline}"
}

baseline_value() {
    key=$1

    awk -F= -v wanted="${key}" '$1 == wanted { print $2; exit }' "${baseline}"
}

failures=0

check_field_set() {
    label=$1
    source_file=$2
    struct_name=$3
    section=$4
    current_file="${tmp_dir}/${section}.current"
    expected_file="${tmp_dir}/${section}.expected"

    field_names "${source_file}" "${struct_name}" | LC_ALL=C sort -u > "${current_file}"
    baseline_section "${section}" | LC_ALL=C sort -u > "${expected_file}"
    current_count=$(wc -l < "${current_file}" | tr -d '[:space:]')
    expected_count=$(wc -l < "${expected_file}" | tr -d '[:space:]')

    added=$(comm -23 "${current_file}" "${expected_file}")
    removed=$(comm -13 "${current_file}" "${expected_file}")
    if [ -n "${added}" ]; then
        printf '%s\n' "guardrail failed: ${label} changed" >&2
        printf '%s\n' "${label}: new entries:" >&2
        printf '%s\n' "${added}" >&2
        failures=$((failures + 1))
    fi
    if [ -n "${removed}" ]; then
        printf '%s\n' "${label}: entries removed; lower the baseline after reviewing the migration:" >&2
        printf '%s\n' "${removed}" >&2
        failures=$((failures + 1))
    fi

    printf '%s\n' "${label}: ${current_count} fields (baseline ${expected_count}); non-zero legacy debt is expected"
}

check_trampoline_set() {
    label=$1
    source_file=$2
    struct_name=$3
    section=$4
    current_file="${tmp_dir}/${section}.current"
    expected_file="${tmp_dir}/${section}.expected"

    field_names "${source_file}" "${struct_name}" \
        | awk '/trampoline/ { print }' \
        | LC_ALL=C sort -u > "${current_file}"
    baseline_section "${section}" | LC_ALL=C sort -u > "${expected_file}"
    current_count=$(wc -l < "${current_file}" | tr -d '[:space:]')
    expected_count=$(wc -l < "${expected_file}" | tr -d '[:space:]')

    added=$(comm -23 "${current_file}" "${expected_file}")
    removed=$(comm -13 "${current_file}" "${expected_file}")
    if [ -n "${added}" ]; then
        printf '%s\n' "guardrail failed: ${label} changed" >&2
        printf '%s\n' "${label}: new entries:" >&2
        printf '%s\n' "${added}" >&2
        failures=$((failures + 1))
    fi
    if [ -n "${removed}" ]; then
        printf '%s\n' "${label}: entries removed; lower the baseline after reviewing the migration:" >&2
        printf '%s\n' "${removed}" >&2
        failures=$((failures + 1))
    fi

    printf '%s\n' "${label}: ${current_count} fields (baseline ${expected_count}); non-zero legacy debt is expected"
}

check_field_count() {
    label=$1
    source_file=$2
    struct_name=$3
    baseline_key=$4

    current_count=$(field_names "${source_file}" "${struct_name}" | LC_ALL=C sort -u | wc -l | tr -d '[:space:]')
    expected_count=$(baseline_value "${baseline_key}")
    if [ -z "${expected_count}" ]; then
        printf '%s\n' "guardrail baseline is missing ${baseline_key}" >&2
        failures=$((failures + 1))
        return
    fi
    if [ "${current_count}" -gt "${expected_count}" ]; then
        printf '%s\n' "guardrail failed: ${label} grew from ${expected_count} to ${current_count} fields" >&2
        failures=$((failures + 1))
    elif [ "${current_count}" -lt "${expected_count}" ]; then
        printf '%s\n' "guardrail failed: ${label} shrank; lower ${baseline_key} in this change" >&2
        failures=$((failures + 1))
    fi
    printf '%s\n' "${label}: ${current_count} fields (baseline ${expected_count}); non-zero legacy debt is expected"
}

# Comparing the complete adapter aggregate shape catches a newly named
# manager field even when it does not match today's manager vocabulary.  The
# nested startup aggregate is checked separately because it is a second
# concentration of PPC-side Toolbox state.
check_field_set \
    "PpcLoadedApp top-level adapter state" \
    "${source_dir}/loader/ppc/mod.rs" \
    PpcLoadedApp \
    ppc_loaded_app_fields
check_field_set \
    "PpcToolboxStartupState top-level adapter state" \
    "${source_dir}/loader/ppc/mod.rs" \
    PpcToolboxStartupState \
    ppc_toolbox_startup_fields

# The runner and dispatcher mix legitimate execution/ABI state with migration
# debt. Count their complete shape so new ownership cannot be added silently;
# the named trampoline sets below make the highest-risk subset stricter by
# rejecting a rename that keeps the total count unchanged.
check_field_count \
    "FixtureRunner top-level ownership" \
    "${source_dir}/runner/mod.rs" \
    FixtureRunner \
    fixture_runner_field_count
check_field_count \
    "TrapDispatcher top-level ownership" \
    "${source_dir}/trap/dispatch.rs" \
    TrapDispatcher \
    trap_dispatcher_field_count

# Exact names also reject replacing a manager field while keeping the total.
check_field_set "FixtureRunner fields" "${source_dir}/runner/mod.rs" FixtureRunner fixture_runner_fields
check_field_set "TrapDispatcher fields" "${source_dir}/trap/dispatch.rs" TrapDispatcher trap_dispatcher_fields

# These are the current manager/callback trampoline fields.  New names must
# first be justified in the operation ledger and assigned to the execution
# layer before they can enter the baseline.
check_trampoline_set \
    "FixtureRunner trampoline fields" \
    "${source_dir}/runner/mod.rs" \
    FixtureRunner \
    fixture_runner_trampolines
check_trampoline_set \
    "TrapDispatcher trampoline fields" \
    "${source_dir}/trap/dispatch.rs" \
    TrapDispatcher \
    trap_dispatcher_trampolines

# `src/guest_call.rs` is the current execution/continuation layer.  Every
# remaining lexical use is migration debt (including match arms and tests),
# so a new use cannot hide in a manager or loader module.
native_call_count=$(
    find "${source_dir}" -type f -name '*.rs' ! -path "${source_dir}/guest_call.rs" \
        -exec grep -Eo 'PpcImportAction[[:space:]]*::[[:space:]]*CallNative[[:space:]]*\{' {} + \
        | wc -l | tr -d '[:space:]'
)
# A shrinking count must lower the baseline in the same patch. Otherwise a
# later reintroduction can silently use the abandoned allowance.
check_exact_count() {
    label=$1
    key=$2
    current=$3
    expected=$(baseline_value "${key}")
    case "${expected}" in
        ""|*[!0-9]*)
            printf '%s\n' "guardrail failed: missing or invalid ${key}" >&2
            failures=$((failures + 1))
            return
            ;;
    esac
    if [ "${current}" -ne "${expected}" ]; then
        printf '%s\n' "guardrail failed: ${label} is ${current}, baseline ${expected}; removals must lower the baseline" >&2
        failures=$((failures + 1))
    fi
    printf '%s\n' "${label}: ${current} (baseline ${expected})"
}

check_exact_count "Direct native calls" ppc_import_action_call_native_occurrences "${native_call_count}"

# These lexical inventories intentionally include tests. Moving the syntax
# into another module does not remove the debt. An implementation migration
# must remove its old construction/conversion path as well.
conversion_count=$(
    find "${source_dir}" -type f -name '*.rs' ! -path "${source_dir}/guest_call.rs" \
        -exec grep -Eo '\.into_ppc_import_action[[:space:]]*\(' {} + | wc -l | tr -d '[:space:]'
)
stamping_count=$(
    find "${source_dir}" -type f -name '*.rs' \
        -exec grep -Eo '\.with_task\(self\.current_task\(\)\)' {} + | wc -l | tr -d '[:space:]'
)
depth_count=$(
    find "${source_dir}" -type f -name '*.rs' \
        -exec grep -Eo '\.suspended_m68k_context_depth[[:space:]]*\(' {} + | wc -l | tr -d '[:space:]'
)
check_exact_count "Adapter effect conversions" adapter_effect_conversions "${conversion_count}"
check_exact_count "Current-task rewrites" current_task_rewrites "${stamping_count}"
check_exact_count "Depth-based context queries" depth_context_queries "${depth_count}"

# Keep named compatibility boundaries visible, including overloads. The
# prefix makes a move to a different file visible instead of forgiving it.
for relative in process_context.rs runner/mod.rs loader/ppc/mod.rs trap/dispatch.rs; do
    awk -v prefix="${relative}:" '
        /fn (attach_|activate_copy_to|activate_quickdraw_selection|sync_ppc_|adopt_ppc_|share_ppc_)/ {
            line = $0
            sub(/^.*fn /, "", line)
            sub(/[^A-Za-z0-9_].*$/, "", line)
            print prefix line
        }
    ' "${source_dir}/${relative}"
done | LC_ALL=C sort > "${tmp_dir}/methods.current"
baseline_section compatibility_methods | LC_ALL=C sort > "${tmp_dir}/methods.expected"
if ! diff -u "${tmp_dir}/methods.expected" "${tmp_dir}/methods.current"; then
    printf '%s\n' "guardrail failed: compatibility methods changed; retire or classify the boundary in the ledger" >&2
    failures=$((failures + 1))
fi
printf '%s\n' "Compatibility methods: $(wc -l < "${tmp_dir}/methods.current" | tr -d '[:space:]')"

# The semantic execution kernel must stay independent of concrete CPU values
# and the PPC import action protocol. Concrete register/context parking is a
# temporary guest_call/runner projection keyed by CallId, not kernel state.
kernel_isa_count=$(
    awk '
        /PpcCpu|PpcImportAction|PpcNativeReturnGpr3|use[[:space:]]+(m68k|ppc)::/ { count += 1 }
        END { print count + 0 }
    ' "${source_dir}/execution_kernel.rs"
)
if [ "${kernel_isa_count}" -ne 0 ]; then
    printf '%s\n' "guardrail failed: execution_kernel.rs contains ${kernel_isa_count} concrete CPU/import-action references" >&2
    failures=$((failures + 1))
fi
printf '%s\n' "Execution kernel concrete CPU/import-action references: ${kernel_isa_count} (required 0)"

if [ "${failures}" -ne 0 ]; then
    printf '%s\n' "unified-runtime guardrails failed; update the contract ledger with any intentional migration before changing the baseline" >&2
    exit 1
fi

if [ "${run_fixture_tests}" = yes ]; then
    python3 "${script_dir}/test-unified-runtime-guardrails.py"
fi

printf '%s\n' "unified-runtime guardrails passed; the baseline remains intentionally non-zero"
