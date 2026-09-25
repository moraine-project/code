use super::*;

#[derive(Deserialize)]
pub(super) struct PasswordChange {
	current: String,
	new: String,
}

pub(super) async fn change_password(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<PasswordChange>,
) -> Response {
	let Some(session_id) = user.session_id.clone() else {
		return (StatusCode::FORBIDDEN, "change a password from a signed-in session").into_response();
	};
	if request.new.len() < MIN_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "the new password is too short").into_response();
	}
	if request.new.len() > MAX_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "the new password is too long").into_response();
	}
	let record = match state.metadata.user_by_id(&user.user_id).await {
		Ok(Some(record)) => record,
		Ok(None) => return (StatusCode::UNAUTHORIZED, "account no longer exists").into_response(),
		Err(error) => return storage_error(error),
	};
	if !password::verify_password(&request.current, &record.password_hash) {
		return (StatusCode::UNAUTHORIZED, "the current password is wrong").into_response();
	}
	let Ok(hash) = password::hash_password(&request.new) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	if let Err(error) = state.metadata.update_password(&user.user_id, &hash, now()).await {
		return storage_error(error);
	}
	if let Err(error) = state.metadata.revoke_other_sessions(&user.user_id, &session_id, now()).await {
		return storage_error(error);
	}
	StatusCode::NO_CONTENT.into_response()
}

pub(super) async fn issue_recovery_codes(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if user.session_id.is_none() {
		return (StatusCode::FORBIDDEN, "issue recovery codes from a signed-in session").into_response();
	}
	let codes: Vec<String> = (0..RECOVERY_CODE_COUNT).map(|_| random_token()).collect();
	let hashes: Vec<Vec<u8>> = codes.iter().map(|code| token_hash(code).to_vec()).collect();
	if let Err(error) = state.metadata.replace_recovery_codes(&user.user_id, &hashes, now()).await {
		return storage_error(error);
	}
	Json(serde_json::json!({ "codes": codes })).into_response()
}

#[derive(Deserialize)]
pub(super) struct RecoverRequest {
	email: String,
	code: String,
	new: String,
}

pub(super) async fn recover(State(state): State<AppState>, Json(request): Json<RecoverRequest>) -> Response {
	let email = request.email.trim().to_lowercase();
	if request.new.len() < MIN_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "the new password is too short").into_response();
	}
	if request.new.len() > MAX_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "the new password is too long").into_response();
	}
	let Some(record) = (match state.metadata.user_by_email(&email).await {
		Ok(record) => record,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::UNAUTHORIZED, "invalid recovery code").into_response();
	};
	let hash = token_hash(request.code.trim());
	let Ok(password_hash) = password::hash_password(&request.new) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	let claimed = match state
		.metadata
		.recover_password(&record.id, &hash, &password_hash, now())
		.await
	{
		Ok(claimed) => claimed,
		Err(error) => return storage_error(error),
	};
	if !claimed {
		return (StatusCode::UNAUTHORIZED, "invalid recovery code").into_response();
	}
	StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
pub(super) struct ResetPasswordRequest {
	email: String,
}

pub(super) async fn reset_password(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<ResetPasswordRequest>,
) -> Response {
	if !user.allows("directory:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let email = request.email.trim().to_lowercase();
	let Some(record) = (match state.metadata.user_by_email(&email).await {
		Ok(record) => record,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::NOT_FOUND, "no account with that email").into_response();
	};
	let temporary = random_token();
	let Ok(password_hash) = password::hash_password(&temporary) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	if let Err(error) = state.metadata.update_password(&record.id, &password_hash, now()).await {
		return storage_error(error);
	}
	if let Err(error) = state.metadata.revoke_sessions(&record.id, now()).await {
		return storage_error(error);
	}
	Json(serde_json::json!({ "password": temporary })).into_response()
}
