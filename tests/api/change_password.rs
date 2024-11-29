use uuid::Uuid;

use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn you_must_be_logged_in_to_see_the_change_password_form() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get_change_password().await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_be_logged_in_to_change_your_password() {
    // Arrange
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();

    // Act
    let response = app.post_change_password(&serde_json::json!({
        "current_password": Uuid::new_v4().to_string(),
        "new_password": &new_password,
        "new_password_check": &new_password
    })).await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn new_password_fields_must_match() {
    // Arrange
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();
    let another_new_password = Uuid::new_v4().to_string();

    // Act part 1 - login
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    })).await;

    // Act part 2 - try to change password
    let response = app.post_change_password(&serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": &new_password,
        "new_password_check": &another_new_password
    })).await;
    assert_is_redirect_to(&response, "/admin/password");

    // Act part 3 - follow redirect
    let html = app.get_change_password_html().await;
    assert!(html.contains("You entered two different new passwords - the field values must match."));
}

#[tokio::test]
async fn current_password_must_be_valid() {
    // Arrange
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();
    let wrong_password = Uuid::new_v4().to_string();

    // Act part 1 - login
    app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    })).await;

    // Act part 2 - try to change password
    let response = app.post_change_password(&serde_json::json!({
        "current_password": &wrong_password,
        "new_password": &new_password,
        "new_password_check": &new_password
    })).await;
    assert_is_redirect_to(&response, "/admin/password");

    // Act part 3 - follow redirect
    let html = app.get_change_password_html().await;
    assert!(html.contains("The current password is incorrect"));
}

#[tokio::test]
async fn logout_clears_session_state() {
    // Arrange
    let app = spawn_app().await;

    // Act part 1 - login
    let response = app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    })).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act part 2 - follow the redirect
    let html = app.get_admin_dashboard_html().await;
    assert!(html.contains(&format!("Welcome {}!", app.test_user.username)));

    // Act part 3 - logout
    let response = app.post_logout().await;
    assert_is_redirect_to(&response, "/login");

    // Act part 4 - follow the redirect
    let html = app.get_login_html().await;
    assert!(html.contains("You have successfully logged out"));

    // Act part 5 - try to get admin dashboard
    let response = app.get_admin_dashboard().await;
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn changing_password_works() {
    // Arrange
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();

    // Act part 1 - login
    let response = app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    })).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act part 2 - try to change password
    let response = app.post_change_password(&serde_json::json!({
        "current_password": &app.test_user.password,
        "new_password": &new_password,
        "new_password_check": &new_password
    })).await;
    assert_is_redirect_to(&response, "/admin/password");

    // Act part 3 - follow redirect
    let html = app.get_change_password_html().await;
    assert!(html.contains("You have successfully changed your password"));

    // Act part 4 - logout
    let response = app.post_logout().await;
    assert_is_redirect_to(&response, "/login");

    // Act part 5 - follow the redirect
    let html = app.get_login_html().await;
    assert!(html.contains("You have successfully logged out"));

    // Act part 6 - login using new password
    let response = app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &new_password
    })).await;
    assert_is_redirect_to(&response, "/admin/dashboard");
}
