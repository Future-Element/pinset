#!/bin/sh
# Standalone full uninstall; no Pinset or Python dependency.
set -eu
plan=0 yes=0 incomplete=0
homes='' installs='' projects='' targets='' roots='' profiles=''
nl='
'
fail() { printf 'pinset uninstall: %s\n' "$*" >&2; exit 1; }
notice() { printf 'pinset uninstall: %s\n' "$*" >&2; incomplete=1; }
safe_path() {
    case "$1" in /*) ;; *) fail "path must be absolute: $1" ;; esac
    case "$1" in *"$nl"*|*'|'*|*/../*|*/..|*/./*|*/.|*'//'*) fail "ambiguous path: $1" ;; esac
    checked=${1%/}
    case "$checked" in ''|/|/bin|/sbin|/usr|/usr/bin|/usr/local|/usr/local/bin|/opt|/etc|/var|/tmp|/Users|/home|"$HOME") fail "unsafe target: $1" ;; esac
    ancestor=$checked
    while [ "$ancestor" != / ] && [ -n "$ancestor" ]; do
        [ ! -L "$ancestor" ] || fail "refusing linked target or ancestor: $ancestor"
        ancestor=${ancestor%/*}; [ -n "$ancestor" ] || ancestor=/
    done
}
add_target() {
    kind=$1 path=$2
    [ -e "$path" ] || [ -L "$path" ] || return 0
    case "$path" in *"$nl"*|*'|'*) fail "unsupported filename: $path" ;; esac
    if [ "$kind" = D ]; then safe_path "$path"
    else
        # The leaf may be a command symlink; its containing path may not be.
        parent=${path%/*}; safe_path "$parent/.pinset-uninstall-check"
    fi
    case "$targets$nl" in *"$nl$kind|$path$nl"*) return ;; esac
    targets="$targets$nl$kind|$path"
}
owned_v3() { [ -f "$1/.pinset-home.json" ] && [ ! -L "$1/.pinset-home.json" ] && grep -Eq '"protocol"[[:space:]]*:[[:space:]]*"pinset/3"' "$1/.pinset-home.json"; }
owned_project() { [ -f "$1/.pinset/config.toml" ] && [ ! -L "$1/.pinset/config.toml" ] && grep -Eq '^protocol[[:space:]]*=[[:space:]]*"pinset/3"[[:space:]]*$' "$1/.pinset/config.toml"; }
legacy_owned() {
    for config in "$1/state/global.toml" "$1/state/global.lock" "$1/state/identities.toml"; do
        [ -f "$config" ] && [ ! -L "$config" ] || continue
        grep -Eq '^schema[[:space:]]*=[[:space:]]*[12345][[:space:]]*$' "$config" || continue
        case "$config" in
            */global.toml) grep -Eq '^\[tools\][[:space:]]*$' "$config" && return 0 ;;
            */global.lock) grep -Eq '^\[\[tool\]\][[:space:]]*$' "$config" && return 0 ;;
            */identities.toml) grep -Eq '^\[\[identities\]\][[:space:]]*$' "$config" && return 0 ;;
        esac
    done
    return 1
}
collect_profiles() {
    for profile in "$HOME/.profile" "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.bashrc" \
        "$HOME/.zshrc" "$HOME/.zprofile" "$HOME/.zshenv" "$HOME/.zlogin" \
        "${ZDOTDIR:-$HOME}/.zshrc" "${ZDOTDIR:-$HOME}/.zprofile" "${ZDOTDIR:-$HOME}/.zshenv" \
        "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" \
        /etc/environment /etc/profile /etc/bash.bashrc /etc/zsh/zshrc /etc/zsh/zshenv \
        "${XDG_CONFIG_HOME:-$HOME/.config}"/environment.d/*.conf \
        "${XDG_CONFIG_HOME:-$HOME/.config}"/fish/conf.d/*.fish \
        /etc/environment.d/*.conf /etc/profile.d/*.sh /etc/paths.d/*; do
        [ ! -f "$profile" ] || profiles="$profiles$nl$profile"
    done
}
profile_homes() {
    awk -v home="$HOME" '
    BEGIN {sq=sprintf("%c",39)}
    /^[[:space:]]*(export[[:space:]]+)?PINSET_(HOME|INSTALL_DIR)[[:space:]]*=/ {
        s=$0; sub(/^[[:space:]]*(export[[:space:]]+)?/,"",s)
        name=s; sub(/[[:space:]]*=.*/,"",name)
        sub(/^[^=]*=[[:space:]]*/,"",s); sub(/[[:space:]]*$/,"",s)
        q=substr(s,1,1)
        if(q==sq || q=="\"") {
            if(substr(s,length(s),1)!=q) {bad=1; next}
            s=substr(s,2,length(s)-2)
        }
        if(index(s,"$HOME/")==1) s=home substr(s,6)
        if(index(s,"${HOME}/")==1) s=home substr(s,8)
        if(substr(s,1,1)!="/" || s ~ /[$;|<>]/ || index(s,"\\") || index(s,"\"") || index(s,sq)) {bad=1; next}
        print name "|" s
    }
    END {if(bad) exit 3}' "$1"
}
# Strict string-map JSON parser. No eval; unusual escape sequences fail closed.
registry_roots() {
    awk '
    function ws() {while(p<=length(s) && substr(s,p,1) ~ /[ \t\r\n]/) p++}
    function str( c,r,e) {
        if(substr(s,p++,1)!="\"") {invalid=1; return ""}
        r=""
        while(p<=length(s)) {
            c=substr(s,p++,1)
            if(c=="\"") return r
            if(c=="\\") {
                e=substr(s,p++,1)
                if(e=="\"" || e=="\\" || e=="/") c=e
                else {invalid=1; return ""}
            }
            if(c ~ /[[:cntrl:]]/) {invalid=1; return ""}
            r=r c
        }
        invalid=1; return ""
    }
    {s=s $0 "\n"}
    END {
        p=1; ws(); if(substr(s,p++,1)!="{") exit 2; ws()
        if(substr(s,p,1)=="}") {p++; ws(); if(p<=length(s)) exit 2; exit}
        while(p<=length(s)) {
            root=str(); ws(); if(invalid || substr(s,p++,1)!=":") exit 2
            ws(); config=str(); if(invalid || substr(root,1,1)!="/" || config!=root "/.pinset/config.toml") exit 2
            output=output root "\n"; ws(); c=substr(s,p++,1)
            if(c=="}") break
            if(c!=",") exit 2
            ws()
        }
        ws(); if(c!="}" || p<=length(s)) exit 2
        printf "%s",output
    }' "$1"
}
# Preserve ambiguous mixed/multiline commands for review. Never execute profiles.
filter_profile() {
    directory_vars_resolved=1
    profile_homes "$1" >/dev/null || directory_vars_resolved=0
    awk -v roots="$roots" -v directory_vars_resolved="$directory_vars_resolved" '
    BEGIN {n=split(roots,r,"\n"); sq=sprintf("%c",39)}
    function owned(s, i) {
        if(s ~ /PINSET_[A-Za-z0-9_]+/ || s ~ /(^|[[:space:](])pinset[[:space:]]/ || s ~ /\$\{?HOME\}?\/\.pinset(\/|:|["[:space:]]|$)|~\/\.pinset(\/|:|["[:space:]]|$)/) return 1
        for(i=1;i<=n;i++) if(r[i]!="") {
            at=index(s,r[i]); after=substr(s,at+length(r[i]),1)
            if(at && (after=="/" || after=="\"" || after==sq || after==":" || after=="" || after ~ /[[:space:]]/)) return 1
        }
        return 0
    }
    {
        line=$0
        if(!owned(line) || line ~ /^[[:space:]]*#/) {print; next}
        if(line ~ /;|&&|\|\||\\[[:space:]]*$/) {print; bad=1; next}
        if(!directory_vars_resolved && line ~ /^[[:space:]]*(export[[:space:]]+)?PINSET_(HOME|INSTALL_DIR)[[:space:]]*=/) {print; bad=1; next}
        if(line ~ /^[[:space:]]*(export[[:space:]]+)?PINSET_[A-Za-z0-9_]+[[:space:]]*=/) next
        if(line ~ /^[[:space:]]*(export[[:space:]]+)?(GOPATH|PYTHONPATH)[[:space:]]*=/ && index(line,":")) {print; bad=1; next}
        if(line ~ /^[[:space:]]*(export[[:space:]]+)?(JAVA_HOME|GOROOT|GOPATH|CARGO_HOME|RUSTUP_HOME|FLUTTER_ROOT|VIRTUAL_ENV|PYTHONHOME|PYTHONPATH)[[:space:]]*=/) next
        if(line ~ /^[[:space:]]*fish_add_path[[:space:]]/) {
            value=line; sub(/^[[:space:]]*fish_add_path[[:space:]]+((--path[[:space:]]+--move[[:space:]]+)?--prepend[[:space:]]+)?/,"",value)
            valid=0
            for(i=1;i<=n;i++) if(r[i]!="" && (value==sq r[i] "/bin" sq || value=="\"" r[i] "/bin\"" || value==r[i] "/bin")) valid=1
            if(valid) next
            print; bad=1; next
        }
        if(line ~ /^[[:space:]]*(source[[:space:]]|\.[[:space:]])/) {
            value=line; sub(/^[[:space:]]*(source|\.)[[:space:]]+/,"",value)
            if(value ~ /^[^[:space:]"\047]+$/ || (substr(value,1,1)==sq && substr(value,length(value),1)==sq && gsub(sq,sq,value)==2) || (substr(value,1,1)=="\"" && substr(value,length(value),1)=="\"" && gsub(/\"/,"\"",value)==2)) next
            print; bad=1; next
        }
        if(line ~ /^[[:space:]]*eval[[:space:]]/) {
            if(line ~ /^[[:space:]]*eval[[:space:]]+"[$][(]pinset[[:space:]]+self[[:space:]]+shell([[:space:]]+(bash|zsh|fish))?[)]"[[:space:]]*$/) next
            print; bad=1; next
        }
        if(line ~ /^[[:space:]]*set[[:space:]]+(-[^[:space:]]+[[:space:]]+)*PINSET_/) next
        if(line ~ /^[[:space:]]*unset[[:space:]]+PINSET_[A-Za-z0-9_]+[[:space:]]*$/) next
        if(line ~ /^[[:space:]]*(export[[:space:]]+)?PATH[[:space:]]*=/) {
            original=line
            sub(/^[[:space:]]*(export[[:space:]]+)?PATH[[:space:]]*=[[:space:]]*/,"",line)
            quote=substr(line,1,1)
            if(quote==sq || quote=="\"") {
                rest=substr(line,2); end=index(rest,quote)
                component=substr(rest,1,end-1)
                valid=0
                for(i=1;i<=n;i++) if(r[i]!="" && (component==r[i] || component==r[i] "/bin" || component==r[i] "/v3/bin")) valid=1
                if(end && valid && substr(rest,end+1)==":\"$PATH\"") next
            }
            if(line ~ /^"\$\{?HOME\}?\/\.pinset\/(v3\/)?bin:\$\{?PATH\}?"[[:space:]]*$/) next
            print original; bad=1; next
        }
        if(line ~ /^\// && line !~ /[[:space:]]/) next
        print; bad=1
    }
    END {if(bad) exit 3}' "$1"
}
main() {
    [ -n "${HOME:-}" ] || fail 'HOME is required'
    os=$(uname -s)
    case "$os" in Darwin|Linux) ;; *) fail 'use uninstall.ps1 on Windows' ;; esac
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --plan) plan=1 ;;
            --yes) yes=1 ;;
            --pinset-home|--install-dir|--project)
                option=$1; shift; [ "$#" -gt 0 ] || fail "$option needs a path"
                case "$option" in
                    --pinset-home) homes="$homes$nl$1" ;;
                    --install-dir) installs="$installs$nl$1" ;;
                    --project) projects="$projects$nl$1" ;;
                esac ;;
            --help|-h) printf '%s\n' 'uninstall.sh [--plan] [--yes] [--pinset-home BASE] [--install-dir DIR] [--project DIR]' 'Repeat directory options as needed. Removes SDKs, state, caches, identities, shell entries and registered project data.'; return ;;
            *) fail "unknown option: $1" ;;
        esac
        shift
    done
    homes="$homes$nl$HOME/.pinset$nl${XDG_DATA_HOME:-$HOME/.local/share}/pinset"
    [ -z "${PINSET_HOME:-}" ] || homes="$homes$nl$PINSET_HOME"
    [ -z "${PINSET_INSTALL_DIR:-}" ] || installs="$installs$nl$PINSET_INSTALL_DIR"
    collect_profiles
    while IFS= read -r profile; do
        [ -n "$profile" ] || continue
        status=0; declarations=$(profile_homes "$profile") || status=$?
        [ "$status" -eq 0 ] || notice "cannot resolve some configured Pinset directories: $profile"
        while IFS='|' read -r name directory; do
            case "$name" in
                PINSET_HOME) homes="$homes$nl$directory" ;;
                PINSET_INSTALL_DIR) installs="$installs$nl$directory" ;;
            esac
        done <<EOF
