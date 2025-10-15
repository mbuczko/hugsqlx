#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SqlBlock {
    Literal(String),
    Conditional(String, String),
}

const BLOCK_OPEN: &[char] = &['-', '-', '~', '{'];
const BLOCK_CLOSE: &[char] = &['-', '-', '~', '}'];

fn is_at_newline_or_start(input: &[char], pos: usize) -> bool {
    pos == 0 || (pos > 0 && input[pos - 1] == '\n')
}

fn matches_pattern(input: &[char], start: usize, pattern: &[char]) -> bool {
    start + pattern.len() <= input.len() && input[start..start + pattern.len()] == pattern[..]
}

fn trim_slice(chars: &[char]) -> &[char] {
    let start = chars
        .iter()
        .position(|c| !c.is_whitespace())
        .unwrap_or(chars.len());
    let end = chars
        .iter()
        .rposition(|c| !c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);

    if start >= end {
        &[]
    } else {
        &chars[start..end]
    }
}

fn parse_conditional_block(input: &[char], start: usize) -> Option<(String, String, usize)> {
    let mut i = start + 4; // Skip "--~{"

    // Extract condition identifier
    let id_start = i;
    while i < input.len() && input[i] != '\n' {
        i += 1;
    }

    if i >= input.len() {
        return None;
    }

    let condition_id: String = trim_slice(&input[id_start..i]).iter().collect();
    let content_start = i;

    // Find the closing "--~}"
    while i < input.len() {
        if is_at_newline_or_start(input, i) && matches_pattern(input, i, BLOCK_CLOSE) {
            let content = trim_slice(&input[content_start..i]);

            // Move past the closing tag and its newline
            i += 4; // "--~}"
            while i < input.len() && input[i] != '\n' {
                i += 1;
            }
            return Some((condition_id, content.iter().collect(), i));
        }
        i += 1;
    }

    None
}

pub(crate) fn parse_sql_blocks(input: &[char]) -> Vec<SqlBlock> {
    let mut result = Vec::with_capacity(3);
    let mut i = 0;
    let mut literal_start = 0;
    while i < input.len() {
        if is_at_newline_or_start(input, i) && matches_pattern(input, i, BLOCK_OPEN) {
            if i > literal_start {
                let literal = trim_slice(&input[literal_start..i]);
                if !literal.is_empty() {
                    result.push(SqlBlock::Literal(literal.iter().collect()));
                }
            }
            if let Some((condition_id, content, end_pos)) = parse_conditional_block(input, i) {
                result.push(SqlBlock::Conditional(condition_id, content));
                i = end_pos;
                literal_start = i;
                continue;
            }
        }
        i += 1;
    }

    // Add any remaining literal content
    if literal_start < input.len() {
        let literal = trim_slice(&input[literal_start..input.len()]);
        if !literal.is_empty() {
            result.push(SqlBlock::Literal(literal.iter().collect()));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_conditional_blocks() {
        let input = r#"
SELECT foo FROM bar
WHERE a=1
--~{ need_contain 
AND b IN (...)        
--~}
--~{ musnt_contain
AND b NOT IN (...)        
--~}
ORDER BY BAZZ"#;

        let chars: Vec<char> = input.chars().collect();
        let result = parse_sql_blocks(&chars);

        assert_eq!(result.len(), 4);
        assert_eq!(
            result[0],
            SqlBlock::Literal("SELECT foo FROM bar\nWHERE a=1".to_string())
        );
        assert_eq!(
            result[1],
            SqlBlock::Conditional("need_contain".to_string(), "AND b IN (...)".to_string())
        );
        assert_eq!(
            result[2],
            SqlBlock::Conditional(
                "musnt_contain".to_string(),
                "AND b NOT IN (...)".to_string()
            )
        );
        assert_eq!(result[3], SqlBlock::Literal("ORDER BY BAZZ".to_string()));
    }

    #[test]
    fn test_no_conditionals() {
        let input = "SELECT * FROM users WHERE id = 1";
        let chars: Vec<char> = input.chars().collect();
        let result = parse_sql_blocks(&chars);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            SqlBlock::Literal("SELECT * FROM users WHERE id = 1".to_string())
        );
    }

    #[test]
    fn test_only_conditional() {
        let input = r#"--~{ test
SELECT 1
--~}"#;

        let chars: Vec<char> = input.chars().collect();
        let result = parse_sql_blocks(&chars);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            SqlBlock::Conditional("test".to_string(), "SELECT 1".to_string())
        );
    }
}
