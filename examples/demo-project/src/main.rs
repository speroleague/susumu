fn list_users() {
    load_users();
}

fn create_user() {
    validate_user();
    save_user();
    publish_user_created();
}

fn router() {
    Router::new()
        .route("/users", get(list_users))
        .route("/users", post(create_user));
}