$declarations
EOF
    done <<EOF
$profiles
EOF
    identity_metadata=0 seen_homes=''
    while IFS= read -r base; do
        [ -n "$base" ] || continue
        base=${base%/}; safe_path "$base"
        case "$seen_homes$nl" in *"$nl$base$nl"*) continue ;; esac
        seen_homes="$seen_homes$nl$base"; roots="$roots$nl$base/v3"
        [ -d "$base" ] || continue
        recognized=0
        if [ -e "$base/v3" ]; then
            safe_path "$base/v3"
            owned_v3 "$base/v3" || fail "unmarked Pinset 3 data: $base/v3"
            add_target D "$base/v3"; recognized=1; roots="$roots$nl$base/v3"
            registry="$base/v3/state/projects.json"
            if [ -f "$registry" ]; then
                safe_path "$registry"
                if discovered=$(registry_roots "$registry"); then projects="$projects$nl$discovered"
                else notice "cannot parse $registry; supply project roots with --project"; fi
            fi
        fi
        if legacy_owned "$base"; then
            recognized=1
            for name in bin shims installs cache downloads state config environments envs; do add_target D "$base/$name"; roots="$roots$nl$base/$name"; done
            for registry in "$base"/state/projects/*.toml; do
                [ -f "$registry" ] || continue
                safe_path "$registry"
                discovered=$(sed -n 's/^config[[:space:]]*=[[:space:]]*"\([^"\\]*\)"[[:space:]]*$/\1/p' "$registry")
                case "$discovered" in /*/pinset.toml) projects="$projects$nl${discovered%/pinset.toml}" ;; *) notice "cannot parse $registry; use --project" ;; esac
            done
        fi
        [ "$recognized" -eq 0 ] || add_target D "$base/migration-backups"
        for metadata in "$base/v3/state/identities.toml" "$base/state/identities.toml"; do
            if [ -f "$metadata" ] && grep -Eq '^id[[:space:]]*=' "$metadata"; then identity_metadata=1; fi
        done
    done <<EOF
