#!/usr/bin/env bash
# Read-only worktree prune audit. Classifies every git worktree by size, merge
# state, uncommitted work, remote/PR state, and the most recent chat that
# operated in it. Emits a table sorted by size with a suggested bucket. Never
# deletes anything; deletion stays a human-gated step in the playbook.
#
# Usage: worktree-audit.sh [-b|--base <ref>] [repo-path]
#   repo-path defaults to the current repo.
#   -b sets the ref the MERGED column checks against (e.g. origin/integration).
#   Without it the base is origin/HEAD, then origin/main, then origin/master.
#   origin/HEAD is the remote's default branch, not necessarily the branch PRs
#   merge into; pass -b when they differ.
set -uo pipefail

base=""
while [ $# -gt 0 ]; do
	case "$1" in
	-b|--base) [ $# -ge 2 ] || { echo "error: $1 needs a ref" >&2; exit 2; }; base="$2"; shift 2 ;;
	-h|--help) sed -n '2,/^set -u/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
	-*) echo "error: unknown option: $1" >&2; exit 2 ;;
	*) break ;;
	esac
done

command -v jq >/dev/null 2>&1 || {
	echo "error: jq is required to build the PR inventory" >&2
	exit 1
}

repo="${1:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -z "$repo" ] && { echo "not in a git repo; pass a repo path" >&2; exit 1; }
cd "$repo" || exit 1
git rev-parse --git-dir >/dev/null 2>&1 || { echo "error: not a git repo: $repo" >&2; exit 1; }

# Git's NUL-delimited porcelain preserves spaces and newlines in worktree paths.
# The main worktree is listed first; everything else is a candidate.
worktrees=()
main_wt=""
while IFS= read -r -d '' record; do
	case "$record" in
	"worktree "*)
		wt_path="${record#worktree }"
		[ -n "$main_wt" ] || main_wt="$wt_path"
		worktrees+=("$wt_path")
		;;
	esac
done < <(git worktree list --porcelain -z)

if [ -z "$base" ]; then
	for cand in "$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null)" origin/main origin/master; do
		[ -n "$cand" ] && git rev-parse --verify --quiet "$cand^{commit}" >/dev/null && { base="$cand"; break; }
	done
	[ -z "$base" ] && { echo "error: no base ref (origin/HEAD, origin/main, origin/master all missing); pass -b <ref>" >&2; exit 1; }
	echo "warn: no -b given; MERGED checks against $base" >&2
fi

