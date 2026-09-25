use regex::Regex;

use crate::config::Config;

pub fn domain_matches(config: &Config, name: &str) -> bool {
    let normalized = name.trim_end_matches('.').to_ascii_lowercase();
    if config
        .exclude_domains
        .iter()
        .any(|domain| domain_match(&normalized, domain))
    {
        return false;
    }
    let included = (config.domain_filter.is_empty() && config.regex_include.is_none())
        || config
            .domain_filter
            .iter()
            .any(|domain| domain_match(&normalized, domain));
    let regex_included = config.regex_include.as_deref().is_none_or(|pattern| {
        Regex::new(pattern)
            .map(|r| r.is_match(name))
            .unwrap_or(false)
    });
    let regex_excluded = config.regex_exclude.as_deref().is_some_and(|pattern| {
        Regex::new(pattern)
            .map(|r| r.is_match(name))
            .unwrap_or(false)
    });
    (included || (config.regex_include.is_some() && regex_included)) && !regex_excluded
}

fn domain_match(name: &str, configured: &str) -> bool {
    let domain = configured.trim_end_matches('.').to_ascii_lowercase();
    if domain.starts_with('.') {
        name.ends_with(&domain)
    } else {
        name == domain || name.ends_with(&format!(".{domain}"))
    }
}
