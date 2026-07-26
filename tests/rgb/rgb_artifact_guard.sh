#!/usr/bin/env bash

# Fail-closed artifact validation shared by the RGB regtest harness and its
# focused regression tests. Callers must keep the optional fixed-string pattern
# file outside the upload tree.

RGB_ARTIFACT_GUARD_FAILED=0
RGB_ARTIFACT_GUARD_CONTAINED=0
RGB_ARTIFACT_QUARANTINED_PATH=""

rgb_artifact_guard_quarantine() {
    local artifact_dir="$1"
    local quarantine_parent="$2"
    local reason="$3"
    local artifact_parent=""
    local quarantine_path=""
    local parent_quarantine_path=""

    RGB_ARTIFACT_GUARD_FAILED=1
    RGB_ARTIFACT_GUARD_CONTAINED=0
    RGB_ARTIFACT_QUARANTINED_PATH=""
    artifact_parent="$(dirname -- "$artifact_dir")"

    if [[ ! -d "$quarantine_parent" ]]; then
        mkdir -m 700 -p -- "$quarantine_parent" || true
    fi

    if [[ -d "$quarantine_parent" && ! -L "$quarantine_parent" && -O "$quarantine_parent" ]]; then
        quarantine_path="$(mktemp -d "${quarantine_parent}/unsafe-run.XXXXXX" 2>/dev/null || true)"
        if [[ -n "$quarantine_path" ]]; then
            rmdir -- "$quarantine_path" || true
            if mv -T -- "$artifact_dir" "$quarantine_path" 2>/dev/null; then
                RGB_ARTIFACT_QUARANTINED_PATH="$quarantine_path"
            fi
        fi
    fi

    if [[ -z "$RGB_ARTIFACT_QUARANTINED_PATH" ]]; then
        rm -rf -- "$artifact_dir" 2>/dev/null || true
    fi

    if [[ -e "$artifact_dir" || -L "$artifact_dir" ]]; then
        # Last-resort containment: move the complete upload root out of the
        # workflow's configured path, then recreate a sanitized root.
        if [[ -d "$quarantine_parent" && ! -L "$quarantine_parent" && -O "$quarantine_parent" ]]; then
            parent_quarantine_path="$(mktemp -d "${quarantine_parent}/unsafe-upload-root.XXXXXX" 2>/dev/null || true)"
            if [[ -n "$parent_quarantine_path" ]]; then
                rmdir -- "$parent_quarantine_path" || true
                if mv -T -- "$artifact_parent" "$parent_quarantine_path" 2>/dev/null; then
                    RGB_ARTIFACT_QUARANTINED_PATH="$parent_quarantine_path"
                fi
            fi
        fi
    fi

    if [[ -e "$artifact_dir" || -L "$artifact_dir" ]]; then
        return 1
    fi

    mkdir -m 700 -p -- "$artifact_dir" || return 1
    {
        printf '%s\n' 'failed: artifact run quarantined by fail-closed credential guard'
        printf 'reason=%s\n' "$reason"
    } >"${artifact_dir}/credential-leak-guard.txt" || return 1
    RGB_ARTIFACT_GUARD_CONTAINED=1
    return 0
}

rgb_copy_guarded_diagnostic() {
    local source_file="$1"
    local destination_file="$2"
    local artifact_dir="$3"
    local quarantine_parent="$4"
    local permissions=""

    [[ -e "$source_file" || -L "$source_file" ]] || return 0
    permissions="$(stat -c '%A' -- "$source_file" 2>/dev/null)" || {
        rgb_artifact_guard_quarantine \
            "$artifact_dir" "$quarantine_parent" "diagnostic-source-metadata-error" || true
        return 1
    }
    if [[ ! -f "$source_file" || -L "$source_file" || ! -O "$source_file" || ! -r "$source_file" || "${permissions:1:1}" != "r" ]]; then
        rgb_artifact_guard_quarantine \
            "$artifact_dir" "$quarantine_parent" "unsafe-diagnostic-source" || true
        return 1
    fi
    if ! cp -T -- "$source_file" "$destination_file" 2>/dev/null; then
        rgb_artifact_guard_quarantine \
            "$artifact_dir" "$quarantine_parent" "diagnostic-copy-error" || true
        return 1
    fi
    if [[ ! -f "$destination_file" || -L "$destination_file" || ! -O "$destination_file" || ! -r "$destination_file" ]]; then
        rgb_artifact_guard_quarantine \
            "$artifact_dir" "$quarantine_parent" "unsafe-copied-diagnostic" || true
        return 1
    fi
}

