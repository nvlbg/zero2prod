use wiremock::{
    matchers::{any, method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::{assert_is_redirect_to, spawn_app, ConfirmationLinks, TestApp};

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    create_unconfirmed_subscriber(&app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        // Assert that no email will be sent out
        .expect(0)
        .mount(&app.email_server)
        .await;

    // Act part 1 - login
    let response = app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    })).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act part 2 - send newsletter
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content_text": "Text content",
        "content_html": "<p>Html content</p>"
    });
    let response = app.post_newsletters(&newsletter_request_body).await;

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    // Mock verifies on Drop that we haven't sent the newsletter email
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act part 1 - login
    let response = app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    })).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act part 2 - send newsletter
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content_text": "Text content",
        "content_html": "<p>Html content</p>"
    });
    let response = app.post_newsletters(&newsletter_request_body).await;

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    // Mock verifies on Dop that we have sent the newsletter email
}

#[tokio::test]
async fn newsletters_returns_400_for_invalid_data() {
    // Arrange
    let app = spawn_app().await;
    let test_cases = vec![
        (
            serde_json::json!({
                "content_text": "Text content",
                "content_html": "<p>Html content</p>"
            }),
            "missing title",
        ),
        (
            serde_json::json!({
                "title": "Title"
            }),
            "missing content",
        ),
    ];

    // Act part 1 - login
    let response = app.post_login(&serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password
    })).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    for (invalid_body, error_message) in test_cases {
        // Act part 2 - try to send newsletter
        let response = app.post_newsletters(&invalid_body).await;

        // Assert
        assert_eq!(
            response.status().as_u16(),
            400,
            "The API did not fail with 400 when the payload was {}",
            error_message
        );
    }
}

#[tokio::test]
async fn cannot_send_newsletter_without_logging_in_first() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.api_client
        .post(format!("{}/admin/newsletters", &app.address))
        .json(&serde_json::json!({
            "title": "Title",
            "content_text": "Text content",
            "content_html": "<p>Html content</p>"
        }))
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert_is_redirect_to(&response, "/login");
}

async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let body = "name=Le%20Guin&email=ursula_le_guin%40gmail.com";

    let _mock_guard = Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;

    app.post_subscriptions(body.into())
        .await
        .error_for_status()
        .unwrap();

    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();
    app.get_confirmation_links(email_request)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    let confirmation_links = create_unconfirmed_subscriber(app).await;
    reqwest::get(confirmation_links.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}
