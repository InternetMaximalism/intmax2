use thiserror::Error;
use url::Url;

/// Error type for URL parsing operations
#[derive(Error, Debug)]
pub enum UrlParseError {
    #[error("Invalid URL '{url}': {source}")]
    InvalidUrl {
        url: String,
        #[source]
        source: url::ParseError,
    },
}

/// Parses a comma-separated string of URLs into a `Vec<String>`
/// Each URL is trimmed of whitespace and validated using the url crate
///
/// # Examples
/// ```
/// use server_common::parser::parse_urls;
///
/// let urls = parse_urls("http://example.com, https://test.org,http://localhost:8080").unwrap();
/// assert_eq!(urls, vec![
///     "http://example.com".to_string(),
///     "https://test.org".to_string(),
///     "http://localhost:8080".to_string()
/// ]);
/// ```
///
/// # Errors
/// Returns a `UrlParseError` if any of the URLs is invalid
pub fn parse_urls(urls_string: &str) -> Result<Vec<String>, UrlParseError> {
    let mut result = Vec::new();

    for url_str in urls_string.split(',') {
        let url_str = url_str.trim();
        if url_str.is_empty() {
            continue;
        }

        // Validate URL
        match Url::parse(url_str) {
            Ok(_) => result.push(url_str.to_string()),
            Err(err) => {
                return Err(UrlParseError::InvalidUrl {
                    url: url_str.to_string(),
                    source: err,
                })
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_urls() {
        // Basic case with spaces after comma
        let result =
            parse_urls("http://example.com, https://test.org, http://localhost:8080").unwrap();
        assert_eq!(
            result,
            vec![
                "http://example.com".to_string(),
                "https://test.org".to_string(),
                "http://localhost:8080".to_string()
            ]
        );

        // No spaces
        let result =
            parse_urls("http://example.com,https://test.org,http://localhost:8080").unwrap();
        assert_eq!(
            result,
            vec![
                "http://example.com".to_string(),
                "https://test.org".to_string(),
                "http://localhost:8080".to_string()
            ]
        );

        // Extra spaces
        let result = parse_urls("  http://example.com  ,  https://test.org  ").unwrap();
        assert_eq!(
            result,
            vec![
                "http://example.com".to_string(),
                "https://test.org".to_string()
            ]
        );

        // Empty entries are filtered out
        let result = parse_urls("http://example.com,,https://test.org").unwrap();
        assert_eq!(
            result,
            vec![
                "http://example.com".to_string(),
                "https://test.org".to_string()
            ]
        );

        // Empty string returns empty vector
        let result = parse_urls("").unwrap();
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_invalid_urls() {
        // Invalid URL format
        let result = parse_urls("http://example.com, invalid-url, http://test.org");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid URL 'invalid-url'"));

        // Missing scheme
        let result = parse_urls("example.com");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid URL 'example.com'"));
    }
}
