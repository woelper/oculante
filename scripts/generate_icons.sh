cd $(git rev-parse --show-toplevel)

# Keep up to date, pinned for reproducibility sake
npm install fantasticon@4.1.0
npx fantasticon ./res/ui_icons/ -o ./res/fonts/ -t ttf -g css --name ui_icons

# Generates src/icons.rs from the CSS that fantasticon outputs.
set -euo pipefail
 
CSS="${1:-res/fonts/ui_icons.css}"
OUT="${2:-src/icons.rs}"
 
[ -f "$CSS" ] || { echo "CSS file not found: $CSS" >&2; exit 1; }
mkdir -p "$(dirname "$OUT")"
 
# 1. Pull "<name>\t<hex codepoint>" pairs out of the CSS
# 2. Sort by name so the 0001_... prefix puts them in order (fantasticon's order is reversed)
# 3. Strip the numeric prefix, uppercase, and emit a Rust const
awk '
  /^\.icon-[0-9]+_/ {
    name = $0
    sub(/^\.icon-/, "", name)
    sub(/:before.*/, "", name)
    next
  }
  name != "" && /content:/ {
    code = $0
    sub(/.*"\\/, "", code)
    sub(/".*/, "", code)
    print name "\t" code
    name = ""
  }
' "$CSS" \
  | sort -t "$(printf '\t')" -k1,1 \
  | awk -F '\t' '
      {
        n = $1
        sub(/^[0-9]+_/, "", n)
        printf "pub const %s: &str = \"\\u{%s}\";\n", toupper(n), $2
      }
    ' > "$OUT"
 
# Rust won't compile duplicate const names, so warn early
dupes=$(sed -E 's/^pub const ([A-Z0-9_]+):.*/\1/' "$OUT" | sort | uniq -d)
if [ -n "$dupes" ]; then
  echo "Warning: duplicate const names after stripping prefixes:" >&2
  echo "$dupes" >&2
fi
 
echo "Wrote $(wc -l < "$OUT") constants to $OUT"
