/// Extracts (owner, repo, pr_number) from a GitHub PR URL.
///
/// Accepts URLs like:
/// - https://github.com/owner/repo/pull/123
/// - https://github.com/owner/repo/pull/123/files
/// - https://github.com/owner/repo/pull/123.diff
/// - https://github.com/owner/repo/pull/123.patch
pub fn parse_pr_url(url: &str) -> Option<(&str, &str, &str)> {
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;

    let mut parts = path.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    let pull = parts.next().filter(|&s| s == "pull")?;
    let _ = pull;
    let number_part = parts.next().filter(|s| !s.is_empty())?;
    // Strip query strings and fragments (e.g., ?tab=files, #discussion_r123)
    let number_part = number_part.split(['?', '#']).next().unwrap_or(number_part);
    let number = number_part
        .strip_suffix(".diff")
        .or_else(|| number_part.strip_suffix(".patch"))
        .unwrap_or(number_part);
    if !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((owner, repo, number))
}

pub async fn fetch_diff(url: &str) -> Result<String, String> {
    let (owner, repo, number) =
        parse_pr_url(url).ok_or_else(|| format!("Not a valid GitHub PR URL: {url}"))?;

    let diff_url = format!("https://github.com/{owner}/{repo}/pull/{number}.diff");

    eprintln!("Fetching diff from {diff_url}...");

    let client = reqwest::Client::new();
    let response = client
        .get(&diff_url)
        .header("Accept", "text/plain")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch diff: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned HTTP {}: {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if text.trim().is_empty() {
        return Err("PR diff is empty".to_string());
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_pr_url() {
        let result = parse_pr_url("https://github.com/metabase/metabase/pull/70213");
        assert_eq!(result, Some(("metabase", "metabase", "70213")));
    }

    #[test]
    fn pr_url_with_trailing_slash() {
        let result = parse_pr_url("https://github.com/owner/repo/pull/123/");
        assert_eq!(result, Some(("owner", "repo", "123")));
    }

    #[test]
    fn pr_url_with_files_tab() {
        let result = parse_pr_url("https://github.com/owner/repo/pull/456/files");
        assert_eq!(result, Some(("owner", "repo", "456")));
    }

    #[test]
    fn pr_url_with_commits_tab() {
        let result = parse_pr_url("https://github.com/owner/repo/pull/789/commits");
        assert_eq!(result, Some(("owner", "repo", "789")));
    }

    #[test]
    fn diff_url() {
        let result = parse_pr_url("https://github.com/owner/repo/pull/123.diff");
        assert_eq!(result, Some(("owner", "repo", "123")));
    }

    #[test]
    fn patch_url() {
        let result = parse_pr_url("https://github.com/owner/repo/pull/123.patch");
        assert_eq!(result, Some(("owner", "repo", "123")));
    }

    #[test]
    fn pr_url_with_query_string() {
        let result = parse_pr_url("https://github.com/owner/repo/pull/123?tab=files");
        assert_eq!(result, Some(("owner", "repo", "123")));
    }

    #[test]
    fn pr_url_with_fragment() {
        let result = parse_pr_url("https://github.com/owner/repo/pull/123#discussion_r12345");
        assert_eq!(result, Some(("owner", "repo", "123")));
    }

    #[test]
    fn http_url() {
        let result = parse_pr_url("http://github.com/owner/repo/pull/123");
        assert_eq!(result, Some(("owner", "repo", "123")));
    }

    #[test]
    fn reject_non_github_url() {
        assert_eq!(parse_pr_url("https://gitlab.com/owner/repo/pull/123"), None);
    }

    #[test]
    fn reject_non_pr_github_url() {
        assert_eq!(parse_pr_url("https://github.com/owner/repo/issues/123"), None);
    }

    #[test]
    fn reject_missing_pr_number() {
        assert_eq!(parse_pr_url("https://github.com/owner/repo/pull/"), None);
    }

    #[test]
    fn reject_non_numeric_pr_number() {
        assert_eq!(parse_pr_url("https://github.com/owner/repo/pull/abc"), None);
    }

    #[test]
    fn reject_plain_file_path() {
        assert_eq!(parse_pr_url("/tmp/my.diff"), None);
    }
}
