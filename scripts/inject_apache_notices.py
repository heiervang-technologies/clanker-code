#!/usr/bin/env python3
import subprocess
import os
import sys
import argparse

NOTICE_TEXT = "Modified by Heiervang Technologies."

def get_comment_syntax(filepath):
    ext = os.path.splitext(filepath)[1].lower()
    if ext in ['.rs', '.ts', '.js', '.c', '.cpp', '.h', '.hpp', '.java', '.go', '.jsonc']:
        return f"// {NOTICE_TEXT}"
    elif ext in ['.py', '.sh', '.bash', '.yaml', '.yml', '.toml', '.mk']:
        return f"# {NOTICE_TEXT}"
    elif ext in ['.html', '.xml']:
        return f"<!-- {NOTICE_TEXT} -->"
    elif ext in ['.css']:
        return f"/* {NOTICE_TEXT} */"
    else:
        return None

def main():
    parser = argparse.ArgumentParser(description="Inject Apache 2.0 modification notices into files diverging from upstream")
    parser.add_argument("--base", default="origin/main", help="Base commit/branch to compare against (upstream)")
    parser.add_argument("--target", default="HEAD", help="Target commit/branch (downstream fork)")
    parser.add_argument("--dry-run", action="store_true", help="Print files that would be modified without modifying them")
    args = parser.parse_args()

    cmd = ["git", "diff-tree", "--name-status", "-r", args.base, args.target]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
    except subprocess.CalledProcessError as e:
        print(f"Error running git diff-tree: {e}", file=sys.stderr)
        sys.exit(1)
    
    modified_files = []
    for line in result.stdout.splitlines():
        if not line:
            continue
        parts = line.split('\t')
        status = parts[0]
        
        if status.startswith('R') or status.startswith('C'):
            filepath = parts[2]
        elif status.startswith('D'):
            continue
        else:
            filepath = parts[-1]
            
        modified_files.append(filepath)

    count = 0
    for filepath in modified_files:
        if not os.path.isfile(filepath):
            continue
            
        comment_line = get_comment_syntax(filepath)
        if not comment_line:
            continue
            
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                lines = f.readlines()
        except UnicodeDecodeError:
            # Skip binary or non-utf8 files
            continue
            
        has_notice = any(NOTICE_TEXT in line for line in lines)
        if has_notice:
            continue
            
        insert_idx = 0
        if lines and lines[0].startswith('#!'):
            insert_idx = 1
            
        lines.insert(insert_idx, comment_line + "\n")
        
        if args.dry_run:
            print(f"Would inject notice into {filepath}")
        else:
            with open(filepath, 'w', encoding='utf-8') as f:
                f.writelines(lines)
            print(f"Injected notice into {filepath}")
        count += 1
        
    print(f"\nTotal files {'to be ' if args.dry_run else ''}modified: {count}")

if __name__ == "__main__":
    main()
