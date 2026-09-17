use super::*;

fn account_json(account: &crate::auth::accounts::AccountRow) -> serde_json::Value {
	serde_json::json!({
		"user_id": account.id,
		"email": account.email,
		"role": account.role,
		"created_at": account.created_at,
		"verified": account.verified_at.is_some(),
	})
}

pub(super) async fn list_accounts(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("directory:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.list_users(500).await {
		Ok(accounts) => {
			let view = accounts.iter().map(account_json).collect::<Vec<_>>();
			Json(view).into_response()
		}
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
pub(super) struct NewAccount {
	email: String,
	#[serde(default)]
	role: Option<String>,
}

pub(super) async fn create_account(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<NewAccount>,
) -> Response {
	if !user.allows("directory:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let email = request.email.trim().to_lowercase();
	if !email.contains('@') {
		return (StatusCode::BAD_REQUEST, "invalid email").into_response();
	}
	let role = request.role.as_deref().unwrap_or("member");
	if !matches!(role, "member" | OPERATOR_ROLE) {
		return (StatusCode::BAD_REQUEST, "role must be member or operator").into_response();
	}
	match state.metadata.user_by_email(&email).await {
		Ok(Some(_)) => return (StatusCode::CONFLICT, "email already registered").into_response(),
		Ok(None) => {}
		Err(error) => return storage_error(error),
	}
	let temporary = random_token();
	let Ok(password_hash) = password::hash_password(&temporary) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	let user_id = new_id();
	if let Err(error) = state
		.metadata
		.create_user(&user_id, &email, &password_hash, role, now())
		.await
	{
		return storage_error(error);
	}
	let _ = state.metadata.set_user_verified(&user_id, now()).await;
	(
		StatusCode::CREATED,
		Json(serde_json::json!({ "user_id": user_id, "password": temporary })),
	)
		.into_response()
}

pub(super) async fn delete_account(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(id): Path<String>,
) -> Response {
	if !user.allows("directory:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.last_owner_orgs(&id).await {
		Ok(orgs) if !orgs.is_empty() => {
			return (
				StatusCode::CONFLICT,
				format!("the account is the last owner of: {}", orgs.join(", ")),
			)
				.into_response();
		}
		Ok(_) => {}
		Err(error) => return storage_error(error),
	}
	match state.metadata.delete_user(&id).await {
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => storage_error(error),
	}
}

pub(super) async fn delete_self(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if user.session_id.is_none() {
		return (StatusCode::FORBIDDEN, "delete an account from a signed-in session").into_response();
	}
	match state.metadata.last_owner_orgs(&user.user_id).await {
		Ok(orgs) if !orgs.is_empty() => {
			return (
				StatusCode::CONFLICT,
				format!("you are the last owner of: {}; transfer them first", orgs.join(", ")),
			)
				.into_response();
		}
		Ok(_) => {}
		Err(error) => return storage_error(error),
	}
	match state.metadata.delete_user(&user.user_id).await {
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => storage_error(error),
	}
}

pub(super) async fn export_account(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if user.session_id.is_none() {
		return (StatusCode::FORBIDDEN, "export an account from a signed-in session").into_response();
	}
	let account = match state.metadata.user_by_id(&user.user_id).await {
		Ok(Some(account)) => account,
		Ok(None) => return (StatusCode::UNAUTHORIZED, "account no longer exists").into_response(),
		Err(error) => return storage_error(error),
	};
	let orgs = state
		.metadata
		.orgs_for_user(&user.user_id)
		.await
		.map(|orgs| {
			orgs.into_iter()
				.map(|org| serde_json::json!({ "handle": org.handle, "role": org.role }))
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let keys = state
		.metadata
		.api_keys_for_user(&user.user_id)
		.await
		.map(|keys| {
			keys.into_iter()
				.map(|key| {
					serde_json::json!({
						"name": key.name,
						"prefix": key.prefix,
						"scopes": key.scopes,
						"created_at": key.created_at,
						"expires_at": key.expires_at,
					})
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let follows = state.metadata.follows(&user.user_id).await.unwrap_or_default();
	let submissions = state
		.metadata
		.submissions_by_submitter(&user.user_id, 500, None)
		.await
		.map(|rows| {
			rows.into_iter()
				.map(|row| {
					serde_json::json!({
						"id": row.id,
						"project_id": row.project_id,
						"state": row.state,
						"created_at": row.created_at,
					})
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	Json(serde_json::json!({
		"account": {
			"user_id": account.id,
			"email": account.email,
			"role": account.role,
			"created_at": account.created_at,
			"verified": account.verified_at.is_some(),
		},
		"organizations": orgs,
		"api_keys": keys,
		"follows": follows,
		"submissions": submissions,
	}))
	.into_response()
}
