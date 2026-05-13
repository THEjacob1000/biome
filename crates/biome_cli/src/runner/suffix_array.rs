use crate::runner::token_store::TokenizedFile;

/// Represents a duplicate code match found by the suffix array algorithm.
#[derive(Clone, Debug)]
pub struct DuplicateMatch {
    pub file1: String,
    pub start_offset1: usize,
    pub end_offset1: usize,
    pub file2: String,
    pub start_offset2: usize,
    pub end_offset2: usize,
    pub token_count: usize,
}

/// Build a suffix array from a sequence of tokens.
/// Returns a vector of (suffix_start_index, original_position) tuples, sorted lexicographically.
fn build_suffix_array(tokens: &[String]) -> Vec<usize> {
    let mut sa: Vec<usize> = (0..tokens.len()).collect();
    sa.sort_by(|&a, &b| {
        tokens[a..].cmp(&tokens[b..])
    });
    sa
}

/// Build the Longest Common Prefix (LCP) array from a suffix array.
fn build_lcp_array(tokens: &[String], sa: &[usize]) -> Vec<usize> {
    let n = sa.len();
    let mut lcp = vec![0; n];
    let mut rank = vec![0; n];
    let mut tmp = vec![0; n];

    for i in 0..n {
        rank[sa[i]] = i;
    }

    let mut h = 0;
    for i in 0..n {
        if rank[i] > 0 {
            let j = sa[rank[i] - 1];
            while i + h < n && j + h < n && tokens[i + h] == tokens[j + h] {
                h += 1;
            }
            lcp[rank[i]] = h;
            if h > 0 {
                h -= 1;
            }
        }
    }

    lcp
}

/// Find duplicate code blocks across multiple files using a suffix array algorithm.
/// Returns a vector of DuplicateMatch instances for blocks of at least `threshold` tokens.
pub fn find_duplicates(files: Vec<TokenizedFile>, threshold: usize) -> Vec<DuplicateMatch> {
    let mut duplicates = vec![];

    // Concatenate all tokens from all files with sentinel values
    let mut all_tokens = vec![];
    let mut file_boundaries = vec![];

    for file in &files {
        file_boundaries.push((file.path.clone(), all_tokens.len(), file.tokens.clone()));
        all_tokens.extend(file.tokens.iter().cloned());
        all_tokens.push(format!("__SENTINEL__{}", file.path));
    }

    if all_tokens.len() < threshold {
        return duplicates;
    }

    let sa = build_suffix_array(&all_tokens);
    let lcp = build_lcp_array(&all_tokens, &sa);

    // Find duplicates by examining consecutive suffix array entries
    for i in 0..lcp.len() - 1 {
        if lcp[i + 1] >= threshold {
            let pos1 = sa[i];
            let pos2 = sa[i + 1];

            // Determine which files these positions belong to
            let (file1_info, file1_tokens) = file_boundaries.iter()
                .find(|(_, start, _)| *start <= pos1)
                .and_then(|(name, start, tokens)| {
                    if pos1 < start + tokens.len() {
                        Some(((name.clone(), *start), tokens.clone()))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| {
                    (("unknown".to_string(), 0), vec![])
                });

            let (file2_info, file2_tokens) = file_boundaries.iter()
                .find(|(_, start, _)| *start <= pos2)
                .and_then(|(name, start, tokens)| {
                    if pos2 < start + tokens.len() {
                        Some(((name.clone(), *start), tokens.clone()))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| {
                    (("unknown".to_string(), 0), vec![])
                });

            let (file1_name, file1_start) = file1_info;
            let (file2_name, file2_start) = file2_info;

            // Skip if both duplicates are in the same file at the same location
            if file1_name == file2_name && pos1 == pos2 {
                continue;
            }

            let offset1 = pos1 - file1_start;
            let offset2 = pos2 - file2_start;

            // Get byte offsets from the files
            let file1_obj = files.iter().find(|f| f.path == file1_name);
            let file2_obj = files.iter().find(|f| f.path == file2_name);

            if let (Some(f1), Some(f2)) = (file1_obj, file2_obj) {
                let start_offset1 = f1.byte_offsets.get(offset1).copied().unwrap_or(0);
                let end_offset1 = if offset1 + lcp[i + 1] < f1.byte_offsets.len() {
                    f1.byte_offsets.get(offset1 + lcp[i + 1]).copied().unwrap_or(start_offset1)
                } else {
                    start_offset1
                };

                let start_offset2 = f2.byte_offsets.get(offset2).copied().unwrap_or(0);
                let end_offset2 = if offset2 + lcp[i + 1] < f2.byte_offsets.len() {
                    f2.byte_offsets.get(offset2 + lcp[i + 1]).copied().unwrap_or(start_offset2)
                } else {
                    start_offset2
                };

                duplicates.push(DuplicateMatch {
                    file1: file1_name.clone(),
                    start_offset1,
                    end_offset1,
                    file2: file2_name.clone(),
                    start_offset2,
                    end_offset2,
                    token_count: lcp[i + 1],
                });
            }
        }
    }

    duplicates
}
