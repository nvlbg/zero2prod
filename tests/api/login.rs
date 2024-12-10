use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange
    let app = spawn_app().await;

    // Act - part 1 - Try to login
    let login_body = serde_json::json!({
        "username": "random-username",
        "password": "random-password"
    });
    let response = app.post_login(&login_body).await;

    // Assert - part 1
    assert_is_redirect_to(&response, "/login");

    // Act - part 2 - Follow the redirect
    let html_page = app.get_login_html().await;
    
    // Assert - part 2
    assert!(html_page.contains(r#"<p><i>Invalid credentials</i></p>"#));

    // Act - part 3 - Reload the login page
    let html_page = app.get_login_html().await;

    // Assert - part 3
    assert!(!html_page.contains(r#"<p><i>Invalid credentials</i></p>"#));
}

#[tokio::test]
async fn redirect_to_admin_dashboard_after_successful_login() {
    // Arrange
    let app = spawn_app().await;
    let login_body = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    });

    // Act - part 1 - Try to login
    let response = app.post_login(&login_body).await;

    // Assert - part 1
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act - part 2 - Follow the redirect
    let html_page = app.get_admin_dashboard_html().await;
    
    // Assert - part 2
    assert!(html_page.contains(&format!("Welcome {}!", app.test_user.username)));
}