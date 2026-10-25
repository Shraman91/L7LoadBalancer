use crate::config::RouteRuleConfig;

#[derive(Debug, Clone)]
pub struct RouteRule {
    pub host: Option<String>,
    pub path_prefix: String,
    pub pool: String,
    pub strip_prefix: bool,
    pub rewrite_prefix: Option<String>,
}

impl RouteRule {
    pub fn from_config(cfg: &RouteRuleConfig) -> Self {
        Self {
            host: cfg.host.as_ref().map(|h| h.to_ascii_lowercase()),
            path_prefix: cfg.path_prefix.clone(),
            pool: cfg.pool.clone(),
            strip_prefix: cfg.strip_prefix.unwrap_or(false),
            rewrite_prefix: cfg.rewrite_prefix.clone(),
        }
    }

    pub fn matches_host(&self, req_host: Option<&str>) -> bool {
        match (&self.host, req_host) {
            (None, _) => true,
            (Some(rule_host), Some(req_h)) => {
                let host_only = req_h.split(':').next().unwrap_or("").to_ascii_lowercase();
                rule_host == &host_only || rule_host == "*"
            }
            (Some(_), None) => false,
        }
    }

    pub fn matches_path(&self, req_path: &str) -> bool {
        req_path.starts_with(&self.path_prefix)
    }

    pub fn rewrite_path(&self, original_path_and_query: &str) -> String {
        let (path, query) = match original_path_and_query.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (original_path_and_query, None),
        };

        let new_path = if let Some(ref rewrite) = self.rewrite_prefix {
            let suffix = &path[self.path_prefix.len()..];
            let clean_suffix = suffix.trim_start_matches('/');
            let clean_rewrite = rewrite.trim_end_matches('/');
            if clean_suffix.is_empty() {
                if clean_rewrite.is_empty() {
                    "/".to_string()
                } else if !clean_rewrite.starts_with('/') {
                    format!("/{}", clean_rewrite)
                } else {
                    clean_rewrite.to_string()
                }
            } else if clean_rewrite.is_empty() {
                format!("/{}", clean_suffix)
            } else {
                format!("{}/{}", clean_rewrite, clean_suffix)
            }
        } else if self.strip_prefix {
            let suffix = &path[self.path_prefix.len()..];
            if suffix.is_empty() || !suffix.starts_with('/') {
                format!("/{}", suffix.trim_start_matches('/'))
            } else {
                suffix.to_string()
            }
        } else {
            path.to_string()
        };

        let normalized_path = if !new_path.starts_with('/') {
            format!("/{}", new_path)
        } else {
            new_path
        };

        match query {
            Some(q) => format!("{}?{}", normalized_path, q),
            None => normalized_path,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Router {
    pub routes: Vec<RouteRule>,
    pub default_pool: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RouteResult<'a> {
    pub pool: &'a str,
    pub path_and_query: String,
}

impl Router {
    pub fn new(routes: &[RouteRuleConfig], default_pool: String) -> Self {
        Self {
            routes: routes.iter().map(RouteRule::from_config).collect(),
            default_pool,
        }
    }

    pub fn route<'a>(
        &'a self,
        req_host: Option<&str>,
        req_path_and_query: &str,
    ) -> RouteResult<'a> {
        let req_path = req_path_and_query.split('?').next().unwrap_or("/");

        let mut best_match: Option<&'a RouteRule> = None;

        for rule in &self.routes {
            if rule.matches_host(req_host) && rule.matches_path(req_path) {
                match best_match {
                    None => {
                        best_match = Some(rule);
                    }
                    Some(current) => {
                        // Longest-prefix wins
                        if rule.path_prefix.len() > current.path_prefix.len() {
                            best_match = Some(rule);
                        } else if rule.path_prefix.len() == current.path_prefix.len() {
                            // Specific host wins over wildcard/None
                            if rule.host.is_some() && current.host.is_none() {
                                best_match = Some(rule);
                            }
                        }
                    }
                }
            }
        }

        if let Some(rule) = best_match {
            RouteResult {
                pool: &rule.pool,
                path_and_query: rule.rewrite_path(req_path_and_query),
            }
        } else {
            RouteResult {
                pool: &self.default_pool,
                path_and_query: req_path_and_query.to_string(),
            }
        }
    }
}