# The base drives the merge check. The fetch is best-effort (stale is fine for
# a first pass) and runs before the check, since -b may name a branch not
# fetched yet. The explicit refspec creates it even in a single-branch clone.
case "$base" in
origin/*) git fetch origin "+refs/heads/${base#origin/}:refs/remotes/$base" --quiet 2>/dev/null \
	|| echo "warn: could not fetch $base; merged column may be stale" >&2 ;;
esac
git rev-parse --verify --quiet "$base^{commit}" >/dev/null 2>&1 || { echo "error: base ref not found: $base" >&2; exit 1; }

# PR state by branch, fetched once as [{number, state, head}] with a lowercase
# state. Origin when its CLI can read this repo's PRs, otherwise gh. Empty when
# neither can.
prs=$(mktemp)
if command -v origin >/dev/null 2>&1 \
	&& raw=$(origin pr list --author "@me" --state all --limit 1000 \
		--json number,status,headRef 2>/dev/null); then
	if ! printf '%s' "$raw" | jq --slurp -e '
		if length == 1 and (.[0] | type) == "array" and all(.[0][];
			. as $pr
			| ($pr.number | type) == "number"
			and $pr.number > 0
			and $pr.number == ($pr.number | floor)
			and ($pr.status | type) == "string"
			and ($pr.headRef | type) == "string"
			and ($pr.headRef | length) > 0
			and (["open", "draft", "merged", "closed"] | index($pr.status | ascii_downcase)) != null
		)
		then .[0] | map({number, state: (.status | ascii_downcase), head: .headRef})
		else error("invalid Origin PR inventory")
		end
	' > "$prs" 2>/dev/null; then
		echo "error: could not parse the Origin PR inventory" >&2
		rm -f "$prs"
		exit 1
	fi
elif raw=$(gh pr list --author "@me" --state all --limit 1000 \
	--json number,state,headRefName 2>/dev/null); then
	if ! printf '%s' "$raw" | jq --slurp -e '
		if length == 1 and (.[0] | type) == "array" and all(.[0][];
			. as $pr
			| ($pr.number | type) == "number"
			and $pr.number > 0
			and $pr.number == ($pr.number | floor)
			and ($pr.state | type) == "string"
			and ($pr.headRefName | type) == "string"
			and ($pr.headRefName | length) > 0
			and (["open", "merged", "closed"] | index($pr.state | ascii_downcase)) != null
		)
		then .[0] | map({number, state: (.state | ascii_downcase), head: .headRefName})
		else error("invalid GitHub PR inventory")
		end
	' > "$prs" 2>/dev/null; then
		echo "error: could not parse the GitHub PR inventory" >&2
		rm -f "$prs"
		exit 1
	fi
else
	# No forge could provide PR state; preserve the documented best-effort empty inventory.
	echo "[]" > "$prs"
fi

# Transcripts dir: ~/.cursor/projects/<slugified-repo-path>/agent-transcripts.
slug=$(printf '%s' "$main_wt" | sed 's#^/##; s#/#-#g')
transcripts="$HOME/.cursor/projects/$slug/agent-transcripts"
now=$(date +%s)

echo "# base: $base"
printf "SIZE\tAGE\tMERGED\tDIRTY\tREMOTE\tPR\tLAST_CHAT\tBUCKET\tDATA\tWORKTREE\n"

if ! {
	for wt in "${worktrees[@]}"; do
	[ "$wt" = "$main_wt" ] && continue

	size=$(cd "$wt" && du -sh . 2>/dev/null | awk '{print $1}')
	head=$(git -C "$wt" rev-parse HEAD 2>/dev/null)
	head_ts=$(git -C "$wt" log -1 --format='%ct' HEAD 2>/dev/null || echo 0)
	age=$([ "$head_ts" -gt 0 ] 2>/dev/null && echo "$(( (now - head_ts) / 86400 ))d" || echo "?")

	# Squash-merged branches are not ancestors of the base, so PR state is the
	# real signal; merge-base only catches fast-forward/rebase merges.
	git merge-base --is-ancestor "$head" "$base" 2>/dev/null && merged=YES || merged=no

	# Enumerate untracked and ignored files as well as tracked edits. Default
	# porcelain omits ignored paths, which --force worktree removal deletes.
	if ! status_file=$(mktemp); then
		echo "error: could not create worktree status file" >&2
		rm -f "$prs"
		exit 1
	fi
	if ! git -C "$wt" status --porcelain=v1 -z --untracked-files=all --ignored=traditional > "$status_file" 2>/dev/null; then
		echo "error: could not read worktree status" >&2
		rm -f "$status_file" "$prs"
		exit 1
	fi
	if ! data_file=$(mktemp); then
		echo "error: could not create worktree data file" >&2
		rm -f "$status_file" "$prs"
		exit 1
	fi
	tracked_count=0; scratch_count=0; ignored_count=0; submodule_count=0
	while IFS= read -r -d '' status_row; do
		status_code="${status_row:0:2}"
		status_path="${status_row:3}"
		case "$status_code" in
		"??")
			scratch_count=$((scratch_count + 1))
			if ! printf 'untracked:%q\n' "$status_path" >> "$data_file"; then
				echo "error: could not record untracked worktree path" >&2
				rm -f "$status_file" "$data_file" "$prs"
				exit 1
			fi
			;;
		"!!")
			ignored_count=$((ignored_count + 1))
			if ! printf 'ignored:%q\n' "$status_path" >> "$data_file"; then
				echo "error: could not record ignored worktree path" >&2
				rm -f "$status_file" "$data_file" "$prs"
				exit 1
			fi
			;;
		*)
			tracked_count=$((tracked_count + 1))
			# Porcelain v1 -z emits a second NUL-delimited path for renames/copies.
			if [[ "$status_code" == *R* || "$status_code" == *C* ]]; then
				if ! IFS= read -r -d '' original_path; then
					echo "error: incomplete rename record in worktree status" >&2
					rm -f "$status_file" "$prs"
					exit 1
				fi
			fi
			;;
		esac
	done < "$status_file"
	rm -f "$status_file"
	if ! index_file=$(mktemp); then
		echo "error: could not create worktree index file" >&2
		rm -f "$data_file" "$prs"
		exit 1
	fi
	if ! git -C "$wt" ls-files --stage -z > "$index_file" 2>/dev/null; then
		echo "error: could not read worktree index" >&2
		rm -f "$index_file" "$data_file" "$prs"
		exit 1
	fi
	while IFS= read -r -d '' index_record; do
		index_header="${index_record%%$'\t'*}"
		if [[ "$index_header" == "160000 "* ]]; then
			index_path="${index_record#*$'\t'}"
			submodule_count=$((submodule_count + 1))
			if ! printf 'submodule:%q\n' "$index_path" >> "$data_file"; then
				echo "error: could not record submodule path" >&2
				rm -f "$index_file" "$data_file" "$prs"
				exit 1
			fi
		fi
	done < "$index_file"
	rm -f "$index_file"
	dirty=""
	if [ "$tracked_count" -gt 0 ]; then dirty="wip:$tracked_count"; fi
	if [ "$scratch_count" -gt 0 ]; then dirty="${dirty:+$dirty,}scratch:$scratch_count"; fi
	if [ "$ignored_count" -gt 0 ]; then dirty="${dirty:+$dirty,}ignored:$ignored_count"; fi
	if [ "$submodule_count" -gt 0 ]; then dirty="${dirty:+$dirty,}submodule:$submodule_count"; fi
	[ -n "$dirty" ] || dirty=clean
	# Paths are shell-escaped one per line before JSON encoding, so tabs and
	# newlines stay intact without passing an unbounded path list in argv.
	if ! data_json=$(jq -Rsc 'split("\n") | map(select(length > 0))' < "$data_file" 2>/dev/null); then
		echo "error: could not encode worktree data paths" >&2
		rm -f "$data_file" "$prs"
		exit 1
	fi
	rm -f "$data_file"

	branch=$(git -C "$wt" symbolic-ref --quiet --short HEAD 2>/dev/null || echo "")
	if [ -z "$branch" ]; then remote=detached
	elif git -C "$wt" show-ref --verify --quiet "refs/remotes/origin/$branch"; then
		[ "$(git -C "$wt" rev-parse "origin/$branch" 2>/dev/null)" = "$head" ] \
			&& remote=pushed \
			|| remote="ahead$(git -C "$wt" rev-list --count "origin/$branch..HEAD" 2>/dev/null)"
	else remote=no-remote; fi

	pr=$([ -n "$branch" ] && jq -r --arg b "$branch" \
		'.[] | select(.head==$b) | "#\(.number)/\(.state)"' "$prs" 2>/dev/null | head -1)
	[ -z "$pr" ] && pr="-"

	# Most recent chat whose transcript operated in this worktree. Match path
	# followed by "/" or a quote so glint-482 does not match glint-482-r37.
	last="-"; last_ts=0
	if [ -d "$transcripts" ]; then
		if ! wt_json=$(jq -Rn --arg path "$wt" '$path' 2>/dev/null); then
			echo "error: could not encode worktree path for transcript search" >&2
			rm -f "$prs"
			exit 1
		fi
		wt_json_prefix=${wt_json%\"}
		if ! matches=$(mktemp); then
			echo "error: could not create recent-chat search results file" >&2
			rm -f "$prs"
			exit 1
		fi
		if rg --fixed-strings --multiline --files-with-matches --null \
			-e "${wt}/" -e "${wt}\"" \
			-e "${wt_json_prefix}/" -e "$wt_json" \
			"$transcripts" > "$matches" 2>/dev/null; then
			while IFS= read -r -d '' chat_file; do
				chat_ts=$(stat -f '%m' "$chat_file" 2>/dev/null || true)
				case "$chat_ts" in ''|*[!0-9]*) chat_ts=$(stat -c '%Y' "$chat_file" 2>/dev/null || true) ;; esac
				case "$chat_ts" in ''|*[!0-9]*)
					echo "error: could not read transcript timestamp for worktree audit" >&2
					rm -f "$matches" "$prs"
					exit 1
					;;
				esac
				[ "$chat_ts" -gt "$last_ts" ] && last_ts="$chat_ts"
			done < "$matches"
		else
			search_status=$?
			if [ "$search_status" -ne 1 ]; then
				echo "error: could not search recent-chat transcripts for worktree" >&2
				rm -f "$matches" "$prs"
				exit 1
			fi
		fi
		rm -f "$matches"
		if [ "$last_ts" -gt 0 ]; then
			last=$(date -r "$last_ts" '+%Y-%m-%d' 2>/dev/null || date -d "@$last_ts" '+%Y-%m-%d' 2>/dev/null || echo "-")
		fi
	fi
	recent=$([ "$last_ts" -gt 0 ] 2>/dev/null && [ $(( (now - last_ts) / 86400 )) -le 4 ] && echo yes || echo no)

	case "$dirty" in wip:*) bucket=hold-wip ;; *scratch:*|*ignored:*) bucket=hold-untracked-data ;; *submodule:*) bucket=hold-submodule-data ;; *)
		case "$pr" in */open|*/draft) bucket=hold-open-pr ;; *)
			if [ "$recent" = yes ]; then bucket=verify-recent-chat
			elif [ "$merged" = YES ] || [ "$pr" != "-" ]; then bucket=safe
			else bucket=review; fi ;;
		esac ;;
	esac

	# Shell-escape the path so tabs and newlines cannot split table rows or sort records.
	display_wt=$(printf '%q' "$wt")
	printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
		"$size" "$age" "$merged" "$dirty" "$remote" "$pr" "$last" "$bucket" "$data_json" "$display_wt"
	done
} | sort -t$'\t' -k1,1 -rh; then
	rm -f "$prs"
	exit 1
fi

rm -f "$prs"
