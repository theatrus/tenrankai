use super::*;

#[test]
fn test_user_lookup_by_email() {
    let mut db = UserDatabase::new();

    // Add a test user
    db.add_user("alice".to_string(), User {
        email: "alice@example.com".to_string(),
        passkeys: Vec::new(),
    });

    // Test lookup by username
    let user = db.get_user_by_username_or_email("alice");
    assert!(user.is_some());
    assert_eq!(user.unwrap().email, "alice@example.com");

    // Test lookup by email (exact case)
    let user = db.get_user_by_username_or_email("alice@example.com");
    assert!(user.is_some());
    assert_eq!(user.unwrap().email, "alice@example.com");

    // Test lookup by email (case insensitive)
    let user = db.get_user_by_username_or_email("Alice@Example.com");
    assert!(user.is_some());
    assert_eq!(user.unwrap().email, "alice@example.com");

    // Test non-existent user
    let user = db.get_user_by_username_or_email("bob");
    assert!(user.is_none());

    // Test non-existent email
    let user = db.get_user_by_username_or_email("bob@example.com");
    assert!(user.is_none());
}
