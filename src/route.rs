//! The browser URL of what is open, as a hash route: `#/d/<project id>/<page id>`, and no hash for
//! the home screen. Ids come from the manifest, so a link survives a rename. A hash route needs no
//! server rewrite and stays clear of the relay's `/p/` and `/s/` paths.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Home,
    Design {
        project: String,
        page: Option<String>,
    },
}

/// The route in `hash` (with or without the leading `#`); None for a hash that is not a route
/// (`#demo`, anchors), which the app leaves alone.
pub fn parse(hash: &str) -> Option<Route> {
    let path = hash.strip_prefix('#').unwrap_or(hash);
    if path.is_empty() || path == "/" {
        return Some(Route::Home);
    }
    let mut parts = path.strip_prefix("/d/")?.split('/');
    let project = parts.next().filter(|p| valid_id(p))?.to_owned();
    let page = match parts.next() {
        None | Some("") => None,
        Some(p) if valid_id(p) => Some(p.to_owned()),
        Some(_) => return None,
    };
    parts
        .next()
        .is_none()
        .then_some(Route::Design { project, page })
}

/// The hash for `route`: empty for home.
pub fn format(route: &Route) -> String {
    match route {
        Route::Home => String::new(),
        Route::Design {
            project,
            page: None,
        } => format!("#/d/{project}"),
        Route::Design {
            project,
            page: Some(page),
        } => format!("#/d/{project}/{page}"),
    }
}

/// Manifest ids are hex (`new_id`) or short slugs; anything else in a URL is not ours.
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design(project: &str, page: Option<&str>) -> Route {
        Route::Design {
            project: project.into(),
            page: page.map(Into::into),
        }
    }

    #[test]
    fn a_design_route_round_trips_with_and_without_its_page() {
        for r in [
            design("p-1", None),
            design("p-1", Some("page_2")),
            Route::Home,
        ] {
            assert_eq!(parse(&format(&r)), Some(r));
        }
        assert_eq!(parse("#/d/abc/"), Some(design("abc", None)));
        assert_eq!(parse("#/"), Some(Route::Home));
    }

    #[test]
    fn hashes_that_are_not_routes_are_left_alone() {
        for h in [
            "#demo",
            "#/x/abc",
            "#/d/",
            "#/d/a/b/c",
            "#/d/../etc",
            "#/d/a%20b",
            "#/d/a/b?c",
        ] {
            assert_eq!(parse(h), None, "{h}");
        }
    }
}