rgb_guard_retained_artifacts() {
    local artifact_dir="$1"
    local quarantine_parent="$2"
    local pattern_file="${3:-}"
    local workspace="$4"
    local inventory_file="${workspace}/artifact-inventory.bin"
    local find_error_file="${workspace}/artifact-find.err"
    local entry=""
    local relative_entry=""
    local permissions=""
    local grep_status=0
    local unsafe_reason=""

    RGB_ARTIFACT_GUARD_FAILED=0
    RGB_ARTIFACT_GUARD_CONTAINED=0
    RGB_ARTIFACT_QUARANTINED_PATH=""
    rm -f -- "$inventory_file" "$find_error_file"

    if [[ ! -d "$artifact_dir" || -L "$artifact_dir" || ! -O "$artifact_dir" ]]; then
        unsafe_reason="invalid-artifact-root"
    elif [[ -n "$pattern_file" && (! -f "$pattern_file" || -L "$pattern_file" || ! -r "$pattern_file" || ! -s "$pattern_file") ]]; then
        unsafe_reason="invalid-secret-pattern-source"
    elif ! find -P "$artifact_dir" -mindepth 1 -print0 >"$inventory_file" 2>"$find_error_file"; then
        unsafe_reason="artifact-traversal-error"
    fi

    if [[ -z "$unsafe_reason" ]]; then
        while IFS= read -r -d '' entry; do
            if [[ -L "$entry" ]]; then
                unsafe_reason="unexpected-symlink"
                break
            fi
            if [[ "${entry##*/}" == ".cookie" ]]; then
                unsafe_reason="forbidden-cookie-file"
                break
            fi
            if [[ -n "$pattern_file" ]]; then
                relative_entry="${entry#"${artifact_dir}/"}"
                grep_status=0
                printf '%s' "$relative_entry" | grep -aFq -f "$pattern_file" || grep_status=$?
                case "$grep_status" in
                    0)
                        unsafe_reason="credential-path-match"
                        break
                        ;;
                    1)
                        ;;
                    *)
                        unsafe_reason="credential-scanner-error"
                        break
                        ;;
                esac
            fi

            permissions="$(stat -c '%A' -- "$entry" 2>/dev/null)" || {
                unsafe_reason="artifact-metadata-error"
                break
            }
            if [[ -d "$entry" ]]; then
                if [[ ! -O "$entry" || ! -r "$entry" || ! -x "$entry" || "${permissions:1:1}" != "r" || "${permissions:3:1}" != "x" ]]; then
                    unsafe_reason="unreadable-directory"
                    break
                fi
            elif [[ -f "$entry" ]]; then
                if [[ ! -O "$entry" || ! -r "$entry" || "${permissions:1:1}" != "r" ]]; then
                    unsafe_reason="unreadable-file"
                    break
                fi
            else
                unsafe_reason="unexpected-filesystem-object"
                break
            fi
        done <"$inventory_file"
    fi

    if [[ -z "$unsafe_reason" && -n "$pattern_file" ]]; then
        while IFS= read -r -d '' entry; do
            [[ -f "$entry" && ! -L "$entry" ]] || continue
            grep_status=0
            grep -aFq -f "$pattern_file" -- "$entry" || grep_status=$?
            case "$grep_status" in
                0)
                    unsafe_reason="credential-content-match"
                    break
                    ;;
                1)
                    ;;
                *)
                    unsafe_reason="credential-scanner-error"
                    break
                    ;;
            esac
        done <"$inventory_file"
    fi

    rm -f -- "$inventory_file" "$find_error_file"
    if [[ -n "$unsafe_reason" ]]; then
        rgb_artifact_guard_quarantine "$artifact_dir" "$quarantine_parent" "$unsafe_reason" || return 1
        return 1
    fi

    printf '%s\n' \
        'passed: artifact tree is readable and regular; cookie files and loaded credential are absent' \
        >"${artifact_dir}/credential-leak-guard.txt"
    RGB_ARTIFACT_GUARD_CONTAINED=1
}
