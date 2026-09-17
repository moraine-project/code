use super::*;

pub(super) async fn send_verification(state: &AppState, user_id: &str, email: &str) -> Result<(), String> {
	let Some(mailer) = state.capability.mailer.as_ref() else {
		return Ok(());
	};
	let Some(base) = state.capability.public_url.as_deref() else {
		return Err("MORAINE_PUBLIC_URL is not set, so the verification link cannot be built".to_string());
	};
	let token = random_token();
	let expires = now() + VERIFICATION_SECONDS;
	state
		.metadata
		.replace_email_verification(user_id, &token_hash(&token), now(), expires)
		.await
		.map_err(|error| error.to_string())?;
	let link = format!("{base}/account?verify={token}");
	mailer
		.send(
			email,
			"Verify your Moraine account",
			format!("Open this link to verify your email:\n\n{link}\n\nThe link expires in 24 hours.\n"),
		)
		.await
}

#[derive(Deserialize)]
pub(super) struct VerifyToken {
	token: String,
}

pub(super) async fn verify_email(State(state): State<AppState>, Json(request): Json<VerifyToken>) -> Response {
	let Some(user_id) = (match state
		.metadata
		.consume_email_verification(&token_hash(request.token.trim()), now())
		.await
	{
		Ok(user_id) => user_id,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::UNAUTHORIZED, "invalid or expired verification link").into_response();
	};
	match state.metadata.set_user_verified(&user_id, now()).await {
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => storage_error(error),
	}
}

pub(super) async fn resend_verification(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if user.session_id.is_none() {
		return (StatusCode::FORBIDDEN, "resend from a signed-in session").into_response();
	}
	let record = match state.metadata.user_by_id(&user.user_id).await {
		Ok(Some(record)) => record,
		Ok(None) => return (StatusCode::UNAUTHORIZED, "account no longer exists").into_response(),
		Err(error) => return storage_error(error),
	};
	if record.verified_at.is_some() || !state.capability.email_verification {
		return StatusCode::NO_CONTENT.into_response();
	}
	match send_verification(&state, &record.id, &record.email).await {
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => (StatusCode::SERVICE_UNAVAILABLE, error).into_response(),
	}
}

pub(crate) async fn verified_or_error(state: &AppState, user_id: &str) -> Option<Response> {
	if !state.capability.require_verified_email {
		return None;
	}
	match state.metadata.user_verified(user_id).await {
		Ok(true) => None,
		_ => Some((StatusCode::FORBIDDEN, "verify your email before publishing").into_response()),
	}
}
