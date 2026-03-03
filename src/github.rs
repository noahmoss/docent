#[derive(Debug, PartialEq)]
pub enum GitHubUrl<'a> {
    PullRequest {
        owner: &'a str,
        repo: &'a str,
        number: &'a str,
    },
    Commit {
        owner: &'a str,
        repo: &'a str,
        sha: &'a str,
    },
    Compare {
        owner: &'a str,
        repo: &'a str,
        basehead: &'a str,
    },
}

fn strip_github_prefix(url: &str) -> Option<&str> {
    url.strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
}

fn strip_suffix_noise(s: &str) -> &str {
    let s = s.split(['?', '#']).next().unwrap_or(s);
    s.strip_suffix(".diff")
        .or_else(|| s.strip_suffix(".patch"))
        .unwrap_or(s)
}

pub fn parse_github_url(url: &str) -> Option<GitHubUrl<'_>> {
    let path = strip_github_prefix(url)?;

    let mut parts = path.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    let kind = parts.next().filter(|s| !s.is_empty())?;

    match kind {
        "pull" => {
            let number_part = parts.next().filter(|s| !s.is_empty())?;
            let number = strip_suffix_noise(number_part);
            if !number.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(GitHubUrl::PullRequest {
                owner,
                repo,
                number,
            })
        }
        "commit" => {
            let sha_part = parts.next().filter(|s| !s.is_empty())?;
            let sha = strip_suffix_noise(sha_part);
            if !sha.chars().all(|c| c.is_ascii_hexdigit()) || sha.len() < 7 {
                return None;
            }
            Some(GitHubUrl::Commit { owner, repo, sha })
        }
        "compare" => {
            let basehead_part = parts.next().filter(|s| !s.is_empty())?;
            let basehead = strip_suffix_noise(basehead_part);
            if !basehead.contains("...") && !basehead.contains("..") {
                return None;
            }
            Some(GitHubUrl::Compare {
                owner,
                repo,
                basehead,
            })
        }
        _ => None,
    }
}

pub async fn fetch_diff(url: &str) -> Result<String, String> {
    let parsed =
        parse_github_url(url).ok_or_else(|| format!("Not a recognized GitHub URL: {url}"))?;

    let (api_url, label) = match &parsed {
        GitHubUrl::PullRequest {
            owner,
            repo,
            number,
        } => (
            format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}"),
            format!("{owner}/{repo}#{number}"),
        ),
        GitHubUrl::Commit { owner, repo, sha } => (
            format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}"),
            format!("{owner}/{repo}@{}", &sha[..7.min(sha.len())]),
        ),
        GitHubUrl::Compare {
            owner,
            repo,
            basehead,
        } => (
            format!("https://api.github.com/repos/{owner}/{repo}/compare/{basehead}"),
            format!("{owner}/{repo} {basehead}"),
        ),
    };

    eprintln!("Fetching diff for {label}...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let mut request = client
        .get(&api_url)
        .header("Accept", "application/vnd.github.v3.diff")
        .header("User-Agent", "docent");

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        request = request.header("Authorization", format!("Bearer {token}"));
    }

    let response = request
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
        return Err("Diff is empty".to_string());
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- PR URLs --

    #[test]
    fn standard_pr_url() {
        assert_eq!(
            parse_github_url("https://github.com/metabase/metabase/pull/70213"),
            Some(GitHubUrl::PullRequest {
                owner: "metabase",
                repo: "metabase",
                number: "70213"
            })
        );
    }

    #[test]
    fn pr_url_with_trailing_slash() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/pull/123/"),
            Some(GitHubUrl::PullRequest { number: "123", .. })
        ));
    }

    #[test]
    fn pr_url_with_files_tab() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/pull/456/files"),
            Some(GitHubUrl::PullRequest { number: "456", .. })
        ));
    }

    #[test]
    fn diff_suffix_on_pr() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/pull/123.diff"),
            Some(GitHubUrl::PullRequest { number: "123", .. })
        ));
    }

    #[test]
    fn patch_suffix_on_pr() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/pull/123.patch"),
            Some(GitHubUrl::PullRequest { number: "123", .. })
        ));
    }

    #[test]
    fn pr_url_with_query_string() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/pull/123?tab=files"),
            Some(GitHubUrl::PullRequest { number: "123", .. })
        ));
    }

    #[test]
    fn pr_url_with_fragment() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/pull/123#discussion_r12345"),
            Some(GitHubUrl::PullRequest { number: "123", .. })
        ));
    }

    #[test]
    fn http_pr_url() {
        assert!(matches!(
            parse_github_url("http://github.com/owner/repo/pull/123"),
            Some(GitHubUrl::PullRequest { number: "123", .. })
        ));
    }

    #[test]
    fn reject_non_numeric_pr_number() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/pull/abc"),
            None
        );
    }

    // -- Commit URLs --

    #[test]
    fn commit_url_full_sha() {
        assert_eq!(
            parse_github_url(
                "https://github.com/libuv/libuv/commit/50ed2fd7bdd42830ff7327773f62540899b0e9a3"
            ),
            Some(GitHubUrl::Commit {
                owner: "libuv",
                repo: "libuv",
                sha: "50ed2fd7bdd42830ff7327773f62540899b0e9a3"
            })
        );
    }

    #[test]
    fn commit_url_short_sha() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/commit/abc1234"),
            Some(GitHubUrl::Commit { sha: "abc1234", .. })
        ));
    }

    #[test]
    fn commit_url_with_diff_suffix() {
        assert!(matches!(
            parse_github_url(
                "https://github.com/owner/repo/commit/50ed2fd7bdd42830ff7327773f62540899b0e9a3.diff"
            ),
            Some(GitHubUrl::Commit { .. })
        ));
    }

    #[test]
    fn reject_commit_url_too_short() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/commit/abc"),
            None
        );
    }

    #[test]
    fn reject_commit_url_non_hex() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/commit/xyz1234"),
            None
        );
    }

    // -- Compare URLs --

    #[test]
    fn compare_url_triple_dot() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/compare/main...feature"),
            Some(GitHubUrl::Compare {
                owner: "owner",
                repo: "repo",
                basehead: "main...feature"
            })
        );
    }

    #[test]
    fn compare_url_double_dot() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/compare/v1.0..v2.0"),
            Some(GitHubUrl::Compare {
                basehead: "v1.0..v2.0",
                ..
            })
        ));
    }

    #[test]
    fn compare_url_with_diff_suffix() {
        assert!(matches!(
            parse_github_url("https://github.com/owner/repo/compare/main...feature.diff"),
            Some(GitHubUrl::Compare { .. })
        ));
    }

    #[test]
    fn reject_compare_without_dots() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/compare/main"),
            None
        );
    }

    // -- General rejections --

    #[test]
    fn reject_non_github_url() {
        assert_eq!(
            parse_github_url("https://gitlab.com/owner/repo/pull/123"),
            None
        );
    }

    #[test]
    fn reject_issues_url() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/issues/123"),
            None
        );
    }

    #[test]
    fn reject_plain_file_path() {
        assert_eq!(parse_github_url("/tmp/my.diff"), None);
    }
}
