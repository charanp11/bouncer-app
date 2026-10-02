//! Line diff shared by the settings-file preview and the island's code view.

/// `old` → `new` as line operations: `' '` kept, `'-'` removed, `'+'` added.
pub fn line_ops<'a>(old: &'a str, new: &'a str) -> Vec<(char, &'a str)> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // ponytail: O(n·m) LCS table; fine for settings files and single edits of a
    // few thousand lines. Myers' diff if huge edits ever show up.
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            ops.push((' ', a[i]));
            (i, j) = (i + 1, j + 1);
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            ops.push(('-', a[i]));
            i += 1;
        } else {
            ops.push(('+', b[j]));
            j += 1;
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_removes_and_adds_lines() {
        assert_eq!(
            line_ops("a\nb\nc", "a\nB\nc\nd"),
            [(' ', "a"), ('-', "b"), ('+', "B"), (' ', "c"), ('+', "d")]
        );
        assert_eq!(line_ops("", "x"), [('+', "x")]);
        assert_eq!(line_ops("x", ""), [('-', "x")]);
    }
}
