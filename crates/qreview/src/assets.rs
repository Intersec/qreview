//! The interface, embedded in the binary.
//!
//! A release build carries `web/dist` inside the executable. A debug build
//! reads the directory from disk, so `make dev` needs no rebuild when a
//! component changes.

use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../web/dist"]
pub struct Assets;

/// The asset for a request path, with the type the browser needs.
///
/// A path that names no asset falls back to `index.html`, because the
/// interface routes on the client side.
pub fn get(path: &str) -> Option<(Vec<u8>, String)> {
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // The type follows the file that is served, not the one that was asked
    // for. A fallback that answers index.html as octet-stream downloads the
    // page instead of showing it.
    let (served, file) = match Assets::get(path) {
        Some(file) => (path, file),
        None => ("index.html", Assets::get("index.html")?),
    };
    let mime = mime_guess::from_path(served).first_or_octet_stream();

    Some((file.data.into_owned(), mime.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_embedded() {
        let (body, mime) = get("/").expect("the interface must be built before the binary");
        assert!(!body.is_empty());
        assert_eq!(mime, "text/html");
    }

    #[test]
    fn an_unknown_path_falls_back_to_the_interface() {
        let (_, mime) = get("/changes/I8f3a").expect("the fallback must serve index.html");
        assert_eq!(mime, "text/html");
    }
}
