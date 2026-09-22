use std::sync::Arc;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::routes::AppState;

pub(super) fn fallback(app: Router<AppState>, web_dir: Option<Arc<std::path::PathBuf>>) -> Router<AppState> {
	match web_dir {
		Some(directory) => {
			let index = directory.join("index.html");
			app.fallback_service(ServeDir::new(&*directory).fallback(ServeFile::new(index)))
		}
		None => app,
	}
}