$homes
EOF
    seen_projects=''
    while IFS= read -r project; do
        [ -n "$project" ] || continue
        project=${project%/}; safe_path "$project"
        case "$seen_projects$nl" in *"$nl$project$nl"*) continue ;; esac
        seen_projects="$seen_projects$nl$project"
        [ -d "$project" ] || continue
        if [ -e "$project/.pinset" ] || [ -L "$project/.pinset" ]; then
            safe_path "$project/.pinset"
            owned_project "$project" || fail "unmarked project data: $project/.pinset"
            add_target D "$project/.pinset"
        fi
        project_id=$(sed -n 's/^project_id[[:space:]]*=[[:space:]]*"\([^"\\]*\)"[[:space:]]*$/\1/p' "$project/.pinset/config.toml" 2>/dev/null || true)
        # Discover v3 and legacy named venvs without following directory links.
        for marker_name in .pinset-owner.toml .pinset-venv.toml; do
            found=$(find "$project" -type d -name .git -prune -o -type f -name "$marker_name" -print) || fail "cannot inventory $project"
            while IFS= read -r marker; do
                [ -n "$marker" ] || continue
                venv=${marker%/*}
                case "$venv" in "$project"|"$project/.git"|"$project/.git/"*) notice "unsafe virtual-environment marker retained: $marker"; continue ;; esac
                valid=0
                if [ -f "$venv/pyvenv.cfg" ] && [ ! -L "$venv/pyvenv.cfg" ]; then
                    marker_root=$(sed -n 's/^root[[:space:]]*=[[:space:]]*"\([^"\\]*\)"[[:space:]]*$/\1/p' "$marker")
                    if [ "$marker_name" = .pinset-owner.toml ]; then
                        marker_id=$(sed -n 's/^project_id[[:space:]]*=[[:space:]]*"\([^"\\]*\)"[[:space:]]*$/\1/p' "$marker")
                        if [ -n "$project_id" ] && [ "$marker_id" = "$project_id" ] && [ "$marker_root" = "$project" ] && grep -Eq '^protocol[[:space:]]*=[[:space:]]*"pinset/3"[[:space:]]*$' "$marker"; then valid=1; fi
                    elif [ "$marker_root" = "$project" ] && grep -Eq '^schema[[:space:]]*=[[:space:]]*[123][[:space:]]*$' "$marker"; then valid=1; fi
                fi
                if [ "$valid" -eq 1 ]; then
                    add_target D "$venv"; roots="$roots$nl$venv"
                else notice "unproven virtual environment retained: $venv"; fi
            done <<EOF
$found
EOF
        done
        if [ -f "$project/pinset.toml" ] && grep -Eq '^schema[[:space:]]*=[[:space:]]*[2345][[:space:]]*$' "$project/pinset.toml" && grep -Eq '^project-id[[:space:]]*=' "$project/pinset.toml"; then
            add_target F "$project/pinset.toml"; add_target F "$project/pinset.lock"
        fi
        for profile in "$project"/.env.*; do
            [ -f "$profile" ] && [ ! -L "$profile" ] || continue
            header=$(head -n 1 "$profile")
            case "$header" in '# pinset-encrypted-env v1'|'# pinset-encrypted-env v3') add_target F "$profile" ;; esac
        done
    done <<EOF
$projects
EOF
    old_ifs=$IFS; IFS=:
    for directory in ${PATH:-}; do [ -z "$directory" ] || installs="$installs$nl$directory"; done
    IFS=$old_ifs
    installs="$installs$nl$HOME/.local/bin$nl/usr/local/bin"
    while IFS= read -r directory; do
        [ -n "$directory" ] || continue
        case "$directory" in /*) ;; *) continue ;; esac
        for name in pinset pinset-shim; do
            binary="$directory/$name"
            [ -e "$binary" ] || [ -L "$binary" ] || continue
            case "$binary" in
                */Cellar/*|*/Homebrew/*|*/opt/pinset/bin/*) continue ;;
                */aquaproj-aqua/*|*/aqua/pkgs/*) notice "aqua package configuration requires manual removal: $binary"; continue ;;
            esac
            if [ -L "$directory" ]; then notice "linked binary directory retained: $directory"; continue; fi
            if [ "$name" = pinset-shim ] && [ -f "$binary" ]; then
                for candidate in "$directory"/*; do
                    [ -f "$candidate" ] || continue
                    if cmp -s "$binary" "$candidate"; then add_target F "$candidate"; fi
                done
            fi
            add_target F "$binary"
        done
    done <<EOF
$installs
EOF
    for file in "$HOME/.local/share/bash-completion/completions/pinset" \
        "${XDG_DATA_HOME:-$HOME/.local/share}/bash-completion/completions/pinset" \
        "${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions/pinset.fish" \
        "$HOME/.zfunc/_pinset" /usr/local/share/zsh/site-functions/_pinset \
        /usr/local/share/bash-completion/completions/pinset; do add_target F "$file"; done
    printf '%s\n' 'Full Pinset uninstall plan (SDKs and encryption identities included):'
    while IFS='|' read -r kind path; do [ -z "$kind" ] || printf '  remove %s\n' "$path"; done <<EOF
$targets
EOF
    seen_profiles=''
    while IFS= read -r profile; do
        [ -n "$profile" ] || continue
        case "$seen_profiles$nl" in *"$nl$profile$nl"*) continue ;; esac
        seen_profiles="$seen_profiles$nl$profile"
        if ! filter_profile "$profile" >/dev/null; then notice "complex settings need manual review: $profile"; fi
        if ! filter_profile "$profile" | cmp -s - "$profile"; then printf '  clean shell settings: %s\n' "$profile"; fi
    done <<EOF
$profiles
EOF
    brew_pinset=0
    if command -v brew >/dev/null 2>&1 && HOMEBREW_NO_AUTO_UPDATE=1 brew list --formula pinset >/dev/null 2>&1; then
        brew_pinset=1; printf '%s\n' '  brew uninstall --force pinset'
    fi
    printf '%s\n' '  remove credentials: dev.pinset.identity.v3 and dev.pinset.identity'
    if [ "$os" = Linux ] && [ "$identity_metadata" -eq 1 ] && ! command -v secret-tool >/dev/null 2>&1; then
        fail 'native identities exist; install libsecret secret-tool before uninstalling (no files changed)'
    fi
    if [ "$plan" -eq 1 ]; then printf '%s\n' 'Plan only; no changes made.'; return "$incomplete"; fi
    if [ "$yes" -ne 1 ]; then
        [ -r /dev/tty ] || fail 'use --yes for noninteractive execution'
        printf 'Delete listed Pinset data and identities? [y/N] ' >/dev/tty
        IFS= read -r answer </dev/tty || exit 1
        case "$answer" in y|Y) ;; *) return ;; esac
    fi
    # Credentials first, so a locked store does not lose its metadata inventory.
    if [ "$os" = Darwin ]; then
        for service in dev.pinset.identity.v3 dev.pinset.identity; do
            while :; do
                status=0; security find-generic-password -s "$service" >/dev/null 2>&1 || status=$?
                if [ "$status" -eq 44 ]; then break; fi
                [ "$status" -eq 0 ] || fail "Keychain lookup failed (status $status): $service"
                security delete-generic-password -s "$service" >/dev/null || fail "Keychain deletion failed: $service"
            done
        done
    elif command -v secret-tool >/dev/null 2>&1; then
        for service in dev.pinset.identity.v3 dev.pinset.identity; do
            status=0; diagnostic=$(secret-tool clear service "$service" 2>&1) || status=$?
            [ "$status" -eq 0 ] || { [ "$status" -eq 1 ] && [ -z "$diagnostic" ]; } || fail "Secret Service deletion failed: $service"
        done
    fi
    if [ "$brew_pinset" -eq 1 ]; then HOMEBREW_NO_AUTO_UPDATE=1 brew uninstall --force pinset || fail 'Homebrew uninstall failed'; fi
    while IFS= read -r profile; do
        [ -n "$profile" ] || continue
        if filter_profile "$profile" | cmp -s - "$profile"; then continue; fi
        if [ -L "$profile" ]; then notice "linked profile retained: $profile"; continue; fi
        directory=${profile%/*}
        if [ ! -w "$profile" ] || [ ! -w "$directory" ]; then notice "permission denied cleaning $profile"; continue; fi
        temporary=$(mktemp "$directory/.pinset-uninstall.XXXXXX") || fail "cannot stage $profile"
        status=0; filter_profile "$profile" >"$temporary" || status=$?
        cat "$temporary" >"$profile" || { rm -f -- "$temporary"; fail "cannot write $profile"; }
        rm -f -- "$temporary"
        [ "$status" -eq 0 ] || notice "some settings retained for review: $profile"
    done <<EOF
$seen_profiles
EOF
    if command -v fish >/dev/null 2>&1; then
        PINSET_UNINSTALL_ROOTS=$roots fish --no-config -c '
            set -l clean
            for entry in $fish_user_paths
                set -l owned 0
                for root in (string split \n -- $PINSET_UNINSTALL_ROOTS)
                    if test -n "$root"; and string match -q -- "$root/*" "$entry"; set owned 1; end
                end
                if test $owned -eq 0; set -a clean "$entry"; end
            end
            set -U fish_user_paths $clean
            for name in (set --names --universal)
                if string match -q "PINSET_*" "$name"; set --erase --universal $name; end
            end
        ' || notice 'could not clean fish universal variables'
    fi
    while IFS='|' read -r kind path; do
        [ -n "$kind" ] || continue
        case "$kind" in
            D) safe_path "$path"; rm -rf -- "$path" || fail "cannot remove $path" ;;
            F) rm -f -- "$path" || fail "cannot remove $path" ;;
        esac
    done <<EOF
$targets
EOF
    while IFS= read -r base; do
        [ -n "$base" ] || continue
        [ ! -d "$base" ] || rmdir -- "$base" 2>/dev/null || notice "unproven files remain in $base"
    done <<EOF
$seen_homes
EOF
    printf '%s\n' 'Pinset cleanup finished. Open a new terminal to discard inherited environment variables.'
    [ "$incomplete" -eq 0 ] || printf '%s\n' 'Some items require manual review; see messages above.' >&2
    return "$incomplete"
}
# Keep the only invocation last, including for curl | sh.
main "$@"
