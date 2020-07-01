//! `Middleware` to clean request's URI, and redirect if necessary.
//!
//! Performs following:
//!
//! - Merges multiple `/` into one.
//! - Resolves and eliminates `..` and `.` if any.
//! - Appends a trailing `/` if one is not present, and there is no file extension.
//!
//! It will respond with a permanent redirect if the path was cleaned.
//!
//! ```rust
//! # fn main() {
//! let app = tide::new()
//!     .middleware(tide_clean_path::CleanPath)
//!     .at("/").get(|_| async { Ok("") });
//! # }
//! ```
use std::future::Future;
use std::pin::Pin;
use tide::{Middleware, Next, Redirect, Request, Result};

pub struct CleanPath;

impl<State: Send + Sync + 'static> Middleware<State> for CleanPath {
    fn handle<'a>(
        &'a self,
        req: Request<State>,
        next: Next<'a, State>,
    ) -> Pin<Box<dyn Future<Output = Result> + Send + 'a>> {
        Box::pin(async move {
            let original_path = req.url().path();
            let trailing_slash = original_path.ends_with('/');

            // non-allocating fast path
            if !original_path.contains("/.")
                && !original_path.contains("//")
                && (has_ext(original_path) ^ trailing_slash)
            {
                return next.run(req).await;
            }

            let mut path = path_clean::clean(&original_path);
            if path != "/" {
                if trailing_slash || !has_ext(&path) {
                    path.push('/');
                }
            }

            if path != original_path {
                let mut new_url = req.url().clone();
                new_url.set_path(&path);
                return Ok(Redirect::permanent(new_url).into());
            }

            next.run(req).await
        })
    }
}

fn has_ext(path: &str) -> bool {
    path.rfind('.')
        .map(|index| {
            let sub = &path[index + 1..];
            !sub.is_empty() && !sub.contains('/')
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::CleanPath;
    use tide::http::{self, url::Url, Method};

    fn app() -> tide::Server<()> {
        let mut app = tide::new();
        app.middleware(CleanPath);
        app.at("/").get(|_| async { Ok("") });
        app.at("/*p").get(|_| async { Ok("") });
        app
    }

    #[async_std::test]
    async fn test_clean() {
        let app = app();
        let cases = vec![
            //("/.", "/"),
            //("/..", "/"),
            //("/..//..", "/"),
            //("/./", "/"),
            ("//", "/"),
            ("///", "/"),
            ("///?a=1", "/?a=1"),
            ("///?a=1&b=2", "/?a=1&b=2"),
            ("//?a=1", "/?a=1"),
            ("//a//b//", "/a/b/"),
            ("//a//b//.", "/a/b/"),
            // ("//a//b//../", "/a/"),
            ("//a//b//./", "/a/b/"),
            ("//m.js", "/m.js"),
            ("/a//b", "/a/b/"),
            ("/a//b/", "/a/b/"),
            ("/a//b//", "/a/b/"),
            ("/a//m.js", "/a/m.js"),
            ("/m.", "/m./"),
        ];
        for (given, clean) in cases.iter() {
            let req = http::Request::new(
                Method::Get,
                Url::parse(&format!("http://localhost{}", given)).unwrap(),
            );
            let res: http::Response = app.respond(req).await.unwrap();
            assert!(res.status().is_redirection(), "for {}", given);
            assert_eq!(
                &res.header(http::headers::LOCATION).unwrap().last().as_str(),
                &format!("http://localhost{}", clean),
                "for {}",
                given,
            );
        }
    }

    #[async_std::test]
    async fn test_pristine() {
        let app = app();
        let cases = vec!["/", "/a/", "/a/b/", "/m.js", "/m./"];
        for given in cases.iter() {
            let req = http::Request::new(
                Method::Get,
                Url::parse(&format!("http://localhost{}", given)).unwrap(),
            );
            let res: http::Response = app.respond(req).await.unwrap();
            assert!(res.status().is_success(), "for {}", given);
        }
    }
}
